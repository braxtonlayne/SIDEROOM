//! Creator tip links (ARC-ROOM-001, wave 42). A game manifest (`sideroom.json`, `tip_links`) and a `/~handle`
//! profile (the Links block, entries with a `tip` kind) may carry peer-to-peer tip links: a Bitcoin or Lightning
//! payment URI (the browser draws it as a QR code), a Ko-fi page or a Patreon page. The platform takes no cut and
//! never sits between payer and payee, so this module only decides what is an acceptable link:
//!
//! * `kofi` and `patreon` are `https://` links on an allow-listed host ([`TIP_HOSTS`]), no user info, no port,
//!   no fragment, path segments of plain characters;
//! * `btc` is a `bitcoin:` URI with a base58 or bech32 (`bc1`) address and only the `amount`, `label` and
//!   `message` parameters (the address checksum is not verified here; the wallet that scans the QR does);
//! * `lightning` is a `lightning:` URI carrying a BOLT11 invoice, an LNURL or a BOLT12 offer.
//!
//! The web client mirrors these rules in `apps/web/src/arcade/tips.ts`; the server is the authority.

use serde::{Deserialize, Serialize};

/// Most tip links on one game or one profile block.
pub const MAX_TIP_LINKS: usize = 4;
/// Longest Ko-fi, Patreon or Bitcoin link.
pub const MAX_TIP_URL: usize = 512;
/// Longest Lightning link (a BOLT11 invoice can be long).
pub const MAX_LIGHTNING_URL: usize = 1200;

/// The hosts a web tip link may point at, by kind. A host must match exactly; subdomains are not allowed.
pub const TIP_HOSTS: &[(&str, &[&str])] = &[
    ("kofi", &["ko-fi.com", "www.ko-fi.com"]),
    ("patreon", &["patreon.com", "www.patreon.com"]),
];

/// The kinds of tip link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TipKind {
    Btc,
    Lightning,
    Kofi,
    Patreon,
}

impl TipKind {
    pub const ALL: [TipKind; 4] = [Self::Btc, Self::Lightning, Self::Kofi, Self::Patreon];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Btc => "btc",
            Self::Lightning => "lightning",
            Self::Kofi => "kofi",
            Self::Patreon => "patreon",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// Whether the browser draws this kind as a QR code (payment URIs) rather than a link.
    pub fn is_qr(self) -> bool {
        matches!(self, Self::Btc | Self::Lightning)
    }
}

/// Why a tip link is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipError {
    UnknownKind,
    TooLong,
    /// Not the shape the kind needs (scheme, address or invoice).
    Malformed,
    /// A web link on a host that is not on the allow-list.
    HostNotAllowed,
    TooMany,
    /// Two links of one kind.
    Duplicate,
}

impl TipError {
    /// Short text for an error message or a form field.
    pub fn text(self) -> &'static str {
        match self {
            Self::UnknownKind => "is not a tip kind (btc, lightning, kofi or patreon)",
            Self::TooLong => "is too long",
            Self::Malformed => "is not a valid tip link for its kind",
            Self::HostNotAllowed => "must be on ko-fi.com or patreon.com",
            Self::TooMany => "has more than 4 tip links",
            Self::Duplicate => "has two tip links of one kind",
        }
    }
}

const BASE58: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const BECH32: &str = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";

fn all_in(s: &str, set: &str) -> bool {
    s.chars().all(|c| set.contains(c))
}

fn bitcoin_ok(v: &str) -> bool {
    let Some(rest) = v.strip_prefix("bitcoin:") else {
        return false;
    };
    let (addr, query) = match rest.split_once('?') {
        Some((a, q)) => (a, Some(q)),
        None => (rest, None),
    };
    let base58 = (26..=35).contains(&addr.len())
        && (addr.starts_with('1') || addr.starts_with('3'))
        && all_in(addr, BASE58);
    let bech = addr
        .strip_prefix("bc1")
        .is_some_and(|d| (11..=71).contains(&d.len()) && all_in(d, BECH32));
    if !(base58 || bech) {
        return false;
    }
    let Some(q) = query else { return true };
    if q.is_empty() {
        return false;
    }
    let mut seen: Vec<&str> = Vec::new();
    for pair in q.split('&') {
        let Some((k, val)) = pair.split_once('=') else {
            return false;
        };
        if !matches!(k, "amount" | "label" | "message") || seen.contains(&k) {
            return false;
        }
        seen.push(k);
        let plain = !val.is_empty()
            && val.len() <= 80
            && val
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '~' | '%'));
        if !plain {
            return false;
        }
        if k == "amount" && !val.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return false;
        }
    }
    true
}

