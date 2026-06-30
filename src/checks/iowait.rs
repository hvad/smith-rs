use crate::checks::{BaseCheck, CheckResult};
use crate::config::AppConfig;
use std::fs::File;
use std::io::{BufRead, BufReader};
use tokio::sync::Mutex;

pub struct IoWaitCheck {
    // Stores the last observed (iowait_ticks, total_ticks)
    last_ticks: Mutex<Option<(u64, u64)>>,
}

impl IoWaitCheck {
    pub fn new() -> Self {
        IoWaitCheck {
            last_ticks: Mutex::new(None),
        }
    }

    fn sample_cpu_ticks(&self) -> Option<(u64, u64)> {
        let file = File::open("/proc/stat").ok()?;
        let reader = BufReader::new(file);

        for line in reader.lines().map_while(Result::ok) {
            if line.starts_with("cpu ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 {
                    let user: u64 = parts[1].parse().unwrap_or(0);
                    let nice: u64 = parts[2].parse().unwrap_or(0);
                    let system: u64 = parts[3].parse().unwrap_or(0);
                    let idle: u64 = parts[4].parse().unwrap_or(0);
                    let iowait: u64 = parts[5].parse().unwrap_or(0);

                    // Sum remaining fields if present (irq, softirq, steal, guest, guest_nice)
                    let rest: u64 = parts[6..]
                        .iter()
                        .map(|s| s.parse::<u64>().unwrap_or(0))
                        .sum();

                    let total = user + nice + system + idle + iowait + rest;
                    return Some((iowait, total));
                }
            }
        }
        None
    }
}

#[async_trait::async_trait]
impl BaseCheck for IoWaitCheck {
    fn name(&self) -> &'static str {
        "I/O Wait"
    }

    fn config_key(&self) -> &'static str {
        "iowait"
    }

    fn default_period(&self) -> u64 {
        15
    }

    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        let (warn, crit) = if let Some(sc) = config.services.get(self.config_key()) {
            (sc.warning, sc.critical)
        } else {
            (15.0, 30.0)
        };

        let current = self.sample_cpu_ticks()?;
        let mut guard = self.last_ticks.lock().await;

        let result = if let Some((prev_iowait, prev_total)) = *guard {
            let delta_iowait = current.0.saturating_sub(prev_iowait);
            let delta_total = current.1.saturating_sub(prev_total);

            if delta_total > 0 {
                let percent = (delta_iowait as f64 / delta_total as f64) * 100.0;
                let mut status = "OK".to_string();

                if percent >= crit {
                    status = "CRITICAL".to_string();
                } else if percent >= warn {
                    status = "WARNING".to_string();
                }

                Some(CheckResult::Single {
                    status,
                    message: format!("CPU iowait: {:.2}%", percent),
                })
            } else {
                None
            }
        } else {
            // First run baseline collection message
            Some(CheckResult::Single {
                status: "OK".to_string(),
                message: "Initializing metric baseline".to_string(),
            })
        };

        // Cache the current metrics for the next check interval transition
        *guard = Some(current);
        result
    }
}
