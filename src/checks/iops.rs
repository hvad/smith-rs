use crate::checks::{BaseCheck, CheckResult};
use crate::config::AppConfig;

#[cfg(target_os = "linux")]
use std::{collections::HashMap, time::Instant, tokio::sync::Mutex};

#[cfg(target_os = "linux")]
struct DiskSamples {
    timestamp: Instant,
    stats: HashMap<String, (u64, u64)>,
}

pub struct IopsCheck {
    #[cfg(target_os = "linux")]
    last_samples: Mutex<Option<DiskSamples>>,
}

impl IopsCheck {
    pub fn new() -> Self {
        IopsCheck {
            #[cfg(target_os = "linux")]
            last_samples: Mutex::new(None),
        }
    }

    #[cfg(target_os = "linux")]
    fn sample_disk_stats(&self) -> Option<HashMap<String, (u64, u64)>> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        let file = File::open("/proc/diskstats").ok()?;
        let reader = BufReader::new(file);
        let mut samples = HashMap::new();

        for line in reader.lines().map_while(Result::ok) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 8 {
                let dev_name = parts[2].to_string();
                let reads: u64 = parts[3].parse().unwrap_or(0);
                let writes: u64 = parts[7].parse().unwrap_or(0);
                samples.insert(dev_name, (reads, writes));
            }
        }
        Some(samples)
    }
}

#[async_trait::async_trait]
impl BaseCheck for IopsCheck {
    fn name(&self) -> &'static str {
        "Disk IOPS"
    }

    fn config_key(&self) -> &'static str {
        "iops"
    }

    fn default_period(&self) -> u64 {
        15
    }

    #[cfg(target_os = "linux")]
    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        let (warn, crit, raw_disks) = if let Some(sc) = config.services.get(self.config_key()) {
            (sc.warning, sc.critical, sc.disks.clone())
        } else {
            (1000.0, 2000.0, Some(vec!["sda".to_string()]))
        };

        let target_devices = raw_disks.unwrap_or_else(|| vec!["sda".to_string()]);
        let current_stats = self.sample_disk_stats()?;
        let current_time = Instant::now();

        let mut guard = self.last_samples.lock().await;
        let mut metrics = HashMap::new();

        if let Some(prev) = guard.as_ref() {
            let duration_secs = current_time.duration_since(prev.timestamp).as_secs_f64();

            if duration_secs > 0.0 {
                for dev in &target_devices {
                    if let (Some(&(curr_reads, curr_writes)), Some(&(prev_reads, prev_writes))) =
                        (current_stats.get(dev), prev.stats.get(dev))
                    {
                        let delta_reads = curr_reads.saturating_sub(prev_reads);
                        let delta_writes = curr_writes.saturating_sub(prev_writes);

                        let read_iops = delta_reads as f64 / duration_secs;
                        let write_iops = delta_writes as f64 / duration_secs;
                        let total_iops = read_iops + write_iops;

                        let status = if total_iops >= crit {
                            "CRITICAL".to_string()
                        } else if total_iops >= warn {
                            "WARNING".to_string()
                        } else {
                            "OK".to_string()
                        };

                        metrics.insert(
                            dev.to_string(),
                            (
                                status,
                                format!(
                                    "Total IOPS: {:.1} (Read: {:.1}/s, Write: {:.1}/s)",
                                    total_iops, read_iops, write_iops
                                ),
                            ),
                        );
                    } else {
                        metrics.insert(
                            dev.to_string(),
                            (
                                "UNKNOWN".to_string(),
                                format!("Device '{}' not found in stats", dev),
                            ),
                        );
                    }
                }
            }
        } else {
            for dev in &target_devices {
                metrics.insert(
                    dev.to_string(),
                    (
                        "OK".to_string(),
                        "Initializing IOPS baseline statistics".to_string(),
                    ),
                );
            }
        }

        *guard = Some(DiskSamples {
            timestamp: current_time,
            stats: current_stats,
        });

        if metrics.is_empty() {
            None
        } else {
            Some(CheckResult::Multi(metrics))
        }
    }

    #[cfg(not(target_os = "linux"))]
    async fn run(&self, _config: &AppConfig) -> Option<CheckResult> {
        None
    }
}
