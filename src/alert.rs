use crate::config::AppConfig;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use std::sync::Arc;

pub struct SMTPAlert {
    smtp_server: String,
    smtp_port: u16,
    sender_email: String,
    receiver_email: String,
    smtp_username: Option<String>,
    smtp_password: Option<String>,
}

impl SMTPAlert {
    pub fn new(config: &AppConfig) -> Self {
        let server = config
            .ini
            .get_from(Some("Email"), "smtp_server")
            .unwrap_or("")
            .to_string();
        let port = config
            .ini
            .get_from(Some("Email"), "smtp_port")
            .unwrap_or("25")
            .parse::<u16>()
            .unwrap_or(25);
        let sender = config
            .ini
            .get_from(Some("Email"), "sender_email")
            .unwrap_or("")
            .to_string();
        let receiver = config
            .ini
            .get_from(Some("Email"), "receiver_email")
            .unwrap_or("")
            .to_string();
        let username = config
            .ini
            .get_from(Some("Email"), "smtp_username")
            .map(|s| s.to_string());
        let password = config
            .ini
            .get_from(Some("Email"), "smtp_password")
            .map(|s| s.to_string());

        SMTPAlert {
            smtp_server: server,
            smtp_port: port,
            sender_email: sender,
            receiver_email: receiver,
            smtp_username: username,
            smtp_password: password,
        }
    }

    pub async fn send_alert(self: Arc<Self>, subject: String, body: String) {
        if self.smtp_server.is_empty() {
            return;
        }

        let email = match Message::builder()
            .from(self.sender_email.parse().unwrap())
            .to(self.receiver_email.parse().unwrap())
            .subject(subject)
            .body(body)
        {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("Failed to build email message: {}", e);
                return;
            }
        };

        let mut mailer = match AsyncSmtpTransport::<Tokio1Executor>::relay(&self.smtp_server) {
            Ok(m) => m.port(self.smtp_port),
            Err(e) => {
                eprintln!("Failed to create SMTP transport: {}", e);
                return;
            }
        };

        if let (Some(user), Some(pass)) = (&self.smtp_username, &self.smtp_password) {
            if !user.is_empty() && !pass.is_empty() {
                mailer = mailer.credentials(Credentials::new(user.clone(), pass.clone()));
            }
        }

        let mailer = mailer.build();
        if let Err(e) = mailer.send(email).await {
            eprintln!("Failed to send SMTP alert: {}", e);
        }
    }
}