fn lightning_ok(v: &str) -> bool {
    let Some(rest) = v.strip_prefix("lightning:") else {
        return false;
    };
    let body = ["lnbcrt", "lnbc", "lntbs", "lntb", "lnsb", "lnurl1", "lno1"]
        .iter()
        .find_map(|p| rest.strip_prefix(p));
    let Some(body) = body else { return false };
    // Lower-case letters and digits only (bech32 text); the checksum is the wallet's to verify.
    (20..=1100).contains(&body.len())
        && body
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

fn web_ok(kind: TipKind, v: &str) -> Result<(), TipError> {
    let hosts = TIP_HOSTS
        .iter()
        .find(|(k, _)| *k == kind.as_str())
        .map(|(_, h)| *h)
        .unwrap_or(&[]);
    let Some(rest) = v.strip_prefix("https://") else {
        return Err(TipError::Malformed);
    };
    let (host, tail) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    // No user info, no port, nothing exotic in the host: the allow-list is an exact match.
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
    {
        return Err(TipError::Malformed);
    }
    if !hosts.contains(&host) {
        return Err(TipError::HostNotAllowed);
    }
    let (path, query) = match tail.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (tail, None),
    };
    // A page, not the site's front: at least one path segment.
    let segs: Vec<&str> = path.split('/').skip(1).filter(|s| !s.is_empty()).collect();
    if segs.is_empty() || path.contains("//") || segs.len() > 4 {
        return Err(TipError::Malformed);
    }
    let seg_ok = |s: &str| {
        s != ".."
            && s != "."
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '~' | '%'))
    };
    if !segs.iter().all(|s| seg_ok(s)) {
        return Err(TipError::Malformed);
    }
    if let Some(q) = query {
        let ok = !q.is_empty()
            && q.len() <= 60
            && q.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '=' | '&' | '.' | '-' | '_'));
        if !ok {
            return Err(TipError::Malformed);
        }
    }
    Ok(())
}

/// Check one link of a named kind.
pub fn validate_tip(kind: &str, value: &str) -> Result<TipKind, TipError> {
    let kind = TipKind::parse(kind).ok_or(TipError::UnknownKind)?;
    let max = if kind == TipKind::Lightning {
        MAX_LIGHTNING_URL
    } else {
        MAX_TIP_URL
    };
    if value.len() > max {
        return Err(TipError::TooLong);
    }
    match kind {
        TipKind::Btc => bitcoin_ok(value).then_some(kind).ok_or(TipError::Malformed),
        TipKind::Lightning => lightning_ok(value)
            .then_some(kind)
            .ok_or(TipError::Malformed),
        TipKind::Kofi | TipKind::Patreon => web_ok(kind, value).map(|()| kind),
    }
}

/// One tip link as a manifest or profile carries it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TipLink {
    pub kind: TipKind,
    pub url: String,
}

