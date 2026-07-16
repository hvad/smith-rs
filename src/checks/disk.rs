// Bring in the BaseCheck trait and CheckResult enum from our checks module
use crate::checks::{BaseCheck, CheckResult};
// Bring in the resolved global configurations
use crate::config::AppConfig;
use std::collections::HashMap; // A standard key-value map implementation
use std::path::Path; // Standard library utility to safely parse and handle filesystem paths
use sysinfo::Disks; // Import the 'Disks' utility from the sysinfo crate to query mounted filesystems

/// The struct responsible for inspecting system disk space usage
pub struct DiskUsageCheck;

// Implement the BaseCheck trait asynchronously
#[async_trait::async_trait]
impl BaseCheck for DiskUsageCheck {
    /// Returns the human-readable name of this check
    fn name(&self) -> &'static str {
        "Disk Space"
    }

    /// Returns the configuration key used in the YAML file to customize this service
    fn config_key(&self) -> &'static str {
        "disk"
    }

    /// Default execution interval (60 seconds) if not overridden in the configuration
    fn default_period(&self) -> u64 {
        60
    }

    /// Asynchronously runs the disk space inspection
    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        // Retrieve a freshly refreshed list of all mounted filesystems and disks from the OS
        let disks = Disks::new_with_refreshed_list();

        // Define a fallback vector monitoring only the root "/" directory
        let default_disks = vec!["/".to_string()];

        // Resolve configuration values:
        // - warn/crit: threshold percentages
        // - target_disks: a reference to the array of disk paths defined in the YAML file
        let (warn, crit, target_disks) = if let Some(sc) = config.services.get(self.config_key()) {
            (
                sc.warning,
                sc.critical,
                sc.disks.as_ref().unwrap_or(&default_disks),
            )
        } else {
            (90.0, 95.0, &default_disks)
        };

        // Create a Hashmap to store status and messages for each target disk (Multi-check)
        let mut results = HashMap::new();

        // Iterate over each disk configured for monitoring
        for path_str in target_disks {
            // Convert the path string into a safe, platform-independent Path reference
            let target_path = Path::new(path_str);
            let mut found = false;

            // Search through the OS-detected disks list to find a match
            for disk in disks.iter() {
                // If the OS mount point matches our configured target path, process it
                if disk.mount_point() == target_path {
                    found = true;
                    let total = disk.total_space() as f64;

                    if total > 0.0 {
                        let available = disk.available_space() as f64;
                        // Calculate percentage of used space
                        let used_percent = ((total - available) / total) * 100.0;

                        // Compare the calculated usage against warn and crit thresholds
                        let mut status = "OK".to_string();
                        if used_percent >= crit {
                            status = "CRITICAL".to_string();
                        } else if used_percent >= warn {
                            status = "WARNING".to_string();
                        }

                        // Store the outcome for this specific disk mount
                        results.insert(
                            path_str.clone(),
                            (status, format!("Used: {:.2}%", used_percent)),
                        );
                    }
                    break; // Found the target mount point, we can stop searching for this path
                }
            }

            // If the loop finished and the disk was not found (e.g. invalid mount path or not mounted)
            if !found {
                results.insert(
                    path_str.clone(),
                    ("ERROR".to_string(), format!("Could not check {}", path_str)),
                );
            }
        }

        // Return the results wrapped in the CheckResult::Multi variant
        if results.is_empty() {
            None
        } else {
            Some(CheckResult::Multi(results))
        }
    }
}
