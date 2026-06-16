use crate::checks::{BaseCheck, CheckResult};
use crate::config::AppConfig;
use rsntp::AsyncSntpClient;

pub struct NTPDriftCheck;

#[async_trait::async_trait]
impl BaseCheck for NTPDriftCheck {
    fn name(&self) -> &'static str {
        "NTP"
    }
    fn config_key(&self) -> &'static str {
        "ntpdrift"
    }
    fn default_period(&self) -> u64 {
        60
    }

    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        let server = config
            .ini
            .get_from(Some("Ntp"), "ntp_pool_server")
            .unwrap_or("");
        if server.is_empty() {
            return None;
        }

        let warn = config
            .ini
            .get_from(Some("Ntp"), "ntp_warning_threshold")
            .unwrap_or("1.0")
            .parse::<f64>()
            .unwrap_or(1.0);
        let crit = config
            .ini
            .get_from(Some("Ntp"), "ntp_critical_threshold")
            .unwrap_or("3.0")
            .parse::<f64>()
            .unwrap_or(3.0);

        let client = AsyncSntpClient::new();
        match client.synchronize(server).await {
            Ok(response) => {
                let offset = response.clock_offset().as_secs_f64().abs();
                let mut status = "OK".to_string();
                if offset >= crit {
                    status = "CRITICAL".to_string();
                } else if offset >= warn {
                    status = "WARNING".to_string();
                }
                Some(CheckResult::Single {
                    status,
                    message: format!("NTP Drift Offset: {:.4}s", offset),
                })
            }
            Err(e) => Some(CheckResult::Single {
                status: "CRITICAL".to_string(),
                message: format!("NTP Error: {}", e),
            }),
        }
    }
}
