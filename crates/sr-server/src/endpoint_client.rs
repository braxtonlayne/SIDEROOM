//! The client behind HTTP interactions endpoints (DEV-BOT-005): applications' own servers are
//! reached through the SSRF-safe `sr-net` client with the strict public-address policy, one
//! pinned connection per request, no redirects.

use async_trait::async_trait;
use sr_api::routes::developer::{EndpointClient, EndpointReply, MAX_ENDPOINT_REPLY};
use sr_net::{NetError, Policy, SafeClient};
use std::time::Duration;
use url::Url;

/// [`EndpointClient`] over [`SafeClient`].
pub struct SafeEndpointClient(SafeClient);

impl SafeEndpointClient {
    /// The production client: public addresses only, system DNS, rustls.
    pub fn production() -> Self {
        Self(SafeClient::production(
            Policy::default(),
            "SideRoom-Interactions/1.0",
        ))
    }

    /// Wrap an existing client (tests).
    pub fn new(c: SafeClient) -> Self {
        Self(c)
    }
}

/// A reason for the owner; it never contains the URL.
fn reason(e: &NetError) -> String {
    match e {
        NetError::Blocked(_) => "the address is not a public host".to_owned(),
        NetError::Timeout => "the request timed out".to_owned(),
        NetError::Resolve(_) => "the host name did not resolve".to_owned(),
        NetError::TooLarge => "the reply was too large".to_owned(),
        NetError::BadUrl(_) => "the URL is not valid".to_owned(),
        _ => "the connection failed".to_owned(),
    }
}

#[async_trait]
impl EndpointClient for SafeEndpointClient {
    async fn post(
        &self,
        url: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        timeout: Duration,
    ) -> Result<EndpointReply, String> {
        let url = Url::parse(url).map_err(|_| "the URL is not valid".to_owned())?;
        let r = self
            .0
            .post_json(&url, headers, body, MAX_ENDPOINT_REPLY, timeout)
            .await
            .map_err(|e| reason(&e))?;
        Ok(EndpointReply {
            status: r.status,
            body: r.body,
        })
    }
}
