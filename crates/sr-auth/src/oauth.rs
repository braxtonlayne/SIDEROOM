//! Opaque credentials of the developer platform and the pure OAuth2 rules
//! (docs/specs/developer-platform.md section 1 and 3.3).
//!
//! Every credential here is 256 bits from the OS RNG, shown once, and stored only as a
//! SHA-256 digest under its own domain label, so a value minted for one purpose can never
//! match a digest stored for another. Digests are compared in constant time.

use crate::{error::AuthError, random, session::TokenHash};
use data_encoding::BASE64URL_NOPAD;
use sha2::{Digest, Sha256};
use std::fmt;
use subtle::ConstantTimeEq;

/// What an opaque token is for. The label is part of the digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpaqueKind {
    /// Application client secret.
    ClientSecret,
    /// OAuth2 authorization code.
    AuthCode,
    /// OAuth2 access token.
    AccessToken,
    /// OAuth2 refresh token.
    RefreshToken,
    /// Interaction token.
    InteractionToken,
}

impl OpaqueKind {
    fn domain(self) -> &'static str {
        match self {
            Self::ClientSecret => "sideroom/client-secret/v1",
            Self::AuthCode => "sideroom/oauth-code/v1",
            Self::AccessToken => "sideroom/oauth-access/v1",
            Self::RefreshToken => "sideroom/oauth-refresh/v1",
            Self::InteractionToken => "sideroom/interaction-token/v1",
        }
    }
}

/// Length of every opaque token in characters (32 bytes, base64url without padding).
pub const OPAQUE_LEN: usize = 43;

/// A freshly minted opaque credential. Shown once; store [`OpaqueToken::hash`] only.
pub struct OpaqueToken {
    kind: OpaqueKind,
    value: String,
}

impl OpaqueToken {
    /// Mint a token of `kind`.
    pub fn generate(kind: OpaqueKind) -> Result<Self, AuthError> {
        Ok(Self {
            kind,
            value: BASE64URL_NOPAD.encode(&random::bytes::<32>()?),
        })
    }

    /// The raw value, for the one-time display.
    pub fn expose(&self) -> &str {
        &self.value
    }

    /// The at-rest digest.
    pub fn hash(&self) -> TokenHash {
        TokenHash::digest(self.kind.domain(), &self.value)
    }

    /// Last four characters, safe to show as a hint ("...abcd").
    pub fn hint(&self) -> String {
        self.value
            .chars()
            .skip(self.value.chars().count().saturating_sub(4))
            .collect()
    }
}

impl fmt::Debug for OpaqueToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OpaqueToken({:?}, ..)", self.kind)
    }
}

/// Digest of a presented token, or `None` when its shape cannot be one of ours (so junk is
/// refused before it reaches the database).
pub fn hash_presented(kind: OpaqueKind, presented: &str) -> Option<TokenHash> {
    if presented.len() != OPAQUE_LEN
        || !BASE64URL_NOPAD
            .decode(presented.as_bytes())
            .is_ok_and(|b| b.len() == 32)
    {
        return None;
    }
    Some(TokenHash::digest(kind.domain(), presented))
}

/// Digest of a bot token presented in `Authorization: Bot` or IDENTIFY.
pub fn hash_bot_token(presented: &str) -> Option<(crate::tokens::BotTokenParts, TokenHash)> {
    let parts = crate::tokens::BotToken::parse_parts(presented)?;
    Some((parts, crate::tokens::BotToken::hash_of(presented)))
}

/// Constant-time equality of two digests (a stored one against a recomputed one).
pub fn digests_equal(a: &TokenHash, b: &TokenHash) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

// ------------------------------------------------------------------ PKCE

/// PKCE challenge methods. Only `S256` is accepted (`plain` defeats the purpose).
pub fn pkce_method_ok(method: Option<&str>) -> bool {
    matches!(method, None | Some("S256"))
}

fn unreserved(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~')
}

/// RFC 7636 verifier shape: 43 to 128 unreserved characters.
pub fn pkce_verifier_ok(verifier: &str) -> bool {
    (43..=128).contains(&verifier.len()) && verifier.chars().all(unreserved)
}

