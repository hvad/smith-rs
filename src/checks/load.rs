// Bring in the BaseCheck trait and CheckResult enum from our checks module
use crate::checks::{BaseCheck, CheckResult};
// Bring in the resolved global configurations
use crate::config::AppConfig;
// Import System from the sysinfo crate to query cross-platform system metrics
use sysinfo::System;

/// The main struct responsible for checking system Load Average (1min, 5min, 15min)
pub struct LoadAverageCheck;

// Implement the BaseCheck trait asynchronously
#[async_trait::async_trait]
impl BaseCheck for LoadAverageCheck {
    /// Returns the human-readable name of this check
    fn name(&self) -> &'static str {
        "Load Average"
    }

    /// Returns the configuration key used in the YAML file to customize this service
    fn config_key(&self) -> &'static str {
        "load"
    }

    /// Default execution interval (10 seconds) if not overridden in the configuration
    fn default_period(&self) -> u64 {
        10
    }

    /// Asynchronously runs the system load check.
    /// Uses sysinfo under the hood, which seamlessly handles OS abstraction for both macOS and Linux!
    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        // Resolve warning and critical threshold benchmarks from our YAML configuration.
        // If the "load" service configuration block is missing, fall back to default (16.0, 24.0).
        let (warn, crit) = if let Some(sc) = config.services.get(self.config_key()) {
            (sc.warning, sc.critical)
        } else {
            (16.0, 24.0)
        };

        // Fetch current load averages across 1-minute, 5-minute, and 15-minute windows.
        // Returns a LoadAvg struct with properties `.one`, `.five`, and `.fifteen`.
        let load = System::load_average();
        let mut status = "OK".to_string();

        // Evaluate if any of the load averages (1m, 5m, 15m) exceed our warning or critical thresholds
        if load.one >= crit || load.five >= crit || load.fifteen >= crit {
            status = "CRITICAL".to_string();
        } else if load.one >= warn || load.five >= warn || load.fifteen >= warn {
            status = "WARNING".to_string();
        }

        // Return the formatted result wrapped inside the CheckResult::Single variant
        Some(CheckResult::Single {
            status,
            message: format!(
                "1min: {:.2}, 5min: {:.2}, 15min: {:.2}",
                load.one, load.five, load.fifteen
            ),
        })
    }
}
