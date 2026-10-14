//! coturn REST-API credentials (ADR-0004, `use-auth-secret`): the username is
//! `<expiry unix seconds>:<user id>` and the credential is
//! `base64(HMAC-SHA1(shared secret, username))`. Nothing is stored; coturn
//! recomputes the MAC and rejects names whose timestamp has passed.

use data_encoding::BASE64;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sr_core::UserId;

/// Credential lifetime for `GET /sr/voice/turn` (spec section 6).
pub const DEFAULT_TTL_SECS: u64 = 3600;

/// `[voice.turn]`-shaped settings, independent of the config crate.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TurnConfig {
    /// `stun:` / `turn:` / `turns:` URLs handed to clients, e.g. `turn:turn.example.org:3478?transport=udp`.
    pub urls: Vec<String>,
    /// coturn `static-auth-secret`. Empty disables TURN credentials.
    pub secret: String,
    pub ttl_secs: u64,
}

/// Body of `GET /sr/voice/turn`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnCredentials {
    pub username: String,
    pub password: String,
    pub ttl: u64,
    pub uris: Vec<String>,
}

/// WebRTC `RTCIceServer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

fn is_stun(url: &str) -> bool {
    url.starts_with("stun:") || url.starts_with("stuns:")
}

impl TurnConfig {
    /// Credentials valid for `ttl_secs` from `now_secs`. `None` when TURN is not configured.
    pub fn credentials(&self, user: UserId, now_secs: u64) -> Option<TurnCredentials> {
        let uris: Vec<String> = self.urls.iter().filter(|u| !is_stun(u)).cloned().collect();
        if self.secret.is_empty() || uris.is_empty() {
            return None;
        }
        let ttl = if self.ttl_secs == 0 {
            DEFAULT_TTL_SECS
        } else {
            self.ttl_secs
        };
        let username = format!("{}:{}", now_secs + ttl, user);
        let password = password_for(&self.secret, &username)?;
        Some(TurnCredentials {
            username,
            password,
            ttl,
            uris,
        })
    }

    /// The `ice_servers` list placed in `VOICE_SERVER_UPDATE`: plain STUN URLs without
    /// credentials, TURN URLs with the ephemeral ones.
    pub fn ice_servers(&self, user: UserId, now_secs: u64) -> Vec<IceServer> {
        let stun: Vec<String> = self.urls.iter().filter(|u| is_stun(u)).cloned().collect();
        let mut out = Vec::new();
        if !stun.is_empty() {
            out.push(IceServer {
                urls: stun,
                username: None,
                credential: None,
            });
        }
        if let Some(c) = self.credentials(user, now_secs) {
            out.push(IceServer {
                urls: c.uris,
                username: Some(c.username),
                credential: Some(c.password),
            });
        }
        out
    }
}

fn password_for(secret: &str, username: &str) -> Option<String> {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(username.as_bytes());
    Some(BASE64.encode(&mac.finalize().into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> TurnConfig {
        TurnConfig {
            urls: vec![
                "stun:turn.example.org:3478".into(),
                "turn:turn.example.org:3478?transport=udp".into(),
            ],
            secret: "shared".into(),
            ttl_secs: 3600,
        }
    }

    #[test]
    fn credentials_match_the_coturn_scheme() {
        let c = cfg().credentials(UserId::new(42), 1000).unwrap();
        assert_eq!(c.username, "4600:42");
        assert_eq!(c.ttl, 3600);
        assert_eq!(c.uris, vec!["turn:turn.example.org:3478?transport=udp"]);
        // Independently recomputed with the documented formula.
        assert_eq!(Some(c.password), password_for("shared", "4600:42"));
        // HMAC-SHA1 is 20 bytes: 28 base64 characters.
        assert_eq!(
            cfg().credentials(UserId::new(1), 0).unwrap().password.len(),
            28
        );
    }

    #[test]
    fn known_vector() {
        // Computed independently: HMAC-SHA1("shared", "4600:42"), base64.
        let c = cfg().credentials(UserId::new(42), 1000).unwrap();
        assert_eq!(c.password, "DpMnq29/Atv00tPQz8mVCWtXTdg=");
    }

    #[test]
    fn unconfigured_turn_yields_nothing() {
        assert!(
            TurnConfig::default()
                .credentials(UserId::new(1), 0)
                .is_none()
        );
        let mut c = cfg();
        c.secret.clear();
        assert!(c.credentials(UserId::new(1), 0).is_none());
        // STUN-only still produces an ICE server entry.
        assert_eq!(c.ice_servers(UserId::new(1), 0).len(), 1);
    }

    #[test]
    fn ice_servers_split_stun_and_turn() {
        let s = cfg().ice_servers(UserId::new(42), 1000);
        assert_eq!(s.len(), 2);
        assert!(s[0].credential.is_none());
        assert_eq!(s[1].username.as_deref(), Some("4600:42"));
    }
}