/// Check a list: at most [`MAX_TIP_LINKS`], one per kind, each valid.
pub fn validate_tips(links: &[TipLink]) -> Result<(), TipError> {
    if links.len() > MAX_TIP_LINKS {
        return Err(TipError::TooMany);
    }
    for (i, l) in links.iter().enumerate() {
        validate_tip(l.kind.as_str(), &l.url)?;
        if links[..i].iter().any(|o| o.kind == l.kind) {
            return Err(TipError::Duplicate);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BC1: &str = "bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq";
    const LEGACY: &str = "bitcoin:1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2";

    #[test]
    fn web_tips_need_an_allow_listed_host() {
        for (k, u) in [
            ("kofi", "https://ko-fi.com/someone"),
            ("kofi", "https://www.ko-fi.com/someone"),
            ("patreon", "https://www.patreon.com/someone"),
            ("patreon", "https://patreon.com/c/someone"),
            ("patreon", "https://www.patreon.com/user?u=123456"),
        ] {
            assert!(validate_tip(k, u).is_ok(), "{k} {u}");
        }
        for (k, u, e) in [
            (
                "kofi",
                "https://evil.example/someone",
                TipError::HostNotAllowed,
            ),
            (
                "kofi",
                "https://ko-fi.com.evil.example/x",
                TipError::HostNotAllowed,
            ),
            ("kofi", "https://sub.ko-fi.com/x", TipError::HostNotAllowed),
            (
                "kofi",
                "https://www.patreon.com/x",
                TipError::HostNotAllowed,
            ),
            ("patreon", "https://ko-fi.com/x", TipError::HostNotAllowed),
            ("kofi", "http://ko-fi.com/x", TipError::Malformed),
            ("kofi", "https://user@ko-fi.com/x", TipError::Malformed),
            ("kofi", "https://ko-fi.com:8443/x", TipError::Malformed),
            ("kofi", "https://ko-fi.com/", TipError::Malformed),
            ("kofi", "https://ko-fi.com", TipError::Malformed),
            ("kofi", "https://ko-fi.com/a/../b", TipError::Malformed),
            ("kofi", "https://ko-fi.com/a b", TipError::Malformed),
            ("kofi", "https://ko-fi.com/a#frag", TipError::Malformed),
            ("kofi", "javascript:alert(1)", TipError::Malformed),
            ("paypal", "https://paypal.me/x", TipError::UnknownKind),
        ] {
            assert_eq!(validate_tip(k, u), Err(e), "{k} {u}");
        }
    }

    #[test]
    fn bitcoin_and_lightning_uris() {
        assert!(validate_tip("btc", BC1).is_ok());
        assert!(validate_tip("btc", LEGACY).is_ok());
        assert!(validate_tip("btc", &format!("{BC1}?amount=0.001&label=Thanks")).is_ok());
        for bad in [
            "bitcoin:",
            "bitcoin:notanaddress",
            "bitcoin:bc1QAR0SRRR7XFKVY5L643LYDNW9RE59GTZZWF5MDQ",
            "BITCOIN:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq",
            "https://example.com",
            "bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq?req-x=1",
            "bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq?amount=1&amount=2",
            "bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq?amount=abc",
            "bitcoin:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq?label=a b",
        ] {
            assert_eq!(validate_tip("btc", bad), Err(TipError::Malformed), "{bad}");
        }
        let inv = format!("lightning:lnbc1pvjluezsp5{}", "q".repeat(60));
        assert!(validate_tip("lightning", &inv).is_ok());
        let lnurl = format!("lightning:lnurl1dp68gurn8ghj7{}", "x".repeat(20));
        assert!(validate_tip("lightning", &lnurl).is_ok());
        for bad in [
            "lightning:",
            "lightning:lnbc",
            "lightning:LNBC1PVJLUEZSP5QQQQQQQQQQQQQQQQQQQQQQQQQQQQ",
            "lightning:user@example.com",
            "lightning:lnbc1pvjluez?x=1",
            "https://ko-fi.com/x",
        ] {
            assert_eq!(
                validate_tip("lightning", bad),
                Err(TipError::Malformed),
                "{bad}"
            );
        }
        let long = format!("lightning:lnbc{}", "q".repeat(MAX_LIGHTNING_URL));
        assert_eq!(validate_tip("lightning", &long), Err(TipError::TooLong));
    }

    #[test]
    fn a_list_has_at_most_four_one_of_each() {
        let l = |kind, url: &str| TipLink {
            kind,
            url: url.to_owned(),
        };
        let ok = vec![
            l(TipKind::Btc, BC1),
            l(TipKind::Kofi, "https://ko-fi.com/a"),
        ];
        assert_eq!(validate_tips(&ok), Ok(()));
        let dup = vec![
            l(TipKind::Kofi, "https://ko-fi.com/a"),
            l(TipKind::Kofi, "https://ko-fi.com/b"),
        ];
        assert_eq!(validate_tips(&dup), Err(TipError::Duplicate));
        let wrong_host = vec![l(TipKind::Patreon, "https://ko-fi.com/a")];
        assert_eq!(validate_tips(&wrong_host), Err(TipError::HostNotAllowed));
        assert_eq!(validate_tips(&[]), Ok(()));
        // More than four is refused before anything else.
        let five: Vec<TipLink> = (0..5).map(|_| l(TipKind::Btc, BC1)).collect();
        assert_eq!(validate_tips(&five), Err(TipError::TooMany));
    }
}
