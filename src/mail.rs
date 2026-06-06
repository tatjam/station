use lettre::{Message, SmtpTransport, Transport, transport::smtp::authentication::Credentials};
use tracing::{error, info};

#[derive(Clone)]
pub struct MailConfig {
    pub smtp_host: String,
    pub smtp_user: String,
    pub smtp_pass: String,
    pub from: String,
    pub to: String,
}

impl MailConfig {
    pub fn new() -> Self {
        MailConfig {
            smtp_host: dotenvy::var("SMTP_HOST").unwrap(),
            smtp_user: dotenvy::var("SMTP_USER").unwrap(),
            smtp_pass: dotenvy::var("SMTP_PASSWORD").unwrap(),
            from: dotenvy::var("SMTP_FROM").unwrap(),
            to: dotenvy::var("SMTP_TO").unwrap(),
        }
    }

    pub fn send(&self, subject: &str, body: String) {
        if self.smtp_host == "disable" {
            info!("Skipping sending email, as SMTP is disabled");
            return;
        }

        let email = match Message::builder()
            .from(self.from.parse().unwrap())
            .to(self.to.parse().unwrap())
            .subject(subject)
            .body(body)
        {
            Ok(e) => e,
            Err(e) => {
                error!("Failed to build email: {}", e);
                return;
            }
        };

        let creds = Credentials::new(self.smtp_user.clone(), self.smtp_pass.clone());
        let mailer = match SmtpTransport::relay(&self.smtp_host) {
            Ok(m) => m.credentials(creds).build(),
            Err(e) => {
                error!("Failed to build mailer: {}", e);
                return;
            }
        };

        if let Err(e) = mailer.send(&email) {
            error!("Failed to send email: {}", e);
        }
    }
}
