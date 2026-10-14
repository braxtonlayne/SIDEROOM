//! Locale-keyed text for outgoing email (XC-I18N-003).
//!
//! Every message the server mails is a [`MailKind`]. The words live here, in one catalog per locale, so a
//! translation is a new [`Catalog`] entry and nothing else: the layout, escaping and links stay in
//! [`super::mail`]. Only English exists today.
//!
//! The locale comes from the account's `locale` (a BCP 47 tag such as `en-US`). [`negotiate`] maps it to a
//! catalog by exact tag, then by primary language (`pt-BR` falls back to `pt`), then to [`DEFAULT_LOCALE`].
//! A locale that has a catalog but lacks one kind falls back to English for that kind, so a half-finished
//! translation never sends a blank mail.

/// The catalog every other locale falls back to.
pub const DEFAULT_LOCALE: &str = "en";

/// Which message to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailKind {
    /// Welcome and confirm the address.
    VerifyEmail,
    /// Confirm a new address typed in Settings (sent to the new address).
    ChangeEmail,
    /// Warn the old address that a change started; carries `{new_email}`.
    ChangeEmailNotice,
    /// Tell the old address the change went through.
    EmailChanged,
    /// A signed-in password change happened.
    PasswordChanged,
    /// Reset-password link.
    ResetPassword,
    /// An operator reset two-factor sign-in.
    TwoFactorReset,
}

/// The words of one message. `{name}` placeholders are filled by [`fill`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Template {
    /// Subject line.
    pub subject: &'static str,
    /// Heading.
    pub heading: &'static str,
    /// Main paragraph.
    pub body: &'static str,
    /// Button label (empty for a link-less notice).
    pub button: &'static str,
    /// Small print under the button (empty for none).
    pub outro: &'static str,
}

/// One locale's templates.
#[derive(Debug, Clone, Copy)]
pub struct Catalog {
    /// Language tag, lower case (`en`, `pt-br`).
    pub locale: &'static str,
    /// Messages this locale translates; the rest fall back to English.
    pub templates: &'static [(MailKind, Template)],
}

const EN: Catalog = Catalog {
    locale: "en",
    templates: &[
        (
            MailKind::VerifyEmail,
            Template {
                subject: "Confirm your email for SIDE ROOM",
                heading: "Confirm your email",
                body: "Welcome in. Confirm this address so you can invite others and recover your account.",
                button: "Confirm email",
                outro: "If you did not create a SIDE ROOM account, ignore this message. The link expires in 24 hours.",
            },
        ),
        (
            MailKind::ChangeEmail,
            Template {
                subject: "Confirm your new email for SIDE ROOM",
                heading: "Confirm your new email",
                body: "Confirm this address to use it for your SIDE ROOM account. Until you do, your old address stays in place.",
                button: "Confirm new email",
                outro: "If you did not ask for this, ignore this message. The link expires in 24 hours.",
            },
        ),
        (
            MailKind::ChangeEmailNotice,
            Template {
                subject: "Your SIDE ROOM email is being changed",
                heading: "Your email is about to change",
                body: "Someone asked to change the email on your SIDE ROOM account to {new_email}. Nothing changes until that address is confirmed. If this was you, you can ignore this message.",
                button: "This wasn't me",
                outro: "If this was not you, use the button to cancel the change. If it already went through, the same link signs every device out. The link works for 7 days.",
            },
        ),
        (
            MailKind::EmailChanged,
            Template {
                subject: "Your SIDE ROOM email was changed",
                heading: "Your email was changed",
                body: "The email address on your SIDE ROOM account was just changed to a new one. If this was you, no action is needed.",
                button: "This wasn't me",
                outro: "If this was not you, use the button to sign out every device, then reset your password and contact the operator. The link works for 7 days.",
            },
        ),
        (
            MailKind::PasswordChanged,
            Template {
                subject: "Your SIDE ROOM password was changed",
                heading: "Your password was changed",
                body: "The password on your SIDE ROOM account was just changed. If this was not you, reset your password right away and contact the operator.",
                button: "",
                outro: "",
            },
        ),
        (
            MailKind::ResetPassword,
            Template {
                subject: "Reset your SIDE ROOM password",
                heading: "Reset your password",
                body: "Use this link to choose a new password. It works once and expires in 30 minutes.",
                button: "Choose a new password",
                outro: "If you did not ask for this, ignore this message. Your password has not changed.",
            },
        ),
        (
            MailKind::TwoFactorReset,
            Template {
                subject: "Two-factor sign-in was reset on SIDE ROOM",
                heading: "Two-factor sign-in was reset",
                body: "Two-factor sign-in was reset on your SIDE ROOM account by an operator. Your authenticator app, recovery codes and passkeys were removed, and every signed-in browser was signed out. Sign in with your password and set up two-factor sign-in again. If you did not expect this, contact the operator.",
                button: "",
                outro: "",
            },
        ),
    ],
};

