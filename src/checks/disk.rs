use crate::checks::{BaseCheck, CheckResult};
use crate::config::AppConfig;
use std::collections::HashMap;
use std::path::Path;
use sysinfo::Disks;

pub struct DiskUsageCheck;

#[async_trait::async_trait]
impl BaseCheck for DiskUsageCheck {
    fn name(&self) -> &'static str {
        "Disk Space"
    }
    fn config_key(&self) -> &'static str {
        "disk"
    }
    fn default_period(&self) -> u64 {
        60
    }

    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        let disks = Disks::new_with_refreshed_list();

        let default_disks = vec!["/".to_string()];
        let (warn, crit, target_disks) = if let Some(sc) = config.services.get(self.config_key()) {
            (
                sc.warning,
                sc.critical,
                sc.disks.as_ref().unwrap_or(&default_disks),
            )
        } else {
            (90.0, 95.0, &default_disks)
        };

        let mut results = HashMap::new();

        for path_str in target_disks {
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
                            path_str.clone(),
                            (status, format!("Used: {:.2}%", used_percent)),
                        );
                    }
                    break;
                }
            }
            if !found {
                results.insert(
                    path_str.clone(),
                    ("ERROR".to_string(), format!("Could not check {}", path_str)),
                );
            }
        }
        Some(CheckResult::Multi(results))
    }
}
