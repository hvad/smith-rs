use crate::checks::{BaseCheck, CheckResult};
use crate::config::AppConfig;
use sysinfo::System;

pub struct MemoryUsageCheck {
    sys: tokio::sync::Mutex<System>,
}

impl MemoryUsageCheck {
    pub fn new() -> Self {
        MemoryUsageCheck {
            sys: tokio::sync::Mutex::new(System::new()),
        }
    }
}

#[async_trait::async_trait]
impl BaseCheck for MemoryUsageCheck {
    fn name(&self) -> &'static str {
        "Memory Usage"
    }
    fn config_key(&self) -> &'static str {
        "memory"
    }
    fn default_period(&self) -> u64 {
        15
    }

    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        let (warn, crit) = if let Some(sc) = config.services.get(self.config_key()) {
            (sc.warning, sc.critical)
        } else {
            (85.0, 95.0)
        };

        let mut sys = self.sys.lock().await;
        sys.refresh_memory();

        let total = sys.total_memory() as f64;
        if total == 0.0 {
            return None;
        }
        let used = sys.used_memory() as f64;
        let percent = (used / total) * 100.0;

        let mut status = "OK".to_string();
        if percent >= crit {
            status = "CRITICAL".to_string();
        } else if percent >= warn {
            status = "WARNING".to_string();
        }

        Some(CheckResult::Single {
            status,
            message: format!("RAM Usage: {:.2}%", percent),
        })
    }
}
