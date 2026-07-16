// Bring in the BaseCheck trait and CheckResult enum from our checks module
use crate::checks::{BaseCheck, CheckResult};
// Bring in the resolved global configurations
use crate::config::AppConfig;
use std::collections::HashMap; // A standard key-value map implementation

// The 'nix' crate allows Rust to talk directly to Unix system functions (POSIX).
// 'statvfs' is the Unix system call that inspects total and free inodes on a mount point.
use nix::sys::statvfs::statvfs;

/// The struct responsible for inspecting system file index nodes (inodes) usage
pub struct InodesUsageCheck;

// Implement the BaseCheck trait asynchronously
#[async_trait::async_trait]
impl BaseCheck for InodesUsageCheck {
    /// Returns the human-readable name of this check
    fn name(&self) -> &'static str {
        "Inode Utilization"
    }

    /// Returns the configuration key used in the YAML file to customize this service
    fn config_key(&self) -> &'static str {
        "inodes"
    }

    /// Default execution interval (60 seconds) if not overridden in the configuration
    fn default_period(&self) -> u64 {
        60
    }

    /// Asynchronously runs the inode utilization check across target storage volumes
    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        // Define a fallback vector monitoring only the root "/" directory
        let default_disks = vec!["/".to_string()];

        // Resolve configuration values from our YAML service mapping profile
        let (warn, crit, target_disks) = if let Some(sc) = config.services.get(self.config_key()) {
            (
                sc.warning,
                sc.critical,
                sc.disks.as_ref().unwrap_or(&default_disks),
            )
        } else {
            (80.0, 90.0, &default_disks)
        };

        // Create a Hashmap to hold status pairs for individual paths (Multi-metric tracking)
        let mut results = HashMap::new();

        // Loop through each filesystem mount requested in the configuration profiles
        for path_str in target_disks {
            // Invoke the standard POSIX statvfs system call to pull raw hardware volume metrics.
            // We use 'match' to cleanly catch operating system access errors (like Permission Denied).
            match statvfs(path_str.as_str()) {
                Ok(stats) => {
                    // Extract total allocation structures and free counters using nix methods.
                    // files() returns total inodes, files_free() returns free inodes.
                    // We cast them to floats (f64) to perform precision math calculations.
                    let total_inodes = stats.files() as f64;
                    let free_inodes = stats.files_free() as f64;

                    // Ensure total nodes are greater than 0 to avoid mathematical "Divide by Zero" runtime crashes
                    if total_inodes > 0.0 {
                        // Calculate percentage of consumed inodes space
                        let used_inodes = total_inodes - free_inodes;
                        let used_percent = (used_inodes / total_inodes) * 100.0;

                        // Evaluate metric limits to determine current monitoring threshold states
                        let mut status = "OK".to_string();
                        if used_percent >= crit {
                            status = "CRITICAL".to_string();
                        } else if used_percent >= warn {
                            status = "WARNING".to_string();
                        }

                        // Save the results inside our multi-target hash container profile map
                        results.insert(
                            path_str.clone(),
                            (status, format!("Inode utilization at {:.2}%", used_percent)),
                        );
                    }
                }
                Err(_) => {
                    // Catch filesystem hardware errors (e.g., directory removed or missing privileges)
                    results.insert(
                        path_str.clone(),
                        (
                            "ERROR".to_string(),
                            format!("Failed statvfs execution for path: {}", path_str),
                        ),
                    );
                }
            }
        }

        // Return the final data payload wrapped in the CheckResult::Multi variant
        if results.is_empty() {
            None
        } else {
            Some(CheckResult::Multi(results))
        }
    }
}
