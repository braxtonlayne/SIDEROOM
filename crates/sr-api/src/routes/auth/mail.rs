//! Outgoing email behind a trait (ENT-AUTH-012/013/014).
//!
//! * [`LogMailer`]: writes the message to the log (development default).
//!   The body contains single-use links, so never use it in production.
//! * [`SmtpMailer`]: `lettre` async SMTP, plain connection. Meant for a local
//!   relay or Mailpit (`localhost:1025`); TLS is not enabled (open question).
//! * [`MemoryMailer`]: captures messages for tests.

use super::mail_text::{self, MailKind};
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

    /// Messages sent so far, emptying the outbox.
    pub fn take(&self) -> Vec<OutgoingMail> {
        std::mem::take(&mut *self.lock())
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

pub(super) fn html_escape(s: &str) -> String {
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

/// Build one message from the locale-keyed catalog ([`mail_text`]). `link` selects the layout: a message
/// with a link gets the button, one without is a plain notice. `vars` fills `{name}` placeholders.
fn render(
    to: &str,
    locale: &str,
    kind: MailKind,
    link: Option<&str>,
    vars: &[(&str, &str)],
) -> OutgoingMail {
    let t = mail_text::template(locale, kind);
    let body = mail_text::fill(t.body, vars);
    match link {
        Some(link) => {
            let (text, html) = wrap(t.heading, &body, link, t.button, t.outro);
            OutgoingMail {
                to: to.to_owned(),
                subject: t.subject.to_owned(),
                text,
                html,
            }
        }
        None => notice(to, t.subject, t.heading, &body),
    }
}

/// Email verification message. `locale` is the account's BCP 47 tag (unknown tags fall back to English).
pub fn verify_email(to: &str, link: &str, locale: &str) -> OutgoingMail {
    render(to, locale, MailKind::VerifyEmail, Some(link), &[])
}

/// Confirm a new address typed in Settings. Sent to the new address only.
pub fn change_email(to: &str, link: &str, locale: &str) -> OutgoingMail {
    render(to, locale, MailKind::ChangeEmail, Some(link), &[])
}

/// Sent to the *old* address when an email change starts: the account holder can
/// cancel it with the "This wasn't me" link (D-167).
pub fn change_email_notice(
    to: &str,
    new_email: &str,
    cancel_link: &str,
    locale: &str,
) -> OutgoingMail {
    render(
        to,
        locale,
        MailKind::ChangeEmailNotice,
        Some(cancel_link),
        &[("new_email", new_email)],
    )
}

/// Sent to the old address once an email change is confirmed. The link signs every
/// device out (D-167).
pub fn email_changed(to: &str, cancel_link: &str, locale: &str) -> OutgoingMail {
    render(to, locale, MailKind::EmailChanged, Some(cancel_link), &[])
}

/// Sent after a signed-in password change.
pub fn password_changed(to: &str, locale: &str) -> OutgoingMail {
    render(to, locale, MailKind::PasswordChanged, None, &[])
}

/// Password reset message.
pub fn reset_password(to: &str, link: &str, locale: &str) -> OutgoingMail {
    render(to, locale, MailKind::ResetPassword, Some(link), &[])
}

/// Notice that two-factor sign-in was reset by an operator (open question 47).
/// No link: the message only says what happened and what to do next.
pub fn two_factor_reset(to: &str, locale: &str) -> OutgoingMail {
    render(to, locale, MailKind::TwoFactorReset, None, &[])
}

/// The account holder changed their own two-factor setup. `change` is a short sentence fragment such as
/// "An authenticator app was turned on".
pub fn two_factor_changed(to: &str, change: &str, locale: &str) -> OutgoingMail {
    render(
        to,
        locale,
        MailKind::TwoFactorChanged,
        None,
        &[("change", change)],
    )
}

/// A sign-in from a device the account has not used before (ENT-AUTH-015). `when` is a UTC timestamp text.
pub fn new_login(to: &str, device: &str, when: &str, locale: &str) -> OutgoingMail {
    render(
        to,
        locale,
        MailKind::NewLogin,
        None,
        &[("device", device), ("when", when)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_contain_link_and_escape() {
        let m = verify_email("a@b.c", "https://x.test/verify-email/tok?a=1&b=2", "en-US");
        assert!(m.text.contains("https://x.test/verify-email/tok?a=1&b=2"));
        assert!(m.html.contains("a=1&amp;b=2"));
        let r = reset_password("a@b.c", "https://x.test/reset/t", "en");
        assert!(r.subject.contains("password"));
    }

    #[test]
    fn unknown_locale_sends_the_english_text() {
        let en = verify_email("a@b.c", "https://x.test/l", "en");
        for tag in ["fr-CA", "", "zz_ZZ", "EN-us"] {
            assert_eq!(verify_email("a@b.c", "https://x.test/l", tag), en, "{tag}");
        }
        assert_eq!(en.subject, "Confirm your email for SIDE ROOM");
    }

    #[test]
    fn every_message_is_built_from_the_catalog() {
        let n = change_email_notice("o@x.test", "n@x.test", "https://x.test/c", "en");
        assert!(n.text.contains("to n@x.test."));
        assert!(n.html.contains("This wasn't me"));
        let p = password_changed("a@b.c", "en");
        assert_eq!(p.subject, "Your SIDE ROOM password was changed");
        assert!(!p.text.contains("http"));
        let t = two_factor_reset("a@b.c", "en");
        assert!(t.text.contains("recovery codes"));
        assert!(
            email_changed("a@b.c", "https://x.test/c", "en")
                .text
                .contains("https://x.test/c")
        );
    }

    #[test]
    fn security_notices_name_what_happened() {
        let n = new_login("a@b.c", "Firefox on Windows", "2026-10-15 09:30 UTC", "en");
        assert!(n.text.contains("Firefox on Windows"));
        assert!(n.text.contains("2026-10-15 09:30 UTC"));
        assert!(!n.text.contains("http"));
        let c = two_factor_changed("a@b.c", "A passkey was added", "en");
        assert!(
            c.text
                .contains("A passkey was added on your SIDE ROOM account")
        );
    }

    #[tokio::test]
    async fn memory_mailer_captures() {
        let m = MemoryMailer::new();
        m.send(verify_email("a@b.c", "l", "en")).await.unwrap();
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
