//! Operator policy over who may connect a bucket and where (BYOS-FLG-001): the pure rules behind the
//! `byos.who_may_mount` and `byos.endpoint_allow` settings. The settings live in `sr-admin`; the routes
//! read them and ask the functions here, so the rules are unit-tested without a server.
//!
//! Neither rule touches end-to-end encryption, authorization or the audit logs: those are not settings
//! (the IMMUTABLE RULE, docs/dev/roadmap-p27.md section 2).

use async_trait::async_trait;

/// `byos.who_may_mount`: who may connect a bucket of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhoMayMount {
    /// Every signed-in member (the default).
    All,
    /// Patrons and staff.
    Patrons,
    /// Staff only.
    Staff,
}

impl WhoMayMount {
    /// The stored text. Empty means the default (everyone). A value nobody recognises fails closed to
    /// staff only, so a hand-edited row never opens the door wider than the operator meant.
    pub fn parse(stored: &str) -> Self {
        match stored.trim().to_ascii_lowercase().as_str() {
            "" | "all" => Self::All,
            "patrons" => Self::Patrons,
            _ => Self::Staff,
        }
    }

    /// Whether a member with these standings may connect a bucket.
    pub fn admits(self, is_patron: bool, is_staff: bool) -> bool {
        match self {
            Self::All => true,
            Self::Patrons => is_patron || is_staff,
            Self::Staff => is_staff,
        }
    }

    /// Whether the answer needs the member's standings at all.
    pub fn is_open(self) -> bool {
        self == Self::All
    }
}

/// The facts `WhoMayMount` needs about a member.
#[async_trait]
pub trait MemberStanding: Send + Sync {
    /// An unrevoked, unexpired patron entitlement.
    async fn is_patron(&self, user_id: i64) -> Result<bool, String>;
    /// An unrevoked staff appointment.
    async fn is_staff(&self, user_id: i64) -> Result<bool, String>;
}

/// `byos.endpoint_allow`: host patterns a bucket endpoint must match. Empty means any endpoint that
/// passes the SSRF policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EndpointAllow {
    patterns: Vec<String>,
}

/// The longest pattern list a setting may hold.
pub const MAX_ALLOW_PATTERNS: usize = 50;

fn items(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
}

impl EndpointAllow {
    /// Parse the stored text: patterns separated by commas, spaces or newlines. A pattern is a host
    /// (`s3.example.com`) or `*.suffix` (any host below `suffix`, not `suffix` itself). Case does not
    /// matter; a port or scheme in a pattern is ignored. An item that is not a pattern is dropped.
    pub fn parse(stored: &str) -> Self {
        let patterns = items(stored)
            .filter_map(normalise)
            .take(MAX_ALLOW_PATTERNS)
            .collect();
        Self { patterns }
    }

    /// Whether the list is empty (every endpoint allowed).
    pub fn is_open(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Whether `host` (no port) may be used as an endpoint.
    pub fn allows(&self, host: &str) -> bool {
        if self.patterns.is_empty() {
            return true;
        }
        let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
        self.patterns.iter().any(|p| match p.strip_prefix("*.") {
            Some(suffix) => host
                .strip_suffix(suffix)
                .is_some_and(|rest| rest.len() > 1 && rest.ends_with('.')),
            None => *p == host,
        })
    }
}

fn normalise(raw: &str) -> Option<String> {
    let s = raw.trim().to_ascii_lowercase();
    let s = s.split("://").last().unwrap_or("");
    let s = s.split('/').next().unwrap_or("");
    // Drop a port, but keep a bracketed IPv6 literal whole.
    let s = if s.starts_with('[') {
        s.split_once(']').map_or(s, |(h, _)| h).to_owned() + "]"
    } else {
        s.split(':').next().unwrap_or("").to_owned()
    };
    let s = s.trim_end_matches('.').to_owned();
    let chars_ok = !s.is_empty()
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '*' | '[' | ']' | ':' | '_')
        });
    let star_ok = !s.contains('*') || (s.starts_with("*.") && s.len() > 2 && !s[2..].contains('*'));
    (chars_ok && star_ok).then_some(s)
}

/// Whether `text` is an acceptable stored value of `byos.endpoint_allow`: every item parses.
pub fn endpoint_allow_text_valid(text: &str) -> Result<(), String> {
    if items(text).count() > MAX_ALLOW_PATTERNS {
        return Err(format!("at most {MAX_ALLOW_PATTERNS} patterns"));
    }
    if let Some(bad) = items(text).find(|i| normalise(i).is_none()) {
        return Err(format!(
            "`{bad}` is not a host or a *.suffix pattern (use s3.example.com or *.example.com)"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn who_may_mount_parses_and_fails_closed() {
        assert_eq!(WhoMayMount::parse(""), WhoMayMount::All);
        assert_eq!(WhoMayMount::parse("ALL"), WhoMayMount::All);
        assert_eq!(WhoMayMount::parse("patrons"), WhoMayMount::Patrons);
        assert_eq!(WhoMayMount::parse("staff"), WhoMayMount::Staff);
        assert_eq!(WhoMayMount::parse("everyone?"), WhoMayMount::Staff);
    }

    #[test]
    fn who_may_mount_admits_by_standing() {
        assert!(WhoMayMount::All.admits(false, false));
        assert!(!WhoMayMount::Patrons.admits(false, false));
        assert!(WhoMayMount::Patrons.admits(true, false));
        assert!(WhoMayMount::Patrons.admits(false, true));
        assert!(!WhoMayMount::Staff.admits(true, false));
        assert!(WhoMayMount::Staff.admits(false, true));
    }

    #[test]
    fn empty_allow_list_allows_everything() {
        let a = EndpointAllow::parse("  ");
        assert!(a.is_open());
        assert!(a.allows("anything.example"));
    }

    #[test]
    fn exact_and_suffix_patterns() {
        let a = EndpointAllow::parse(
            "s3.eu-west-1.amazonaws.com, *.r2.cloudflarestorage.com\nGARAGE.local",
        );
        assert!(a.allows("s3.eu-west-1.amazonaws.com"));
        assert!(a.allows("S3.EU-WEST-1.AMAZONAWS.COM"));
        assert!(a.allows("abc.r2.cloudflarestorage.com"));
        assert!(a.allows("a.b.r2.cloudflarestorage.com"));
        assert!(
            !a.allows("r2.cloudflarestorage.com"),
            "a suffix pattern needs a label"
        );
        assert!(!a.allows("evilr2.cloudflarestorage.com"));
        assert!(!a.allows("r2.cloudflarestorage.com.evil.example"));
        assert!(!a.allows("s3.us-east-1.amazonaws.com"));
        assert!(a.allows("garage.local"));
    }

    #[test]
    fn schemes_ports_and_paths_in_patterns_are_ignored() {
        let a = EndpointAllow::parse("https://s3.example.com:9000/bucket");
        assert!(a.allows("s3.example.com"));
        assert!(!a.allows("other.example.com"));
    }

    #[test]
    fn text_validation_rejects_junk_and_midword_wildcards() {
        assert!(endpoint_allow_text_valid("").is_ok());
        assert!(endpoint_allow_text_valid("a.example.com, *.b.example.com").is_ok());
        assert!(endpoint_allow_text_valid("a*.example.com").is_err());
        assert!(endpoint_allow_text_valid("*").is_err());
        assert!(endpoint_allow_text_valid("bad host!").is_err());
        let many = (0..=MAX_ALLOW_PATTERNS)
            .map(|i| format!("h{i}.example.com"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(endpoint_allow_text_valid(&many).is_err());
    }
}