/// Every catalog, English first.
static CATALOGS: &[Catalog] = &[EN];

fn catalog(locale: &str) -> Option<&'static Catalog> {
    CATALOGS.iter().find(|c| c.locale == locale)
}

/// Pick the catalog for an account's locale tag: exact match, then the primary language, then English.
/// Case and `_` versus `-` do not matter. The result is the catalog's own tag.
pub fn negotiate(preferred: &str) -> &'static str {
    let tag = preferred.trim().replace('_', "-").to_ascii_lowercase();
    if let Some(c) = catalog(&tag) {
        return c.locale;
    }
    let primary = tag.split('-').next().unwrap_or("");
    catalog(primary).map_or(DEFAULT_LOCALE, |c| c.locale)
}

/// The template for `kind` in `preferred`'s catalog, or the English one when that locale lacks it.
pub fn template(preferred: &str, kind: MailKind) -> &'static Template {
    let find = |c: &'static Catalog| c.templates.iter().find(|(k, _)| *k == kind).map(|(_, t)| t);
    catalog(negotiate(preferred))
        .and_then(find)
        .or_else(|| find(&EN))
        .unwrap_or_else(|| unreachable!("the English catalog covers every MailKind"))
}

/// Replace `{name}` placeholders. Unknown placeholders are left as they are.
pub fn fill(text: &str, vars: &[(&str, &str)]) -> String {
    vars.iter().fold(text.to_owned(), |acc, (name, value)| {
        acc.replace(&format!("{{{name}}}"), value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [MailKind; 7] = [
        MailKind::VerifyEmail,
        MailKind::ChangeEmail,
        MailKind::ChangeEmailNotice,
        MailKind::EmailChanged,
        MailKind::PasswordChanged,
        MailKind::ResetPassword,
        MailKind::TwoFactorReset,
    ];

    #[test]
    fn english_covers_every_kind_with_a_subject_heading_and_body() {
        for kind in ALL {
            let t = EN
                .templates
                .iter()
                .find(|(k, _)| *k == kind)
                .map(|(_, t)| t)
                .unwrap_or_else(|| panic!("{kind:?} missing from the English catalog"));
            assert!(!t.subject.is_empty() && !t.heading.is_empty() && !t.body.is_empty());
        }
    }

    #[test]
    fn negotiates_exact_then_primary_then_default() {
        assert_eq!(negotiate("en"), "en");
        assert_eq!(negotiate("en-US"), "en");
        assert_eq!(negotiate("EN_gb"), "en");
        assert_eq!(negotiate("fr-CA"), DEFAULT_LOCALE);
        assert_eq!(negotiate(""), DEFAULT_LOCALE);
        assert_eq!(negotiate("not a locale"), DEFAULT_LOCALE);
    }

    #[test]
    fn unknown_locales_get_the_english_template() {
        for kind in ALL {
            assert_eq!(template("ja-JP", kind), template("en", kind));
        }
    }

    #[test]
    fn a_partial_catalog_falls_back_per_kind() {
        // A locale that translates only one message still sends every other message in English.
        static PARTIAL: Catalog = Catalog {
            locale: "xx",
            templates: &[(
                MailKind::ResetPassword,
                Template {
                    subject: "S",
                    heading: "H",
                    body: "B",
                    button: "",
                    outro: "",
                },
            )],
        };
        let find = |c: &'static Catalog, kind| {
            c.templates.iter().find(|(k, _)| *k == kind).map(|(_, t)| t)
        };
        assert!(find(&PARTIAL, MailKind::ResetPassword).is_some());
        assert!(find(&PARTIAL, MailKind::VerifyEmail).is_none());
        assert!(find(&EN, MailKind::VerifyEmail).is_some());
    }

    #[test]
    fn fills_placeholders() {
        let t = template("en", MailKind::ChangeEmailNotice);
        let s = fill(t.body, &[("new_email", "new@x.test")]);
        assert!(s.contains("to new@x.test."));
        assert!(!s.contains('{'));
        assert_eq!(fill("{a} {b}", &[("a", "1")]), "1 {b}");
    }
}