/// A challenge is the unpadded base64url of a SHA-256 digest: exactly 43 characters.
pub fn pkce_challenge_ok(challenge: &str) -> bool {
    challenge.len() == 43
        && BASE64URL_NOPAD
            .decode(challenge.as_bytes())
            .is_ok_and(|b| b.len() == 32)
}

/// `BASE64URL(SHA256(verifier)) == challenge`, in constant time.
pub fn pkce_verify(verifier: &str, challenge: &str) -> bool {
    if !pkce_verifier_ok(verifier) || !pkce_challenge_ok(challenge) {
        return false;
    }
    let computed = BASE64URL_NOPAD.encode(&Sha256::digest(verifier.as_bytes()));
    computed.as_bytes().ct_eq(challenge.as_bytes()).into()
}

// ------------------------------------------------------------ redirect URIs

/// Why a redirect URI was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectError {
    /// Not an absolute URL, too long, or has a fragment or userinfo.
    Malformed,
    /// Neither `https` nor loopback `http`.
    Scheme,
}

impl RedirectError {
    /// Human-readable reason.
    pub fn message(self) -> &'static str {
        match self {
            Self::Malformed => {
                "Must be an absolute URL of at most 512 characters without a fragment or credentials."
            }
            Self::Scheme => {
                "Must use https (http is allowed only for localhost, 127.0.0.1 and [::1])."
            }
        }
    }
}

/// Registered redirect URIs: absolute, `https` (or `http` on loopback), no fragment, no
/// userinfo, at most 512 characters. Comparison later is exact string equality.
pub fn validate_redirect_uri(uri: &str) -> Result<(), RedirectError> {
    if uri.is_empty() || uri.len() > 512 || uri.chars().any(|c| c.is_control() || c == ' ') {
        return Err(RedirectError::Malformed);
    }
    let (scheme, rest) = uri.split_once("://").ok_or(RedirectError::Malformed)?;
    if uri.contains('#') || rest.is_empty() {
        return Err(RedirectError::Malformed);
    }
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || authority.contains('\\') {
        return Err(RedirectError::Malformed);
    }
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        stripped.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    if host.is_empty() {
        return Err(RedirectError::Malformed);
    }
    let loopback = matches!(host, "localhost" | "127.0.0.1" | "::1");
    match scheme {
        "https" => Ok(()),
        "http" if loopback => Ok(()),
        _ => Err(RedirectError::Scheme),
    }
}

/// Exact match of a presented URI against the registered list.
pub fn redirect_registered(registered: &[String], presented: &str) -> bool {
    registered.iter().any(|r| r == presented)
}

// -------------------------------------------------------------------- scopes

/// An OAuth2 scope this platform issues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Scope {
    /// `identify`: the user's id, name and avatar.
    Identify,
    /// `email`: the user's address (needs `identify`).
    Email,
    /// `guilds`: the servers the user is in.
    Guilds,
    /// `bot`: add the application's bot to a server.
    Bot,
    /// `applications.commands`: the application's commands in a server.
    ApplicationsCommands,
    /// `applications.commands.permissions.update`: edit command permissions.
    CommandsPermissionsUpdate,
}

impl Scope {
    /// All scopes.
    pub const ALL: &'static [Scope] = &[
        Scope::Identify,
        Scope::Email,
        Scope::Guilds,
        Scope::Bot,
        Scope::ApplicationsCommands,
        Scope::CommandsPermissionsUpdate,
    ];

    /// Wire name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Identify => "identify",
            Self::Email => "email",
            Self::Guilds => "guilds",
            Self::Bot => "bot",
            Self::ApplicationsCommands => "applications.commands",
            Self::CommandsPermissionsUpdate => "applications.commands.permissions.update",
        }
    }

    /// Consent-screen sentence.
    pub const fn description(self) -> &'static str {
        match self {
            Self::Identify => "Access your username, avatar and banner",
            Self::Email => "Access your email address",
            Self::Guilds => "Know which servers you are in",
            Self::Bot => "Add a bot to a server",
            Self::ApplicationsCommands => "Create commands in a server",
            Self::CommandsPermissionsUpdate => {
                "Update permissions for this application's commands in a server"
            }
        }
    }

    /// Parse one scope name.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|x| x.name() == s)
    }
}

