use crate::checks::{BaseCheck, CheckResult};
use crate::config::AppConfig;
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::Mutex;

struct NetSample {
    timestamp: Instant,
    // Maps explicit card name (e.g., "eth0", "en0") to (rx_bytes, tx_bytes)
    stats: HashMap<String, (u64, u64)>,
}

pub struct NetworkThroughputCheck {
    last_sample: Mutex<Option<NetSample>>,
}

impl NetworkThroughputCheck {
    pub fn new() -> Self {
        NetworkThroughputCheck {
            last_sample: Mutex::new(None),
        }
    }

    #[cfg(target_os = "linux")]
    fn sample_network_bytes(&self) -> Option<HashMap<String, (u64, u64)>> {
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        let file = File::open("/proc/net/dev").ok()?;
        let reader = BufReader::new(file);
        let mut samples = HashMap::new();

        for line in reader.lines().map_while(Result::ok) {
            if let Some(idx) = line.find(':') {
                let iface_name = line[..idx].trim().to_string();
                let metrics_str = &line[idx + 1..];
                let parts: Vec<&str> = metrics_str.split_whitespace().collect();

                if parts.len() >= 9 {
                    let rx_bytes: u64 = parts[0].parse().unwrap_or(0);
                    let tx_bytes: u64 = parts[8].parse().unwrap_or(0);
                    samples.insert(iface_name, (rx_bytes, tx_bytes));
                }
            }
        }
        Some(samples)
    }

    #[cfg(target_os = "macos")]
    fn sample_network_bytes(&self) -> Option<HashMap<String, (u64, u64)>> {
        use std::ffi::CStr;
        use std::ptr;

        let mut samples = HashMap::new();
        let mut ifap: *mut libc::ifaddrs = ptr::null_mut();

        unsafe {
            if libc::getifaddrs(&mut ifap) != 0 {
                return None;
            }

            let mut curr = ifap;
            while !curr.is_null() {
                let ifa = *curr;
                if !ifa.ifa_addr.is_null() && (*ifa.ifa_addr).sa_family == libc::AF_LINK as u8 {
                    let name = CStr::from_ptr(ifa.ifa_name).to_string_lossy().into_owned();
                    if !ifa.ifa_data.is_null() {
                        let if_data = *(ifa.ifa_data as *mut libc::if_data);
                        let rx_bytes = if_data.ifi_ibytes as u64;
                        let tx_bytes = if_data.ifi_obytes as u64;
                        samples.insert(name, (rx_bytes, tx_bytes));
                    }
                }
                curr = ifa.ifa_next;
            }
            libc::freeifaddrs(ifap);
        }
        Some(samples)
    }
}

#[async_trait::async_trait]
impl BaseCheck for NetworkThroughputCheck {
    fn name(&self) -> &'static str {
        "Network Throughput"
    }

    fn config_key(&self) -> &'static str {
        "network"
    }

    fn default_period(&self) -> u64 {
        15
    }

    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        // Warning & Critical match the configured threshold values (Mbps)
        let (warn, crit, raw_interfaces) = if let Some(sc) = config.services.get(self.config_key())
        {
            (sc.warning, sc.critical, sc.disks.clone()) // Pulls the card names list array
        } else {
            // Default fallback if service configuration entry is completely missing
            let default_card = if cfg!(target_os = "macos") {
                "en0"
            } else {
                "eth0"
            };
            (100.0, 500.0, Some(vec![default_card.to_string()]))
        };

        // Unwraps the vector list containing declared network card labels
        let target_interfaces = raw_interfaces.unwrap_or_else(Vec::new);
        if target_interfaces.is_empty() {
            return None;
        }

        let current_stats = self.sample_network_bytes()?;
        let current_time = Instant::now();

        let mut guard = self.last_sample.lock().await;
        let mut metrics = HashMap::new();

        if let Some(prev) = guard.as_ref() {
            let duration_secs = current_time.duration_since(prev.timestamp).as_secs_f64();

            if duration_secs > 0.0 {
                // Loop exclusively through your explicitly configured network card labels
                for iface in &target_interfaces {
                    if let (Some(&(curr_rx, curr_tx)), Some(&(prev_rx, prev_tx))) =
                        (current_stats.get(iface), prev.stats.get(iface))
                    {
                        let delta_rx = curr_rx.saturating_sub(prev_rx);
                        let delta_tx = curr_tx.saturating_sub(prev_tx);

                        // Convert total payload delta into Megabits per second
                        let rx_mbps = (delta_rx as f64 * 8.0) / (duration_secs * 1_000_000.0);
                        let tx_mbps = (delta_tx as f64 * 8.0) / (duration_secs * 1_000_000.0);
                        let total_mbps = rx_mbps + tx_mbps;

                        let status = if total_mbps >= crit {
                            "CRITICAL".to_string()
                        } else if total_mbps >= warn {
                            "WARNING".to_string()
                        } else {
                            "OK".to_string()
                        };

                        metrics.insert(
                            iface.to_string(),
                            (
                                status,
                                format!(
                                    "Throughput: {:.2} Mbps (RX: {:.2} Mbps, TX: {:.2} Mbps)",
                                    total_mbps, rx_mbps, tx_mbps
                                ),
                            ),
                        );
                    } else {
                        metrics.insert(
                            iface.to_string(),
                            (
                                "UNKNOWN".to_string(),
                                format!(
                                    "Network card '{}'
not found on system",
                                    iface
                                ),
                            ),
                        );
                    }
                }
            }
        } else {
            // Populate metric initialization messages for declared cards on the first frame
            for iface in &target_interfaces {
                metrics.insert(
                    iface.to_string(),
                    (
                        "OK".to_string(),
                        format!("Initializing baseline statistics for card '{}'", iface),
                    ),
                );
            }
        }

        *guard = Some(NetSample {
            timestamp: current_time,
            stats: current_stats,
        });

        if metrics.is_empty() {
            None
        } else {
            Some(CheckResult::Multi(metrics))
        }
    }
}
