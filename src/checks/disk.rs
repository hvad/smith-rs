use crate::checks::{BaseCheck, CheckResult};
use crate::config::AppConfig;
use std::collections::HashMap;
use std::path::Path;
use sysinfo::Disks;

pub struct DiskUsageCheck;

#[async_trait::async_trait]
impl BaseCheck for DiskUsageCheck {
    fn name(&self) -> &'static str {
        "Disk Usage"
    }
    fn config_key(&self) -> &'static str {
        "diskusage"
    }
    fn default_period(&self) -> u64 {
        60
    }

    async fn run(&self, _config: &AppConfig) -> Option<CheckResult> {
        // Only queries mount points directly from the OS kernel VFS
        let disks = Disks::new_with_refreshed_list();
        let disks_input = _config.ini.get_from(Some("System"), "disks").unwrap_or("/");

        let warn = _config
            .ini
            .get_from(Some("System"), "disk_warning_threshold")
            .unwrap_or("90")
            .parse::<f64>()
            .unwrap_or(90.0);
        let crit = _config
            .ini
            .get_from(Some("System"), "disk_critical_threshold")
            .unwrap_or("95")
            .parse::<f64>()
            .unwrap_or(95.0);

        let mut results = HashMap::new();

        // High efficiency substring lookup matching path bytes directly
        for path_str in disks_input.split(',').map(|s| s.trim()) {
            let target_path = Path::new(path_str);
            let mut found = false;

            for disk in disks.iter() {
                if disk.mount_point() == target_path {
                    found = true;
                    let total = disk.total_space() as f64;
                    if total > 0.0 {
                        let available = disk.available_space() as f64;
                        let used_percent = ((total - available) / total) * 100.0;

                        let mut status = "OK".to_string();
                        if used_percent >= crit {
                            status = "CRITICAL".to_string();
                        } else if used_percent >= warn {
                            status = "WARNING".to_string();
                        }

                        results.insert(
                            path_str.to_string(),
                            (status, format!("Used: {:.2}%", used_percent)),
                        );
                    }
                    break;
                }
            }
            if !found {
                results.insert(
                    path_str.to_string(),
                    ("ERROR".to_string(), format!("Could not check {}", path_str)),
                );
            }
        }

        Some(CheckResult::Multi(results))
    }
}
