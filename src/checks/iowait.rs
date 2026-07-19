// Bring in the BaseCheck trait and CheckResult enum from our checks module
use crate::checks::{BaseCheck, CheckResult};
// Bring in the resolved global configurations
use crate::config::AppConfig;

use std::sync::Mutex; // A synchronous Mutex used to safely protect our history metrics across check runs

// ==========================================
// 1. LINUX OS COMPILATION GATEWAY
// ==========================================
// The `#[cfg(target_os = "linux")]` attribute ensures that these imports and the
// storage structures are only processed when compiling on Linux.
// On macOS, these lines are completely ignored, preventing compilation warnings or errors.
#[cfg(target_os = "linux")]
use std::fs::File;
#[cfg(target_os = "linux")]
use std::io::{BufRead, BufReader};

#[cfg(target_os = "linux")]
/// A structure to store previous CPU time ticks so we can calculate the delta change.
struct IoWaitHistory {
    last_iowait_ticks: u64,
    last_total_ticks: u64,
}

/// The main struct responsible for checking CPU I/O Wait percentage
pub struct IoWaitCheck {
    // Condition-gated data fields:
    #[cfg(target_os = "linux")]
    history: Mutex<Option<IoWaitHistory>>,

    #[cfg(not(target_os = "linux"))]
    _history: Mutex<Option<bool>>, // Fallback field to keep the struct size and compilation valid on macOS
}

impl IoWaitCheck {
    /// Public constructor initialization pipeline
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            history: Mutex::new(None), // Starts as None on initial boot

            #[cfg(not(target_os = "linux"))]
            _history: Mutex::new(None),
        }
    }
}

// Implement the BaseCheck trait asynchronously
#[async_trait::async_trait]
impl BaseCheck for IoWaitCheck {
    /// Returns the human-readable name of this check
    fn name(&self) -> &'static str {
        "CPU I/O Wait"
    }

    /// Returns the configuration key used in the YAML file to customize this service
    fn config_key(&self) -> &'static str {
        "iowait"
    }

    /// Default execution interval (60 seconds) if not overridden in the configuration
    fn default_period(&self) -> u64 {
        60
    }

    // ==========================================
    // 2. LINUX CORE RUNNER METHOD
    // ==========================================
    /// Asynchronous runner executed strictly on Linux systems.
    /// This parses `/proc/stat` to capture raw CPU execution ticks.
    #[cfg(target_os = "linux")]
    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        // Open Linux kernel statistics pseudo-file
        let file = match File::open("/proc/stat") {
            Ok(f) => f,
            Err(_) => return None,
        };

        let mut reader = BufReader::new(file);
        let mut first_line = String::new();

        // We only care about the very first line starting with "cpu " (aggregate of all cores)
        if reader.read_line(&mut first_line).is_err() {
            return None;
        }

        let fields: Vec<&str> = first_line.split_whitespace().collect();
        if fields.len() < 6 {
            return None;
        }

        // Parse CPU time fields into numerical integer ticks.
        // /proc/stat CPU line sequence: user, nice, system, idle, iowait, irq, softirq...
        let user = fields[1].parse::<u64>().unwrap_or(0);
        let nice = fields[2].parse::<u64>().unwrap_or(0);
        let system = fields[3].parse::<u64>().unwrap_or(0);
        let idle = fields[4].parse::<u64>().unwrap_or(0);
        let iowait = fields[5].parse::<u64>().unwrap_or(0);

        // Also capture trailing optional ticks if available
        let mut trailing_ticks = 0;
        for field in fields.iter().skip(6) {
            trailing_ticks += field.parse::<u64>().unwrap_or(0);
        }

        // Sum up total CPU ticks recorded since system startup
        let total_ticks = user + nice + system + idle + iowait + trailing_ticks;

        // Safely lock our shared history tracking Mutex container
        let mut history_guard = self.history.lock().unwrap();

        let result = match &*history_guard {
            Some(history) => {
                // Calculate difference in ticks between now and the last run
                let total_delta = total_ticks.saturating_sub(history.last_total_ticks) as f64;
                let iowait_delta = iowait.saturating_sub(history.last_iowait_ticks) as f64;

                if total_delta > 0.0 {
                    // Compute percentage out of total CPU activity time
                    let iowait_percent = (iowait_delta / total_delta) * 100.0;

                    // Fetch threshold targets configured in our app profile profiles
                    let (warn, crit) = config
                        .services
                        .get(self.config_key())
                        .map(|sc| (sc.warning, sc.critical))
                        .unwrap_or((5.0, 10.0)); // Default fallback metrics if omitted in config

                    let mut status = "OK".to_string();
                    if iowait_percent >= crit {
                        status = "CRITICAL".to_string();
                    } else if iowait_percent >= warn {
                        status = "WARNING".to_string();
                    }

                    Some(CheckResult::Single {
                        status,
                        message: format!("I/O Wait: {:.2}%", iowait_percent),
                    })
                } else {
                    None
                }
            }
            None => {
                // Initialization run baseline pass
                Some(CheckResult::Single {
                    status: "OK".to_string(),
                    message: "Initializing CPU I/O Wait baseline counters... Calculating rates on next loop iteration.".to_string(),
                })
            }
        };

        // Save current ticks value context into historical cache registry
        *history_guard = Some(IoWaitHistory {
            last_iowait_ticks: iowait,
            last_total_ticks: total_ticks,
        });

        result
    }

    // ==========================================
    // 3. NON-LINUX FALLBACK RUNNER METHOD
    // ==========================================
    /// Asynchronous fallback runner executed on non-Linux platforms (like macOS).
    /// Returning 'None' ensures the engine seamlessly bypasses this check on Mac
    /// without throwing any unused-variable compiler warnings.
    #[cfg(not(target_os = "linux"))]
    async fn run(&self, _config: &AppConfig) -> Option<CheckResult> {
        None
    }
}
