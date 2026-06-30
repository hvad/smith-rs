use crate::checks::{BaseCheck, CheckResult};
use crate::config::AppConfig;
use std::collections::HashMap;
use std::ffi::CString;
use std::mem;

pub struct InodesUsageCheck;

impl InodesUsageCheck {
    fn get_inode_utilization(path: &str) -> Result<f64, String> {
        let c_path = CString::new(path).map_err(|_| "Invalid path string".to_string())?;

        unsafe {
            let mut stats: libc::statvfs = mem::zeroed();
            if libc::statvfs(c_path.as_ptr(), &mut stats) == 0 {
                let total_inodes = stats.f_files as f64;
                let free_inodes = stats.f_ffree as f64;

                if total_inodes == 0.0 {
                    return Err("Total inodes reported as zero".to_string());
                }

                let used_inodes = total_inodes - free_inodes;
                let utilization_percentage = (used_inodes / total_inodes) * 100.0;
                Ok(utilization_percentage)
            } else {
                Err("Failed to execute statvfs call".to_string())
            }
        }
    }
}

#[async_trait::async_trait]
impl BaseCheck for InodesUsageCheck {
    fn name(&self) -> &'static str {
        "Inode Utilization"
    }

    fn config_key(&self) -> &'static str {
        "inodes"
    }

    fn default_period(&self) -> u64 {
        60
    }

    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        // Wrap the default vector in Some() to match the Option type inside sc.disks
        let (warn, crit, raw_disks) = if let Some(sc) = config.services.get(self.config_key()) {
            (sc.warning, sc.critical, sc.disks.clone())
        } else {
            (80.0, 90.0, Some(vec!["/".to_string()]))
        };

        // Fallback if the configuration entry exists but 'disks' was omitted/null
        let target_disks = raw_disks.unwrap_or_else(|| vec!["/".to_string()]);

        let mut metrics: HashMap<String, (String, String)> = HashMap::new();

        for disk in &target_disks {
            match Self::get_inode_utilization(disk) {
                Ok(percent) => {
                    let status = if percent >= crit {
                        "CRITICAL".to_string()
                    } else if percent >= warn {
                        "WARNING".to_string()
                    } else {
                        "OK".to_string()
                    };

                    metrics.insert(
                        disk.to_string(),
                        (status, format!("Inode utilization at {:.2}%", percent)),
                    );
                }
                Err(err) => {
                    metrics.insert(disk.to_string(), ("UNKNOWN".to_string(), err));
                }
            }
        }

        if metrics.is_empty() {
            None
        } else {
            Some(CheckResult::Multi(metrics))
        }
    }
}
