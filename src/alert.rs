// Import the global configuration structure
use crate::config::AppConfig;

// Import SMTP authentication helper from the 'lettre' crate
use lettre::transport::smtp::authentication::Credentials;
// Import core email components from the 'lettre' crate
use lettre::{Message, SmtpTransport, Transport};

// Create a type alias. 'SMTPAlert' is now another name for our 'AlertSystem' struct.
// This allows other parts of the code (like the engine) to import it as 'SMTPAlert'.
pub type SMTPAlert = AlertSystem;

/// The alert management system responsible for preparing and sending email notifications
pub struct AlertSystem {
    config: AppConfig, // A local copy of the resolved application configuration
}

impl AlertSystem {
    /// Constructor to initialize the AlertSystem with a reference to our application configuration
    pub fn new(config: &AppConfig) -> Self {
        Self {
            // We clone the configuration so the alert system owns its data independently.
            // This is useful when moving instances across thread boundaries.
            config: config.clone(),
        }
    }

    /// Sends a generic email using SMTP relay servers parsed from the configuration file.
    /// Returns:
    /// - `Ok(())` on a successful email delivery.
    /// - `Err(String)` wrapping a descriptive error message if any step fails.
    pub async fn send_email_alert(
        &self,
        to_email: &str,
        subject: &str,
        body: &str,
    ) -> Result<(), String> {
        // 1. BUILD THE EMAIL MESSAGE
        // We use the Builder pattern to safely construct the email structure.
        let email = Message::builder()
            // Parse the sender address. The '?' operator returns the error early if parsing fails.
            .from(
                self.config
                    .email
                    .sender_email
                    .parse()
                    .map_err(|e| format!("Invalid sender: {}", e))?,
            )
            // Parse the recipient address.
            .to(to_email
                .parse()
                .map_err(|e| format!("Invalid receiver: {}", e))?)
            // Assign the subject line
            .subject(subject)
            // Define the email content body as plain text
            .body(body.to_string())
            // Convert any builder errors into a readable String error
            .map_err(|e| format!("Message building failed: {}", e))?;

        // 2. CONFIGURE THE SMTP RELAY TRANSPORT
        // We set up our connection with the SMTP server using the configured address and port.
        let mut transport_builder = SmtpTransport::relay(&self.config.email.smtp_server)
            .map_err(|e| format!("Invalid SMTP relay server path: {}", e))?
            .port(self.config.email.smtp_port);

        // 3. OPTIONALLY ADD SMTP AUTHENTICATION
        // We use 'if let' matching to check if BOTH username and password are provided (Some).
        // If either is None, the authentication setup is skipped entirely.
        if let (Some(user), Some(pass)) = (
            &self.config.email.smtp_username,
            &self.config.email.smtp_password,
        ) {
            // Supply credentials to our transport builder sequence
            transport_builder =
                transport_builder.credentials(Credentials::new(user.clone(), pass.clone()));
        }

        // 4. ESTABLISH TRANSPORT AND SEND
        // Build the final transport object
        let transport = transport_builder.build();

        // Attempt to deliver the email.
        // We map a successful send to Ok(()) and wrap any SmtpError inside our custom Err(String).
        transport
            .send(&email)
            .map(|_| ())
            .map_err(|e| format!("SMTP failure: {}", e))
    }

    /// Prepares a structured, Nagios-compliant "HARD ALERT" notification payload
    /// and triggers the SMTP send operation.
    pub async fn send_nagios_hard_alert(
        &self,
        check_name: &str,
        contact_email: &str,
        state_str: &str,
        message: &str,
    ) -> Result<(), String> {
        // Format a standard monitoring subject header line, e.g., "** HARD ALERT ** - Disk Space is CRITICAL"
        let subject = format!("** HARD ALERT ** - {} is {}", check_name, state_str);

        // Format a structured email message block detailing the alert attributes
        let body = format!(
            "Notification Type: HARD ALERT\nService: {}\nState: {}\nHost: {}\n\nInfo:\n{}",
            check_name, state_str, self.config.system.hostname, message
        );

        // Forward the constructed details to our SMTP client execution wrapper
        self.send_email_alert(contact_email, &subject, &body).await
    }
}
