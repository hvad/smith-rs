use crate::checks::{BaseCheck, CheckResult};
use crate::config::AppConfig;
use sysinfo::System;

pub struct LoadAverageCheck;

#[async_trait::async_trait]
impl BaseCheck for LoadAverageCheck {
    fn name(&self) -> &'static str {
        "Load Average"
    }
    fn config_key(&self) -> &'static str {
        "load"
    }
    fn default_period(&self) -> u64 {
        10
    }

    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        let (warn, crit) = if let Some(sc) = config.services.get(self.config_key()) {
            (sc.warning, sc.critical)
        } else {
            (16.0, 24.0)
        };

        let load = System::load_average();
        let mut status = "OK".to_string();

        if load.one >= crit || load.five >= crit || load.fifteen >= crit {
            status = "CRITICAL".to_string();
        } else if load.one >= warn || load.five >= warn || load.fifteen >= warn {
            status = "WARNING".to_string();
        }

        Some(CheckResult::Single {
            status,
            message: format!(
                "1min: {:.2}, 5min: {:.2}, 15min: {:.2}",
                load.one, load.five, load.fifteen
            ),
        })
    }
}
