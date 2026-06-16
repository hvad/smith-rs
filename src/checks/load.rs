use crate::checks::{BaseCheck, CheckResult};
use crate::config::AppConfig;
use sysinfo::System;

pub struct LoadAverageCheck;

#[async_trait::async_trait]
impl BaseCheck for LoadAverageCheck {
    fn name(&self) -> &'static str {
        "Load"
    }
    fn config_key(&self) -> &'static str {
        "loadaverage"
    }
    fn default_period(&self) -> u64 {
        30
    }

    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        let warn = config
            .ini
            .get_from(Some("System"), "load_average_warning_threshold")
            .unwrap_or("16.0")
            .parse::<f64>()
            .unwrap_or(16.0);
        let crit = config
            .ini
            .get_from(Some("System"), "load_average_critical_threshold")
            .unwrap_or("24.0")
            .parse::<f64>()
            .unwrap_or(24.0);

        // System::load_average() returns a LoadAvg structure containing .one, .five, and .fifteen
        let load = System::load_average();
        let mut status = "OK".to_string();

        // Check alerts by looking at the highest sustained pressure points across 1, 5, or 15 mins
        if load.one >= crit || load.five >= crit || load.fifteen >= crit {
            status = "CRITICAL".to_string();
        } else if load.one >= warn || load.five >= warn || load.fifteen >= warn {
            status = "WARNING".to_string();
        }

        Some(CheckResult::Single {
            status,
            // Added load.fifteen formatting to the logged message payload
            message: format!(
                "1min: {:.2}, 5min: {:.2}, 15min: {:.2}",
                load.one, load.five, load.fifteen
            ),
        })
    }
}
