//! DNS resolution under the SSRF policy.

use crate::client::{CONNECT_TIMEOUT, NetError};
use crate::policy::{Blocked, Policy};
use async_trait::async_trait;
use std::net::{IpAddr, SocketAddr};
use url::{Host, Url};

/// Resolves a host name to addresses.
#[async_trait]
pub trait Resolver: Send + Sync {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<IpAddr>, String>;
}

/// The operating system resolver.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemResolver;

#[async_trait]
impl Resolver for SystemResolver {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<IpAddr>, String> {
        let addrs = tokio::net::lookup_host((host, port))
            .await
            .map_err(|e| e.to_string())?;
        Ok(addrs.map(|a| a.ip()).collect())
    }
}

/// The validated socket addresses for `url`, or why it is refused. Every address of a
/// lookup must pass: a name that is public for some answers and private for others is
/// hostile.
pub async fn resolve_validated(
    policy: &Policy,
    resolver: &dyn Resolver,
    url: &Url,
) -> Result<Vec<SocketAddr>, NetError> {
    policy.check_url(url)?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| NetError::BadUrl("no port".into()))?;
    let ips = match url.host() {
        Some(Host::Ipv4(ip)) => vec![IpAddr::V4(ip)],
        Some(Host::Ipv6(ip)) => vec![IpAddr::V6(ip)],
        Some(Host::Domain(d)) => {
            let d = d.trim_end_matches('.');
            tokio::time::timeout(CONNECT_TIMEOUT, resolver.resolve(d, port))
                .await
                .map_err(|_| NetError::Timeout)?
                .map_err(NetError::Resolve)?
        }
        None => return Err(Blocked::NoHost.into()),
    };
    policy.check_all(&ips)?;
    Ok(ips
        .into_iter()
        .map(|ip| SocketAddr::new(ip, port))
        .collect())
}
