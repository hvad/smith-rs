// Bring in the BaseCheck trait and CheckResult enum from our checks module
use crate::checks::{BaseCheck, CheckResult};
// Bring in the resolved global configurations
use crate::config::AppConfig;
// Import System from the sysinfo crate to query cross-platform system memory
use sysinfo::System;
// Use a synchronous Mutex to safely share and mutate our System instance across check runs
use std::sync::Mutex;

/// The struct responsible for inspecting system RAM usage
pub struct MemoryUsageCheck {
    // We wrap `System` inside a `Mutex` because `sysinfo::System` requires mutable access
    // (`&mut self`) to refresh its memory counters, while `run()` only takes `&self`.
    sys: Mutex<System>,
}

impl MemoryUsageCheck {
    /// Public constructor to initialize the memory check instance
    pub fn new() -> Self {
        Self {
            // Initialize an empty System struct instance wrapped in a Mutex
            sys: Mutex::new(System::new()),
        }
    }
}

// Implement the BaseCheck trait asynchronously
#[async_trait::async_trait]
impl BaseCheck for MemoryUsageCheck {
    /// Returns the human-readable name of this check
    fn name(&self) -> &'static str {
        "Memory Usage"
    }

    /// Returns the configuration key used in the YAML file to customize this service
    fn config_key(&self) -> &'static str {
        "memory"
    }

    /// Default execution interval (60 seconds) if not overridden in the configuration
    fn default_period(&self) -> u64 {
        60
    }

    /// Asynchronously runs the RAM utilization check on macOS and Linux
    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        // Lock the Mutex to gain safe, mutable access to our System tracking instance.
        // unwrap() acquires the lock guard or panics if the thread holding it previously panicked.
        let mut sys = self.sys.lock().unwrap();

        // Instruct sysinfo to fetch the latest RAM metrics from the OS kernel
        sys.refresh_memory();

        let total_mem = sys.total_memory() as f64;

        // If total memory is 0 (e.g. system read error), safely exit early
        if total_mem == 0.0 {
            return None;
        }

        let used_mem = sys.used_memory() as f64;

        // Calculate the percentage of RAM currently in use
        let used_percent = (used_mem / total_mem) * 100.0;

        // Convert raw bytes to megabytes (MB) for human-readable output
        let used_mb = used_mem / 1024.0 / 1024.0;
        let total_mb = total_mem / 1024.0 / 1024.0;

        // Fetch warning and critical thresholds from the YAML configuration
        let (warn, crit) = if let Some(sc) = config.services.get(self.config_key()) {
            (sc.warning, sc.critical)
        } else {
            (80.0, 90.0) // Fallback defaults if omitted in config
        };

        // Evaluate metric bounds against warning and critical rules
        let mut status = "OK".to_string();
        if used_percent >= crit {
            status = "CRITICAL".to_string();
        } else if used_percent >= warn {
            status = "WARNING".to_string();
        }

        // Return the formatted single status result
        Some(CheckResult::Single {
            status,
            message: format!(
                "RAM: {:.1}% used ({:.0} MB / {:.0} MB)",
                used_percent, used_mb, total_mb
            ),
        })
    }
}
