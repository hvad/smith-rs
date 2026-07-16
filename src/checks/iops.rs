// Bring in the BaseCheck trait and CheckResult enum from our checks module
use crate::checks::{BaseCheck, CheckResult};
// Bring in the resolved global configurations
use crate::config::AppConfig;

use std::sync::Mutex; // A synchronous Mutex used to safely protect our history metrics across check runs

// ==========================================
// 1. LINUX OS COMPILATION GATEWAY
// ==========================================
// The `#[cfg(target_os = "linux")]` attribute is a conditional compilation flag.
// It tells the Rust compiler to ONLY compile these specific imports and structures if the target OS is Linux.
// This prevents unused import warnings or missing file errors on macOS or Windows.
#[cfg(target_os = "linux")]
use std::fs::File;
#[cfg(target_os = "linux")]
use std::io::{BufRead, BufReader};
#[cfg(target_os = "linux")]
use std::time::Instant; // Used to calculate precise elapsed time durations between check intervals

#[cfg(target_os = "linux")]
/// A simple internal structure to preserve historical counters across check loops on Linux.
struct IopsHistory {
    last_read_ios: u64,
    last_write_ios: u64,
    last_check_time: Instant,
}

/// The main struct responsible for checking disk IOPS activity
pub struct IopsCheck {
    // Condition-gated data fields:
    // If compiling on Linux, we use our IopsHistory struct wrapped in a Mutex tracker.
    #[cfg(target_os = "linux")]
    history: Mutex<Option<IopsHistory>>,

    // If NOT compiling on Linux, we provide a tiny dummy boolean Mutex field.
    // This prevents the compiler from throwing a "field not found" error on macOS/Windows.
    #[cfg(not(target_os = "linux"))]
    _history: Mutex<Option<bool>>,
}

impl IopsCheck {
    /// Public constructor initialization pipeline
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            history: Mutex::new(None), // Starts as None because we have no history on the very first boot

            #[cfg(not(target_os = "linux"))]
            _history: Mutex::new(None),
        }
    }
}

// Implement the BaseCheck trait asynchronously
#[async_trait::async_trait]
impl BaseCheck for IopsCheck {
    /// Returns the human-readable name of this check
    fn name(&self) -> &'static str {
        "Disk IOPS"
    }

    /// Returns the configuration key used in the YAML file to customize this service
    fn config_key(&self) -> &'static str {
        "iops"
    }

    /// Default execution interval (60 seconds) if not overridden in the configuration
    fn default_period(&self) -> u64 {
        60
    }

    // ==========================================
    // 2. LINUX CORE RUNNER METHOD
    // ==========================================
    /// Asynchronous runner executed strictly on Linux systems.
    /// This targets the `/proc/diskstats` kernel pseudo-filesystem interface.
    #[cfg(target_os = "linux")]
    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        // Open Linux kernel pseudo-filesystem stats file.
        // If file system reading fails, safely exit early by returning None.
        let file = match File::open("/proc/diskstats") {
            Ok(f) => f,
            Err(_) => return None,
        };

        let reader = BufReader::new(file);
        let mut total_read_ios = 0;
        let mut total_write_ios = 0;

        // Parse diskstats line by line
        for line_res in reader.lines() {
            if let Ok(line) = line_res {
                // Split the line by spaces into a vector of string slices (&str)
                let fields: Vec<&str> = line.split_whitespace().collect();

                // Standard Linux /proc/diskstats structure contains at least 14 fields for major disks.
                // Field index 3 represents completed reads, index 7 represents completed writes.
                if fields.len() >= 14 {
                    let disk_name = fields[2];

                    // Aggregate stats for primary drive partitions only (e.g., sda, nvme0n1),
                    // skipping sub-partitions (e.g., sda1, nvme0n1p1) to avoid duplicate metrics.
                    if disk_name.starts_with("sd") && disk_name.len() == 3
                        || disk_name.starts_with("nvme") && !disk_name.contains('p')
                    {
                        total_read_ios += fields[3].parse::<u64>().unwrap_or(0);
                        total_write_ios += fields[7].parse::<u64>().unwrap_or(0);
                    }
                }
            }
        }

        let current_time = Instant::now();

        // Lock our Mutex window safely to read and update our history registry.
        // unwrap() is used because if another thread panicked while holding this lock,
        // the Mutex is poisoned and our application should safely crash.
        let mut history_guard = self.history.lock().unwrap();

        // Match over the current contents of the Mutex history state
        let result = match &*history_guard {
            Some(history) => {
                // Calculate time difference between the last execution and now as a float
                let elapsed_secs = current_time
                    .duration_since(history.last_check_time)
                    .as_secs_f64();

                if elapsed_secs > 0.0 {
                    // Compute absolute difference in operation counters.
                    // saturating_sub ensures that if counters wrap around, it clips safely to 0 instead of panicking.
                    let reads_diff = total_read_ios.saturating_sub(history.last_read_ios) as f64;
                    let writes_diff = total_write_ios.saturating_sub(history.last_write_ios) as f64;

                    // Divide operations by elapsed seconds to calculate rates per second (IOPS)
                    let read_iops = reads_diff / elapsed_secs;
                    let write_iops = writes_diff / elapsed_secs;
                    let total_iops = read_iops + write_iops;

                    // Gather configuration threshold benchmarks from YAML
                    let (warn, crit) = config
                        .services
                        .get(self.config_key())
                        .map(|sc| (sc.warning, sc.critical))
                        .unwrap_or((2000.0, 5000.0));

                    // Evaluate metric bounds against warning and critical threshold rules
                    let mut status = "OK".to_string();
                    if total_iops >= crit {
                        status = "CRITICAL".to_string();
                    } else if total_iops >= warn {
                        status = "WARNING".to_string();
                    }

                    Some(CheckResult::Single {
                        status,
                        message: format!(
                            "Total IOPS: {:.1} (Read: {:.1}/s, Write: {:.1}/s)",
                            total_iops, read_iops, write_iops
                        ),
                    })
                } else {
                    None
                }
            }
            None => {
                // First initialization cycle: we have no past context to subtract from yet.
                // We return a baseline notice, and actual rates will compute normally on the next run.
                Some(CheckResult::Single {
                    status: "OK".to_string(),
                    message: "Initializing IOPS baseline counters... Calculating rates on next loop iteration.".to_string(),
                })
            }
        };

        // Save current counters to historical storage to prepare for the next monitoring cycle
        *history_guard = Some(IopsHistory {
            last_read_ios: total_read_ios,
            last_write_ios: total_write_ios,
            last_check_time: current_time,
        });

        result
    }

    // ==========================================
    // 3. NON-LINUX FALLBACK RUNNER METHOD
    // ==========================================
    /// Asynchronous fallback runner executed on non-Linux platforms (like macOS/Windows).
    /// Returning 'None' tells the central scheduler loop to skip this specific check
    /// entirely without throwing warnings or crashing the background agent service.
    #[cfg(not(target_os = "linux"))]
    async fn run(&self, _config: &AppConfig) -> Option<CheckResult> {
        None
    }
}
