use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::Mailbox,
    transport::smtp::authentication::Credentials,
};

use crate::config::SmtpConfig;

#[derive(Clone)]
pub struct Mailer {
    config: SmtpConfig,
}

impl Mailer {
    pub fn new(config: SmtpConfig) -> Self {
        Self { config }
    }

    pub fn enabled(&self) -> bool {
        self.config.enabled && self.config.host.is_some() && self.config.from.is_some()
    }

    pub async fn send(&self, to: &str, subject: &str, body: &str) -> anyhow::Result<bool> {
        if !self.enabled() {
            return Ok(false);
        }
        let host = self.config.host.as_deref().expect("checked by enabled");
        let from = self.config.from.as_deref().expect("checked by enabled");
        let mut transport = AsyncSmtpTransport::<Tokio1Executor>::relay(host)?
            .port(self.config.port.unwrap_or(587));
        if let (Some(username), Some(password)) = (&self.config.username, &self.config.password) {
            transport = transport.credentials(Credentials::new(username.clone(), password.clone()));
        }
        let mailer = transport.build();
        let message = Message::builder()
            .from(from.parse::<Mailbox>()?)
            .to(to.parse::<Mailbox>()?)
            .subject(subject)
            .body(body.to_string())?;
        mailer.send(message).await?;
        Ok(true)
    }
}
