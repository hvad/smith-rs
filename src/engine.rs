use crate::alert::SMTPAlert;
use crate::checks::{BaseCheck, CheckResult};
use crate::config::AppConfig;

use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

pub struct SmithEngine {
    config: AppConfig,
    email_alert: Arc<SMTPAlert>,
    checks: Arc<Vec<Box<dyn BaseCheck>>>,
}

impl SmithEngine {
    pub fn new(config: AppConfig) -> Self {
        let email_alert = Arc::new(SMTPAlert::new(&config));

        SmithEngine {
            config,
            email_alert,
            checks: Arc::new(Vec::new()),
        }
    }

    pub fn add_check<T: BaseCheck + 'static>(&mut self, check: T) {
        let enabled = self
            .config
            .ini
            .get_from(Some("Setting"), check.config_key())
            .unwrap_or("true")
            .parse::<bool>()
            .unwrap_or(true);

        if enabled {
            Arc::get_mut(&mut self.checks)
                .expect("SmithEngine initialization requires exclusive access")
                .push(Box::new(check));
        }
    }

    fn log_and_alert(&self, category: &str, status: &str, message: &str) {
        let log_entry = format!("{}: {} - {}", category, status, message);
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let formatted_log = format!(
            "{} - INFO - [{}] {}\n",
            timestamp, self.config.hostname, log_entry
        );

        print!("{}", formatted_log);

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.config.log_file_path)
        {
            let _ = file.write_all(formatted_log.as_bytes());
        }

        let alert_key = category
            .to_lowercase()
            .replace(' ', "")
            .split('(')
            .next()
            .unwrap_or("")
            .to_string();
        let alert_enabled = self
            .config
            .ini
            .get_from(Some("Alerts"), &alert_key)
            .unwrap_or("false")
            .parse::<bool>()
            .unwrap_or(false);

        if alert_enabled && (status == "CRITICAL" || status == "WARNING") {
            let subject = format!(
                "duck Alert [{}] - {} on {}",
                status, category, self.config.hostname
            );
            let email_alert_clone = Arc::clone(&self.email_alert);
            let msg_clone = message.to_string();

            tokio::spawn(async move {
                email_alert_clone.send_alert(subject, msg_clone).await;
            });
        }
    }

    async fn process_result(&self, category: &str, result: CheckResult) {
        match result {
            CheckResult::Single { status, message } => {
                self.log_and_alert(category, &status, &message);
            }
            CheckResult::Multi(map) => {
                for (item, (status, msg)) in map {
                    self.log_and_alert(&format!("{} ({})", category, item), &status, &msg);
                }
            }
        }
    }

    pub async fn run_scheduler(self) {
        let engine = Arc::new(self);
        let mut worker_handles = Vec::new();
        let total_checks = engine.checks.len();

        for index in 0..total_checks {
            let engine_clone = Arc::clone(&engine);

            let check_key = engine_clone.checks[index].config_key();
            let default_period = engine_clone.checks[index].default_period();
            let check_name = engine_clone.checks[index].name();

            let period_key = format!("{}_period", check_key);
            let period_secs = engine_clone
                .config
                .ini
                .get_from(Some("Setting"), &period_key)
                .unwrap_or(&default_period.to_string())
                .parse::<u64>()
                .unwrap_or(default_period);

            let handle = tokio::spawn(async move {
                loop {
                    let loop_start = tokio::time::Instant::now();

                    // PERFORMANCE: No more global Mutex lock contention here!
                    let res = engine_clone.checks[index].run(&engine_clone.config).await;

                    if let Some(check_result) = res {
                        engine_clone.process_result(check_name, check_result).await;
                    }

                    let elapsed = loop_start.elapsed();
                    let interval = Duration::from_secs(period_secs);
                    if elapsed < interval {
                        tokio::time::sleep(interval - elapsed).await;
                    }
                }
            });
            worker_handles.push(handle);
        }

        for handle in worker_handles {
            let _ = handle.await;
        }
    }
}
