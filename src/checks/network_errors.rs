// Bring in the BaseCheck trait and CheckResult enum from our checks module
use crate::checks::{BaseCheck, CheckResult};
// Bring in the resolved global configurations
use crate::config::AppConfig;

use std::collections::HashMap; // Standard key-value map implementation
use std::sync::Mutex; // Synchronous Mutex to safely share state across check cycles
use std::time::Instant; // Used to compute precise time durations between executions
use sysinfo::Networks; // Import Networks struct from sysinfo crate to query interface error counters

/// Stores historical packet error counts per interface to calculate error rates per second
struct NetworkErrorsHistory {
    last_total_errors: u64,
    last_check_time: Instant,
}

/// The main struct responsible for checking network interface error and packet drop rates
pub struct NetworkErrorsCheck;

impl NetworkErrorsCheck {
    /// Public constructor initialization pipeline
    pub fn new() -> Self {
        Self
    }
}

// Implement the BaseCheck trait asynchronously
#[async_trait::async_trait]
impl BaseCheck for NetworkErrorsCheck {
    /// Returns the human-readable name of this check
    fn name(&self) -> &'static str {
        "Network Errors"
    }

    /// Returns the configuration key used in the YAML file to customize this service
    fn config_key(&self) -> &'static str {
        "network_errors"
    }

    /// Default execution interval (60 seconds) if not overridden in the configuration
    fn default_period(&self) -> u64 {
        60
    }

    /// Asynchronously runs the network interface error rate check on macOS and Linux
    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        // Create internal persistent state trackers for sysinfo and error history
        static NETWORKS: Mutex<Option<Networks>> = Mutex::new(None);
        static HISTORY: Mutex<Option<HashMap<String, NetworkErrorsHistory>>> = Mutex::new(None);

        let mut networks_guard = NETWORKS.lock().unwrap();
        let mut history_guard = HISTORY.lock().unwrap();

        let networks = networks_guard.get_or_insert_with(Networks::new_with_refreshed_list);
        let history = history_guard.get_or_insert_with(HashMap::new);

        // Refresh all network interfaces (sysinfo 0.39+ requires true parameter)
        networks.refresh(true);

        // Fallback default network card filter if omitted in YAML
        let default_interfaces = vec!["en0".to_string()];

        // Resolve warning/critical thresholds (in drops/sec) and target interfaces
        let (warn, crit, target_interfaces) =
            if let Some(sc) = config.services.get(self.config_key()) {
                (
                    sc.warning,
                    sc.critical,
                    sc.interfaces.as_ref().unwrap_or(&default_interfaces),
                )
            } else {
                (0.05, 1.0, &default_interfaces)
            };

        let current_time = Instant::now();
        let mut results = HashMap::new();

        // Loop through each interface specified in the configuration
        for target_interface in target_interfaces {
            let mut found = false;

            for (interface_name, data) in networks.iter() {
                if interface_name == target_interface {
                    found = true;

                    // Fetch absolute cumulative RX and TX packet errors
                    let rx_errors = data.errors_on_received();
                    let tx_errors = data.errors_on_transmitted();
                    let current_total_errors = rx_errors + tx_errors;

                    // Calculate rate if historical data exists for this specific interface
                    if let Some(prev) = history.get(interface_name) {
                        let elapsed_secs = current_time
                            .duration_since(prev.last_check_time)
                            .as_secs_f64();

                        if elapsed_secs > 0.0 {
                            let errors_diff =
                                current_total_errors.saturating_sub(prev.last_total_errors) as f64;
                            let errors_per_sec = errors_diff / elapsed_secs;

                            // Evaluate error rate against warning and critical rules
                            let mut status = "OK".to_string();
                            if errors_per_sec >= crit {
                                status = "CRITICAL".to_string();
                            } else if errors_per_sec >= warn {
                                status = "WARNING".to_string();
                            }

                            results.insert(
                                target_interface.clone(),
                                (
                                    status,
                                    format!(
                                        "{:.2} errors/sec (Total RX: {}, TX: {})",
                                        errors_per_sec, rx_errors, tx_errors
                                    ),
                                ),
                            );
                        }
                    } else {
                        // Initial execution cycle baseline pass
                        results.insert(
                            target_interface.clone(),
                            (
                                "OK".to_string(),
                                "Initializing baseline counters... Error rate will calculate on next loop.".to_string(),
                            ),
                        );
                    }

                    // Store current state into historical record
                    history.insert(
                        interface_name.clone(),
                        NetworkErrorsHistory {
                            last_total_errors: current_total_errors,
                            last_check_time: current_time,
                        },
                    );

                    break;
                }
            }

            // Interface specified in YAML is missing from host
            if !found {
                results.insert(
                    target_interface.clone(),
                    (
                        "ERROR".to_string(),
                        format!("Interface {} not found", target_interface),
                    ),
                );
            }
        }

        if results.is_empty() {
            None
        } else {
            Some(CheckResult::Multi(results))
        }
    }
}
