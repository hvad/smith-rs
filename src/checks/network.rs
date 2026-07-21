// Bring in the BaseCheck trait and CheckResult enum from our checks module
use crate::checks::{BaseCheck, CheckResult};
// Bring in the resolved global configurations
use crate::config::AppConfig;

use std::collections::HashMap; // Standard key-value map implementation
use std::sync::Mutex; // Synchronous Mutex to safely handle shared mutable state across threads
use std::time::Instant; // Used to calculate precise elapsed time between executions
use sysinfo::Networks; // Import Networks struct from sysinfo crate to query interface data

/// Holds historical metrics for calculating rate/throughput per second
struct NetworkHistory {
    last_rx_bytes: u64,
    last_tx_bytes: u64,
    last_check_time: Instant,
}

/// The main struct responsible for monitoring Network Throughput / Bandwidth (Mbps)
pub struct NetworkCheck;

impl NetworkCheck {
    /// Public constructor initialization pipeline
    pub fn new() -> Self {
        Self
    }
}

// Implement the BaseCheck trait asynchronously
#[async_trait::async_trait]
impl BaseCheck for NetworkCheck {
    /// Returns the human-readable name of this check
    fn name(&self) -> &'static str {
        "Network Throughput"
    }

    /// Returns the configuration key used in the YAML file to customize this service
    fn config_key(&self) -> &'static str {
        "network"
    }

    /// Default execution interval (10 seconds) if not overridden in the configuration
    fn default_period(&self) -> u64 {
        10
    }

    /// Asynchronously runs network bandwidth throughput monitoring on macOS and Linux
    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        // Create an internal persistent state tracker for network interfaces
        static NETWORKS: Mutex<Option<Networks>> = Mutex::new(None);
        static HISTORY: Mutex<Option<HashMap<String, NetworkHistory>>> = Mutex::new(None);

        let mut networks_guard = NETWORKS.lock().unwrap();
        let mut history_guard = HISTORY.lock().unwrap();

        let networks = networks_guard.get_or_insert_with(Networks::new_with_refreshed_list);
        let history = history_guard.get_or_insert_with(HashMap::new);

        // Refresh all interface metrics (sysinfo 0.39+ requires true argument)
        networks.refresh(true);

        // Fallback default network card filter if omitted in YAML
        let default_interfaces = vec!["en0".to_string()];

        // Resolve warning/critical thresholds (in Mbps) and target interfaces from YAML
        let (warn, crit, target_interfaces) =
            if let Some(sc) = config.services.get(self.config_key()) {
                (
                    sc.warning,
                    sc.critical,
                    sc.interfaces.as_ref().unwrap_or(&default_interfaces),
                )
            } else {
                (600.0, 900.0, &default_interfaces)
            };

        let current_time = Instant::now();
        let mut results = HashMap::new();

        // Iterate through each interface requested in the YAML configuration
        for target_interface in target_interfaces {
            let mut found = false;

            for (interface_name, data) in networks.iter() {
                if interface_name == target_interface {
                    found = true;

                    let current_rx_bytes = data.total_received();
                    let current_tx_bytes = data.total_transmitted();

                    // Check if we have historical data for this interface
                    if let Some(prev) = history.get(interface_name) {
                        let elapsed_secs = current_time
                            .duration_since(prev.last_check_time)
                            .as_secs_f64();

                        if elapsed_secs > 0.0 {
                            // Calculate byte differences since last check
                            let rx_bytes_diff =
                                current_rx_bytes.saturating_sub(prev.last_rx_bytes) as f64;
                            let tx_bytes_diff =
                                current_tx_bytes.saturating_sub(prev.last_tx_bytes) as f64;

                            // Convert Bytes/sec to Megabits/sec (1 Byte = 8 bits, 1 Megabit = 1,000,000 bits)
                            let rx_mbps = (rx_bytes_diff * 8.0) / (elapsed_secs * 1_000_000.0);
                            let tx_mbps = (tx_bytes_diff * 8.0) / (elapsed_secs * 1_000_000.0);
                            let total_mbps = rx_mbps + tx_mbps;

                            // Evaluate throughput against warning and critical rules
                            let mut status = "OK".to_string();
                            if total_mbps >= crit {
                                status = "CRITICAL".to_string();
                            } else if total_mbps >= warn {
                                status = "WARNING".to_string();
                            }

                            results.insert(
                                target_interface.clone(),
                                (
                                    status,
                                    format!(
                                        "Total: {:.2} Mbps (RX: {:.2} Mbps, TX: {:.2} Mbps)",
                                        total_mbps, rx_mbps, tx_mbps
                                    ),
                                ),
                            );
                        }
                    } else {
                        // First execution cycle: store baseline data, report initialization
                        results.insert(
                            target_interface.clone(),
                            (
                                "OK".to_string(),
                                "Initializing baseline counters... Throughput will calculate on next loop.".to_string(),
                            ),
                        );
                    }

                    // Update historical state for next cycle
                    history.insert(
                        interface_name.clone(),
                        NetworkHistory {
                            last_rx_bytes: current_rx_bytes,
                            last_tx_bytes: current_tx_bytes,
                            last_check_time: current_time,
                        },
                    );

                    break;
                }
            }

            // Interface missing on host system
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
