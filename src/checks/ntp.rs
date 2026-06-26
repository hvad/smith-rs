use crate::checks::{BaseCheck, CheckResult};
use crate::config::AppConfig;
use rsntp::AsyncSntpClient;

pub struct NTPDriftCheck;

#[async_trait::async_trait]
impl BaseCheck for NTPDriftCheck {
    fn name(&self) -> &'static str {
        "NTP Drift"
    }
    fn config_key(&self) -> &'static str {
        "ntp"
    }
    fn default_period(&self) -> u64 {
        120
    }

    async fn run(&self, config: &AppConfig) -> Option<CheckResult> {
        let sc = config.services.get(self.config_key())?;
        let server = sc.ntp_pool_server.as_ref()?;
        if server.is_empty() {
            return None;
        }

        let warn = sc.warning;
        let crit = sc.critical;

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
                    message: format!("Offset: {:.4}s", offset),
                })
            }
            Err(e) => Some(CheckResult::Single {
                status: "CRITICAL".to_string(),
                message: format!("NTP Error: {}", e),
            }),
        }
    }
}
