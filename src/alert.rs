use crate::config::AppConfig;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use std::sync::Arc;

pub struct SMTPAlert {
    config: AppConfig,
}

impl SMTPAlert {
    pub fn new(config: &AppConfig) -> Self {
        SMTPAlert {
            config: config.clone(),
        }
    }

    pub async fn send_nagios_hard_alert(
        self: Arc<Self>,
        check_key: &str,
        category: &str,
        status: &str,
        message: &str,
    ) {
        if self.config.email.smtp_server.is_empty() {
            return;
        }

        // Récupération de la description du service si disponible
        let service_desc = self
            .config
            .services
            .get(check_key)
            .map(|s| s.description.as_str())
            .unwrap_or(category);

        for contact in self.config.contacts.values() {
            if contact.wants_notification(status, &self.config.timeperiods) {
                let subject = format!(
                    "** Nagios HARD Alert [{}] - {} on {} **",
                    status, service_desc, self.config.system.hostname
                );

                let body = format!(
                    "***** Smith-RS Nagios-Style Alert *****\n\n\
                     Notification Type: HARD STATE\n\
                     Service: {}\n\
                     Host: {}\n\
                     State: {}\n\
                     Recipient Alias: {}\n\
                     Recipient Email: {}\n\n\
                     Additional Info:\n{}",
                    service_desc,
                    self.config.system.hostname,
                    status,
                    contact.alias,
                    contact.email,
                    message
                );

                let email = match Message::builder()
                    .from(self.config.email.sender_email.parse().unwrap())
                    .to(contact.email.parse().unwrap())
                    .subject(subject)
                    .body(body)
                {
                    Ok(msg) => msg,
                    Err(_) => continue,
                };

                let mut mailer = match AsyncSmtpTransport::<Tokio1Executor>::relay(
                    &self.config.email.smtp_server,
                ) {
                    Ok(m) => m.port(self.config.email.smtp_port),
                    Err(_) => return,
                };

                if let (Some(user), Some(pass)) = (
                    &self.config.email.smtp_username,
                    &self.config.email.smtp_password,
                ) {
                    if !user.is_empty() && !pass.is_empty() {
                        mailer = mailer.credentials(Credentials::new(user.clone(), pass.clone()));
                    }
                }

                let mailer = mailer.build();
                tokio::spawn(async move {
                    let _ = mailer.send(email).await;
                });
            }
        }
    }
}