/// Parse a space-separated scope string into a sorted, de-duplicated set. `Err` holds the
/// first unknown name. An empty string is `Ok(vec![])`.
pub fn parse_scopes(raw: &str) -> Result<Vec<Scope>, String> {
    let mut out = Vec::new();
    for part in raw.split([' ', '+']).filter(|p| !p.is_empty()) {
        let s = Scope::parse(part).ok_or_else(|| part.to_owned())?;
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out.sort();
    Ok(out)
}

/// Names of a scope set, for storage and `scope` responses.
pub fn scope_names(scopes: &[Scope]) -> Vec<String> {
    scopes.iter().map(|s| s.name().to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_distinct_and_domain_separated() {
        let a = OpaqueToken::generate(OpaqueKind::AccessToken).unwrap();
        let b = OpaqueToken::generate(OpaqueKind::AccessToken).unwrap();
        assert_ne!(a.expose(), b.expose());
        assert_eq!(a.expose().len(), OPAQUE_LEN);
        // The same string hashed for another purpose never matches.
        let as_refresh = hash_presented(OpaqueKind::RefreshToken, a.expose()).unwrap();
        assert!(!digests_equal(&a.hash(), &as_refresh));
        let again = hash_presented(OpaqueKind::AccessToken, a.expose()).unwrap();
        assert!(digests_equal(&a.hash(), &again));
        assert_eq!(format!("{a:?}"), "OpaqueToken(AccessToken, ..)");
        assert_eq!(a.hint().len(), 4);
    }

    #[test]
    fn junk_is_refused_before_hashing() {
        assert!(hash_presented(OpaqueKind::AccessToken, "short").is_none());
        assert!(hash_presented(OpaqueKind::AccessToken, &"!".repeat(43)).is_none());
        assert!(hash_presented(OpaqueKind::AccessToken, &"A".repeat(44)).is_none());
    }

    #[test]
    fn pkce_rfc7636_vector() {
        // RFC 7636 appendix B.
        let v = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let c = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert!(pkce_verify(v, c));
        assert!(!pkce_verify(
            "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXl",
            c
        ));
        assert!(!pkce_verify("short", c));
        assert!(!pkce_verify(v, "short"));
        assert!(pkce_method_ok(Some("S256")) && pkce_method_ok(None));
        assert!(!pkce_method_ok(Some("plain")));
    }

    #[test]
    fn redirect_uri_rules() {
        for ok in [
            "https://example.com/cb",
            "https://example.com:8443/cb?x=1",
            "http://localhost:3000/cb",
            "http://127.0.0.1/cb",
            "http://[::1]:8080/cb",
        ] {
            assert_eq!(validate_redirect_uri(ok), Ok(()), "{ok}");
        }
        for bad in [
            "",
            "example.com/cb",
            "https://example.com/cb#frag",
            "https://user:pw@example.com/cb",
            "https://",
            "https://exa mple.com",
            "javascript:alert(1)",
            "myapp://callback",
        ] {
            assert!(validate_redirect_uri(bad).is_err(), "{bad}");
        }
        assert_eq!(
            validate_redirect_uri("http://example.com/cb"),
            Err(RedirectError::Scheme)
        );
        assert_eq!(
            validate_redirect_uri("http://localhost.evil.com/cb"),
            Err(RedirectError::Scheme)
        );
        assert!(redirect_registered(
            &["https://a.test/cb".into()],
            "https://a.test/cb"
        ));
        assert!(!redirect_registered(
            &["https://a.test/cb".into()],
            "https://a.test/cb/"
        ));
    }

    #[test]
    fn scopes_parse_and_reject_unknown() {
        let s = parse_scopes("identify guilds identify").unwrap();
        assert_eq!(s, vec![Scope::Identify, Scope::Guilds]);
        assert_eq!(parse_scopes("identify+email").unwrap().len(), 2);
        assert_eq!(parse_scopes("identify rpc"), Err("rpc".to_owned()));
        assert_eq!(parse_scopes("").unwrap(), vec![]);
        for sc in Scope::ALL {
            assert_eq!(Scope::parse(sc.name()), Some(*sc));
        }
    }
}
