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
    transport::smtp::{authentication::Credentials, client::Tls},
};
use sr_core::config::{SmtpConfig, SmtpTls};
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

/// SMTP delivery through `lettre` with rustls (`tokio1-rustls-tls`, pure
/// Rust TLS; open question 145). Built from the `[smtp]` config section.
pub struct SmtpMailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpMailer {
    /// Build the transport from `[smtp]`: `tls = "starttls"` (default, port
    /// 587), `"tls"` (implicit TLS, port 465) or `"none"` (plaintext, for a
    /// local relay such as Mailpit only). Credentials are used when `username`
    /// is set. Connects lazily, on the first send.
    pub fn from_config(cfg: &SmtpConfig) -> Result<Self, MailError> {
        let from = cfg
            .from
            .parse::<Mailbox>()
            .map_err(|e| MailError(format!("invalid from address: {e}")))?;
        let mut builder = match cfg.tls {
            SmtpTls::Starttls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.host)
                .map_err(|e| MailError(e.to_string()))?,
            SmtpTls::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.host)
                .map_err(|e| MailError(e.to_string()))?,
            SmtpTls::None => {
                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&cfg.host).tls(Tls::None)
            }
        };
        builder = builder.port(cfg.port);
        if !cfg.username.is_empty() {
            builder = builder.credentials(Credentials::new(
                cfg.username.clone(),
                cfg.password.expose().to_owned(),
            ));
        }
        Ok(Self {
            transport: builder.build(),
            from,
        })
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

/// Confirm a new address typed in Settings. Sent to the new address only.
pub fn change_email(to: &str, link: &str) -> OutgoingMail {
    let (text, html) = wrap(
        "Confirm your new email",
        "Confirm this address to use it for your SIDE ROOM account. Until you do, your old address stays in place.",
        link,
        "Confirm new email",
        "If you did not ask for this, ignore this message. The link expires in 24 hours.",
    );
    OutgoingMail {
        to: to.to_owned(),
        subject: "Confirm your new email for SIDE ROOM".to_owned(),
        text,
        html,
    }
}

/// A link-less security notice (`heading`, then `body`).
fn notice(to: &str, subject: &str, heading: &str, body: &str) -> OutgoingMail {
    let text = format!("{heading}\n\n{body}\n\nSIDE ROOM\n");
    let (h, b) = (html_escape(heading), html_escape(body));
    let html = format!(
        "<!doctype html><html><body style=\"margin:0;background:#14110f;color:#eee6d8;\
         font-family:Georgia,serif\"><div style=\"max-width:480px;margin:0 auto;padding:32px\">\
         <h1 style=\"font-weight:normal\">{h}</h1><p>{b}</p>\
         <p style=\"color:#9a9082;font-size:13px\">SIDE ROOM</p></div></body></html>"
    );
    OutgoingMail {
        to: to.to_owned(),
        subject: subject.to_owned(),
        text,
        html,
    }
}

/// Sent to the old address once an email change is confirmed.
pub fn email_changed(to: &str) -> OutgoingMail {
    notice(
        to,
        "Your SIDE ROOM email was changed",
        "Your email was changed",
        "The email address on your SIDE ROOM account was just changed to a new one. If this was not you, reset your password right away and contact the operator.",
    )
}

/// Sent after a signed-in password change.
pub fn password_changed(to: &str) -> OutgoingMail {
    notice(
        to,
        "Your SIDE ROOM password was changed",
        "Your password was changed",
        "The password on your SIDE ROOM account was just changed. If this was not you, reset your password right away and contact the operator.",
    )
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

/// Notice that two-factor sign-in was reset by an operator (open question 47).
/// No link: the message only says what happened and what to do next.
pub fn two_factor_reset(to: &str) -> OutgoingMail {
    let text = "Two-factor sign-in was reset on your SIDE ROOM account by an operator.\n\n\
        Your authenticator app, recovery codes and passkeys were removed, and every \
        signed-in browser was signed out. Sign in with your password and set up two-factor \
        sign-in again. If you did not expect this, contact the operator.\n\nSIDE ROOM\n"
        .to_owned();
    let html = "<!doctype html><html><body style=\"margin:0;background:#14110f;color:#eee6d8;\
        font-family:Georgia,serif\"><div style=\"max-width:480px;margin:0 auto;padding:32px\">\
        <h1 style=\"font-weight:normal\">Two-factor sign-in was reset</h1>\
        <p>An operator removed the second factors from your account and signed out every \
        browser. Sign in with your password and set up two-factor sign-in again.</p>\
        <p style=\"color:#9a9082;font-size:13px\">If you did not expect this, contact the \
        operator.</p><p style=\"color:#9a9082;font-size:13px\">SIDE ROOM</p></div></body></html>"
        .to_owned();
    OutgoingMail {
        to: to.to_owned(),
        subject: "Two-factor sign-in was reset on SIDE ROOM".to_owned(),
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
        let cfg = SmtpConfig {
            host: "localhost".into(),
            port: 1025,
            tls: SmtpTls::None,
            from: "not an address".into(),
            ..SmtpConfig::default()
        };
        assert!(SmtpMailer::from_config(&cfg).is_err());
    }

    #[test]
    fn smtp_builds_for_each_tls_mode() {
        for tls in [SmtpTls::Starttls, SmtpTls::Tls, SmtpTls::None] {
            let cfg = SmtpConfig {
                host: "mail.example.org".into(),
                tls,
                from: "SIDE ROOM <no-reply@example.org>".into(),
                ..SmtpConfig::default()
            };
            assert!(SmtpMailer::from_config(&cfg).is_ok(), "{tls:?}");
        }
    }
}
