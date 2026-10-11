//! Outgoing email behind a trait (ENT-AUTH-012/013/014).
//!
//! * [`LogMailer`]: writes the message to the log (development default).
//!   The body contains single-use links, so never use it in production.
//! * [`SmtpMailer`]: `lettre` async SMTP, plain connection. Meant for a local
//!   relay or Mailpit (`localhost:1025`); TLS is not enabled (open question).
//! * [`MemoryMailer`]: captures messages for tests.

use async_trait::async_trait;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, MultiPart},
};
use std::sync::{Mutex, MutexGuard};

/// A message to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingMail {
    /// Recipient address.
    pub to: String,
    /// Subject line.
    pub subject: String,
    /// Plain-text body.
    pub text: String,
    /// HTML body.
    pub html: String,
}

/// Mail delivery failure. The message never contains the mail body.
#[derive(Debug, thiserror::Error)]
#[error("mail delivery failed: {0}")]
pub struct MailError(pub String);

/// Sends email.
#[async_trait]
pub trait Mailer: Send + Sync {
    /// Deliver (or enqueue) one message.
    async fn send(&self, mail: OutgoingMail) -> Result<(), MailError>;
}

/// Logs messages instead of sending them.
#[derive(Debug, Default)]
pub struct LogMailer;

#[async_trait]
impl Mailer for LogMailer {
    async fn send(&self, mail: OutgoingMail) -> Result<(), MailError> {
        tracing::info!(
            target: "sr_api::mail",
            to = %mail.to,
            subject = %mail.subject,
            body = %mail.text,
            "outgoing mail (LogMailer, development only)"
        );
        Ok(())
    }
}

/// Captures messages in memory.
#[derive(Debug, Default)]
pub struct MemoryMailer(Mutex<Vec<OutgoingMail>>);

impl MemoryMailer {
    /// Empty outbox.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<OutgoingMail>> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Messages sent so far.
    pub fn sent(&self) -> Vec<OutgoingMail> {
        self.lock().clone()
    }
}

#[async_trait]
impl Mailer for MemoryMailer {
    async fn send(&self, mail: OutgoingMail) -> Result<(), MailError> {
        self.lock().push(mail);
        Ok(())
    }
}

/// SMTP delivery through `lettre` (no TLS, see module docs).
pub struct SmtpMailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpMailer {
    /// Connect to `host:port` (e.g. `localhost:1025` for Mailpit) and send
    /// as `from` (e.g. `SIDE ROOM <no-reply@side.room>`).
    pub fn new(host: &str, port: u16, from: &str) -> Result<Self, MailError> {
        let from = from
            .parse::<Mailbox>()
            .map_err(|e| MailError(format!("invalid from address: {e}")))?;
        let transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
            .port(port)
            .build();
        Ok(Self { transport, from })
    }
}

#[async_trait]
impl Mailer for SmtpMailer {
    async fn send(&self, mail: OutgoingMail) -> Result<(), MailError> {
        let to = mail
            .to
            .parse::<Mailbox>()
            .map_err(|e| MailError(format!("invalid recipient: {e}")))?;
        let msg = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(mail.subject)
            .multipart(MultiPart::alternative_plain_html(mail.text, mail.html))
            .map_err(|e| MailError(e.to_string()))?;
        self.transport
            .send(msg)
            .await
            .map(|_| ())
            .map_err(|e| MailError(e.to_string()))
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn wrap(heading: &str, intro: &str, link: &str, button: &str, outro: &str) -> (String, String) {
    let text = format!("{heading}\n\n{intro}\n\n{link}\n\n{outro}\n\nSIDE ROOM\n");
    let (heading, intro, outro, button) = (
        html_escape(heading),
        html_escape(intro),
        html_escape(outro),
        html_escape(button),
    );
    let href = html_escape(link);
    let html = format!(
        "<!doctype html><html><body style=\"margin:0;background:#14110f;color:#eee6d8;\
         font-family:Georgia,serif\"><div style=\"max-width:480px;margin:0 auto;padding:32px\">\
         <h1 style=\"font-weight:normal\">{heading}</h1><p>{intro}</p>\
         <p><a href=\"{href}\" style=\"display:inline-block;padding:12px 20px;background:#c9a24b;\
         color:#14110f;text-decoration:none;border-radius:6px\">{button}</a></p>\
         <p style=\"color:#9a9082;font-size:13px\">{outro}</p>\
         <p style=\"color:#9a9082;font-size:13px\">SIDE ROOM</p></div></body></html>"
    );
    (text, html)
}

/// Email verification message.
pub fn verify_email(to: &str, link: &str) -> OutgoingMail {
    let (text, html) = wrap(
        "Confirm your email",
        "Welcome in. Confirm this address so you can invite others and recover your account.",
        link,
        "Confirm email",
        "If you did not create a SIDE ROOM account, ignore this message. The link expires in 24 hours.",
    );
    OutgoingMail {
        to: to.to_owned(),
        subject: "Confirm your email for SIDE ROOM".to_owned(),
        text,
        html,
    }
}

/// Password reset message.
pub fn reset_password(to: &str, link: &str) -> OutgoingMail {
    let (text, html) = wrap(
        "Reset your password",
        "Use this link to choose a new password. It works once and expires in 30 minutes.",
        link,
        "Choose a new password",
        "If you did not ask for this, ignore this message. Your password has not changed.",
    );
    OutgoingMail {
        to: to.to_owned(),
        subject: "Reset your SIDE ROOM password".to_owned(),
        text,
        html,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_contain_link_and_escape() {
        let m = verify_email("a@b.c", "https://x.test/verify-email/tok?a=1&b=2");
        assert!(m.text.contains("https://x.test/verify-email/tok?a=1&b=2"));
        assert!(m.html.contains("a=1&amp;b=2"));
        let r = reset_password("a@b.c", "https://x.test/reset/t");
        assert!(r.subject.contains("password"));
    }

    #[tokio::test]
    async fn memory_mailer_captures() {
        let m = MemoryMailer::new();
        m.send(verify_email("a@b.c", "l")).await.unwrap();
        assert_eq!(m.sent().len(), 1);
    }

    #[test]
    fn smtp_rejects_bad_from() {
        assert!(SmtpMailer::new("localhost", 1025, "not an address").is_err());
    }
}
