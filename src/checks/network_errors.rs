use crate::checks::{BaseCheck, CheckResult};
use crate::config::AppConfig;
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::Mutex;

struct NetErrorSample {
    timestamp: Instant,
    // Maps card name to (rx_errors, rx_dropped)
    stats: HashMap<String, (u64, u64)>,
}

pub struct NetworkErrorsCheck {
    last_sample: Mutex<Option<NetErrorSample>>,
}

impl NetworkErrorsCheck {
    pub fn new() -> Self {
        NetworkErrorsCheck {
            last_sample: Mutex::new(None),
        }
    }

    #[cfg(target_os = "linux")]
    fn sample_network_errors(&self) -> Option<HashMap<String, (u64, u64)>> {
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

                if parts.len() >= 4 {
                    let rx_errs: u64 = parts[2].parse().unwrap_or(0);
                    let rx_drop: u64 = parts[3].parse().unwrap_or(0);
                    samples.insert(iface_name, (rx_errs, rx_drop));
                }
            }
        }
        Some(samples)
    }

    #[cfg(target_os = "macos")]
    fn sample_network_errors(&self) -> Option<HashMap<String, (u64, u64)>> {
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
                        let rx_errs = if_data.ifi_ierrors as u64;
                        let rx_drop = if_data.ifi_iqdrops as u64;
                        samples.insert(name, (rx_errs, rx_drop));
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
impl BaseCheck for NetworkErrorsCheck {
    fn name(&self) -> &'static str {
        "Network Errors"
    }

    fn config_key(&self) -> &'static str {
        "network_errors"
    }

    fn default_period(&self) -> u64 {
        15
    }

    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        // Threshold counts represent error/drop event count rates per second
        let (warn, crit, raw_interfaces) = if let Some(sc) = config.services.get(self.config_key())
        {
            (sc.warning, sc.critical, sc.disks.clone()) // Pulls card array configurations
        } else {
            let default_card = if cfg!(target_os = "macos") {
                "en0"
            } else {
                "eth0"
            };
            (0.1, 1.0, Some(vec![default_card.to_string()]))
        };

        let target_interfaces = raw_interfaces.unwrap_or_else(Vec::new);
        if target_interfaces.is_empty() {
            return None;
        }

        let current_stats = self.sample_network_errors()?;
        let current_time = Instant::now();

        let mut guard = self.last_sample.lock().await;
        let mut metrics = HashMap::new();

        if let Some(prev) = guard.as_ref() {
            let duration_secs = current_time.duration_since(prev.timestamp).as_secs_f64();

            if duration_secs > 0.0 {
                for iface in &target_interfaces {
                    if let (Some(&(curr_errs, curr_drop)), Some(&(prev_errs, prev_drop))) =
                        (current_stats.get(iface), prev.stats.get(iface))
                    {
                        let delta_errs = curr_errs.saturating_sub(prev_errs);
                        let delta_drop = curr_drop.saturating_sub(prev_drop);

                        let errs_per_sec = delta_errs as f64 / duration_secs;
                        let drop_per_sec = delta_drop as f64 / duration_secs;
                        let total_fault_rate = errs_per_sec + drop_per_sec;

                        let status = if total_fault_rate >= crit {
                            "CRITICAL".to_string()
                        } else if total_fault_rate >= warn {
                            "WARNING".to_string()
                        } else {
                            "OK".to_string()
                        };

                        metrics.insert(
                            iface.to_string(),
                            (
                                status,
                                format!(
                                    "Errors: {} ({:.2}/s), Dropped: {} ({:.2}/s)",
                                    delta_errs, errs_per_sec, delta_drop, drop_per_sec
                                ),
                            ),
                        );
                    } else {
                        metrics.insert(
                            iface.to_string(),
                            (
                                "UNKNOWN".to_string(),
                                format!("Card '{}' not found on system", iface),
                            ),
                        );
                    }
                }
            }
        } else {
            for iface in &target_interfaces {
                metrics.insert(
                    iface.to_string(),
                    (
                        "OK".to_string(),
                        format!("Initializing error tracking baseline for card '{}'", iface),
                    ),
                );
            }
        }

        *guard = Some(NetErrorSample {
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
