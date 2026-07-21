// Bring in the BaseCheck trait and CheckResult enum from our checks module
use crate::checks::{BaseCheck, CheckResult};
// Bring in the resolved global configurations
use crate::config::AppConfig;
// Import the async SNTP client from the rsntp crate
use rsntp::AsyncSntpClient;

/// The main struct responsible for checking NTP clock drift relative to a remote time server
pub struct NTPDriftCheck;

// Implement the BaseCheck trait asynchronously
#[async_trait::async_trait]
impl BaseCheck for NTPDriftCheck {
    /// Returns the human-readable name of this check
    fn name(&self) -> &'static str {
        "NTP Drift"
    }

    /// Returns the configuration key used in the YAML file to customize this service
    fn config_key(&self) -> &'static str {
        "ntp"
    }

    /// Default execution interval (120 seconds) if not overridden in the configuration
    fn default_period(&self) -> u64 {
        120
    }

    /// Asynchronously queries the configured NTP server and computes local clock offset
    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        // Look up the service configuration using the 'ntp' config key; return None if missing
        let sc = config.services.get(self.config_key())?;

        // Extract the target NTP server pool address; return None if not defined or empty
        let server = sc.ntp_pool_server.as_ref()?;
        if server.is_empty() {
            return None;
        }

        // Fetch configured warning and critical drift thresholds (in seconds)
        let warn = sc.warning;
        let crit = sc.critical;

        // Instantiate an asynchronous SNTP client
        let client = AsyncSntpClient::new();

        // Perform the asynchronous UDP time synchronization request
        match client.synchronize(server).await {
            Ok(response) => {
                // Calculate the absolute value of the local system clock offset in seconds
                let offset = response.clock_offset().as_secs_f64().abs();

                // Evaluate current time drift against thresholds
                let mut status = "OK".to_string();
                if offset >= crit {
                    status = "CRITICAL".to_string();
                } else if offset >= warn {
                    status = "WARNING".to_string();
                }

                // Return structured check result with formatted status and message
                Some(CheckResult::Single {
                    status,
                    message: format!("Offset: {:.4}s", offset),
                })
            }
            Err(e) => {
                // If network communication or NTP parsing fails, report a CRITICAL status
                Some(CheckResult::Single {
                    status: "CRITICAL".to_string(),
                    message: format!("NTP Error: {}", e),
                })
            }
        }
    }
}
