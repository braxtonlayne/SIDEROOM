//! Connecting to a member's endpoint without trusting DNS twice (BYOS-BRW-001, the rebind gap of wave 27).
//!
//! An endpoint is checked against the SSRF policy when a mount is saved, but a name can answer differently
//! later (DNS rebinding). The clients this crate signs itself ([`crate::s3admin::S3Admin`],
//! [`crate::s3multipart::S3Multipart`]) therefore connect to addresses that were resolved and checked
//! **once, just before the call**: an [`EndpointGuard`] returns them and the client pins the host name to
//! exactly those addresses (`reqwest`'s `resolve_to_addrs`), so the connection cannot be redirected by a
//! second lookup. TLS still validates the host name, so a pinned address does not weaken it.
//!
//! The one client that cannot be pinned is `object_store`'s (listing, metadata, header reads): it builds its
//! own `reqwest` and has no resolver hook. Those paths re-check the endpoint through the guard immediately
//! before the call, which narrows the window to the milliseconds between the check and the connect.

use std::net::{IpAddr, SocketAddr};

use async_trait::async_trait;

/// Resolves and vets the host of an endpoint URL.
#[async_trait]
pub trait EndpointGuard: Send + Sync {
    /// Resolve the host of `url` once, check every address against the policy, and return the addresses to
    /// connect to. `Err` carries a reason that is safe to log, never to show.
    async fn validated_addrs(&self, url: &str) -> Result<Vec<SocketAddr>, String>;
}

/// The host name of a `scheme://host[:port]` base URL when it is a name (an IP literal needs no pinning).
pub(crate) fn pin_host(base_url: &str) -> Option<String> {
    let rest = base_url.split_once("://").map_or(base_url, |(_, r)| r);
    let authority = rest.split('/').next().unwrap_or(rest);
    let host = match authority.rsplit_once(':') {
        Some((h, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => h,
        _ => authority,
    };
    let host = host.trim_matches(|c| c == '[' || c == ']');
    if host.is_empty() || host.parse::<IpAddr>().is_ok() {
        return None;
    }
    Some(host.to_owned())
}

#[cfg(test)]
mod tests {
    use super::pin_host;

    #[test]
    fn only_names_are_pinned() {
        assert_eq!(
            pin_host("https://s3.example.test").as_deref(),
            Some("s3.example.test")
        );
        assert_eq!(
            pin_host("http://shelf.s3.example.test:3900/x").as_deref(),
            Some("shelf.s3.example.test")
        );
        assert_eq!(pin_host("http://127.0.0.1:3900"), None);
        assert_eq!(pin_host("http://[::1]:3900"), None);
        assert_eq!(pin_host(""), None);
    }
}
