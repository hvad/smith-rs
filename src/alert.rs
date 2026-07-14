use crate::config::AppConfig;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};

pub type SMTPAlert = AlertSystem;

pub struct AlertSystem {
    config: AppConfig,
}

impl AlertSystem {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    pub async fn send_email_alert(
        &self,
        to_email: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), String> {
        let email = Message::builder()
            .from(
                self.config
                    .email
                    .sender_email
                    .parse()
                    .map_err(|e| format!("Invalid sender: {}", e))?,
            )
            .to(to_email
                .parse()
                .map_err(|e| format!("Invalid receiver: {}", e))?)
            .subject(subject)
            .body(body.to_string())
            .map_err(|e| format!("Message building failed: {}", e))?;

        let mut transport_builder = SmtpTransport::relay(&self.config.email.smtp_server)
            .map_err(|e| format!("Invalid SMTP relay server path: {}", e))?
            .port(self.config.email.smtp_port);

        if let (Some(user), Some(pass)) = (
            &self.config.email.smtp_username,
            &self.config.email.smtp_password,
        ) {
            let creds = Credentials::new(user.to_string(), pass.to_string());
            transport_builder = transport_builder.credentials(creds);
        }

        let transport = transport_builder.build();

        match transport.send(&email) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!(
                "Could not send alert notification payload over SMTP: {}",
                e
            )),
        }
    }

    /// State is now accepted as a string slice (&str) to match engine expectations
    pub async fn send_nagios_hard_alert(
        &self,
        check_name: &str,
        contact_email: &str,
        state_str: &str,
        message: &str,
    ) -> Result<(), String> {
        let subject = format!("** HARD ALERT ** - {} is {}", check_name, state_str);
        let body = format!(
            "Notification Type: HARD ALERT\nService: {}\nState: {}\nHost: {}\n\nInfo:\n{}",
            check_name, state_str, self.config.system.hostname, message
        );

        self.send_email_alert(contact_email, &subject, &body).await
    }
}
