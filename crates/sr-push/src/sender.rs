//! Delivery: build the encrypted, VAPID-signed request for one subscription and POST it
//! to the push service over the SSRF-safe path (the endpoint URL is user supplied).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use sha2::{Digest, Sha256};
use sr_net::{Policy, Resolver, SystemResolver, pinned_builder, resolve_validated};
use url::Url;

use crate::vapid::VapidKey;
use crate::{PushError, ece};

/// A browser's push subscription, as the `PushSubscription.toJSON()` fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    pub endpoint: String,
    /// Base64url, the 65-byte uncompressed receiver key.
    pub p256dh: String,
    /// Base64url, the 16-byte authentication secret.
    pub auth: String,
}

/// How soon the push service should deliver (RFC 8030 `Urgency`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low,
    Normal,
    High,
}

impl Urgency {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
        }
    }
}

/// One message for one subscription.
#[derive(Debug, Clone)]
pub struct Notification {
    /// The (small) payload the service worker receives, usually JSON.
    pub payload: Vec<u8>,
    /// Messages with the same collapse key replace each other while undelivered
    /// (the `Topic` header; hashed, so any string works).
    pub collapse_key: Option<String>,
    pub urgency: Urgency,
    /// How long the push service may hold the message.
    pub ttl_secs: u32,
}

/// What the push service said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendOutcome {
    /// Accepted for delivery.
    Delivered,
    /// 404 or 410: the subscription is gone; delete it.
    Gone,
    /// Anything else (throttling, outage, a refused request); the message is dropped.
    Failed(u16),
}

/// Push delivery seam: production uses [`WebPushClient`], tests a fake.
#[async_trait]
pub trait PushSender: Send + Sync {
    async fn send(&self, sub: &Subscription, n: &Notification) -> Result<SendOutcome, PushError>;
}

/// The request that goes to the push service, before the network.
#[derive(Debug, Clone)]
pub struct PreparedRequest {
    pub url: Url,
    pub authorization: String,
    pub topic: Option<String>,
    pub urgency: Urgency,
    pub ttl_secs: u32,
    pub body: Vec<u8>,
}

/// The `Topic` header value for a collapse key: 22 base64url characters of its hash.
pub fn topic_for(collapse_key: &str) -> String {
    let digest = Sha256::digest(collapse_key.as_bytes());
    B64.encode(&digest[..16])
}

/// A subscription endpoint must be an https URL (push services always are).
pub fn parse_endpoint(endpoint: &str) -> Result<Url, PushError> {
    let url =
        Url::parse(endpoint).map_err(|_| PushError::BadSubscription("endpoint is not a URL"))?;
    if url.scheme() != "https" {
        return Err(PushError::BadSubscription("endpoint must be https"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(PushError::BadSubscription("endpoint has credentials"));
    }
    Ok(url)
}

/// Validate the key material of a subscription without sending anything.
pub fn validate_subscription(sub: &Subscription) -> Result<(), PushError> {
    parse_endpoint(&sub.endpoint)?;
    let key = B64
        .decode(sub.p256dh.trim_end_matches('='))
        .map_err(|_| PushError::BadSubscription("p256dh is not base64url"))?;
    p256::PublicKey::from_sec1_bytes(&key)
        .map_err(|_| PushError::BadSubscription("p256dh is not a P-256 point"))?;
    let auth = B64
        .decode(sub.auth.trim_end_matches('='))
        .map_err(|_| PushError::BadSubscription("auth is not base64url"))?;
    if auth.len() != 16 {
        return Err(PushError::BadSubscription("auth secret must be 16 bytes"));
    }
    Ok(())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Encrypt and sign. `now_unix` is injected for tests.
pub fn prepare(
    vapid: &VapidKey,
    subject: &str,
    sub: &Subscription,
    n: &Notification,
    now_unix: u64,
) -> Result<PreparedRequest, PushError> {
    let url = parse_endpoint(&sub.endpoint)?;
    let ua = B64
        .decode(sub.p256dh.trim_end_matches('='))
        .map_err(|_| PushError::BadSubscription("p256dh is not base64url"))?;
    let auth = B64
        .decode(sub.auth.trim_end_matches('='))
        .map_err(|_| PushError::BadSubscription("auth is not base64url"))?;
    let body = ece::encrypt(&ua, &auth, &n.payload)?;
    let audience = url.origin().ascii_serialization();
    Ok(PreparedRequest {
        authorization: vapid.authorization(&audience, subject, now_unix + 12 * 3600),
        topic: n.collapse_key.as_deref().map(topic_for),
        urgency: n.urgency,
        ttl_secs: n.ttl_secs,
        url,
        body,
    })
}

/// Production [`PushSender`]: resolves the endpoint under the SSRF policy, pins the
/// connection to the validated addresses and POSTs the encrypted message.
#[derive(Clone)]
pub struct WebPushClient {
    vapid: VapidKey,
    subject: String,
    policy: Policy,
    resolver: Arc<dyn Resolver>,
}

impl WebPushClient {
    pub fn new(vapid: VapidKey, subject: impl Into<String>, policy: Policy) -> Self {
        Self {
            vapid,
            subject: subject.into(),
            policy,
            resolver: Arc::new(SystemResolver),
        }
    }

    pub fn public_key(&self) -> String {
        self.vapid.public_key()
    }
}

#[async_trait]
impl PushSender for WebPushClient {
    async fn send(&self, sub: &Subscription, n: &Notification) -> Result<SendOutcome, PushError> {
        let req = prepare(&self.vapid, &self.subject, sub, n, unix_now())?;
        let addrs = resolve_validated(&self.policy, &*self.resolver, &req.url)
            .await
            .map_err(|e| PushError::Network(e.to_string()))?;
        let client = pinned_builder("SideRoom-WebPush", &req.url, &addrs)
            .build()
            .map_err(|e| PushError::Network(e.to_string()))?;
        let mut post = client
            .post(req.url.clone())
            .header("Authorization", req.authorization)
            .header("Content-Encoding", "aes128gcm")
            .header("Content-Type", "application/octet-stream")
            .header("TTL", req.ttl_secs.to_string())
            .header("Urgency", req.urgency.as_str());
        if let Some(t) = req.topic {
            post = post.header("Topic", t);
        }
        let resp = post
            .body(req.body)
            .send()
            .await
            .map_err(|e| PushError::Network(e.without_url().to_string()))?;
        Ok(match resp.status().as_u16() {
            200..=299 => SendOutcome::Delivered,
            404 | 410 => SendOutcome::Gone,
            other => SendOutcome::Failed(other),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::elliptic_curve::sec1::ToEncodedPoint;

    fn subscription() -> (Subscription, p256::SecretKey, Vec<u8>) {
        let secret = ece::random_secret().unwrap();
        let auth = ece::random::<16>().unwrap().to_vec();
        let sub = Subscription {
            endpoint: "https://push.example.test/send/abc".into(),
            p256dh: B64.encode(secret.public_key().to_encoded_point(false).as_bytes()),
            auth: B64.encode(&auth),
        };
        (sub, secret, auth)
    }

    #[test]
    fn prepared_requests_decrypt_and_carry_vapid_and_topic() {
        let vapid = VapidKey::generate().unwrap();
        let (sub, secret, auth) = subscription();
        let n = Notification {
            payload: br#"{"title":"hi"}"#.to_vec(),
            collapse_key: Some("channel:42".into()),
            urgency: Urgency::High,
            ttl_secs: 600,
        };
        let r = prepare(&vapid, "mailto:ops@example.test", &sub, &n, 1_000).unwrap();
        assert_eq!(ece::decrypt(&secret, &auth, &r.body).unwrap(), n.payload);
        assert!(r.authorization.starts_with("vapid t="));
        assert!(
            r.authorization
                .ends_with(&format!(", k={}", vapid.public_key()))
        );
        assert_eq!(r.topic.as_deref(), Some(topic_for("channel:42").as_str()));
        assert_eq!(topic_for("channel:42").len(), 22);
        assert_ne!(topic_for("channel:42"), topic_for("channel:43"));
        assert_eq!(r.url.as_str(), "https://push.example.test/send/abc");
    }

    #[test]
    fn subscriptions_are_validated() {
        let (good, _, _) = subscription();
        assert!(validate_subscription(&good).is_ok());
        let mut http = good.clone();
        http.endpoint = "http://push.example.test/x".into();
        assert!(validate_subscription(&http).is_err());
        let mut creds = good.clone();
        creds.endpoint = "https://u:p@push.example.test/x".into();
        assert!(validate_subscription(&creds).is_err());
        let mut key = good.clone();
        key.p256dh = B64.encode([1_u8; 65]);
        assert!(validate_subscription(&key).is_err());
        let mut auth = good;
        auth.auth = B64.encode([1_u8; 4]);
        assert!(validate_subscription(&auth).is_err());
    }

    #[tokio::test]
    async fn private_endpoints_are_refused_before_any_connection() {
        let client = WebPushClient::new(
            VapidKey::generate().unwrap(),
            "mailto:ops@example.test",
            Policy::default(),
        );
        let (mut sub, _, _) = subscription();
        sub.endpoint = "https://127.0.0.1:9/x".into();
        let n = Notification {
            payload: b"x".to_vec(),
            collapse_key: None,
            urgency: Urgency::Normal,
            ttl_secs: 60,
        };
        assert!(matches!(
            client.send(&sub, &n).await,
            Err(PushError::Network(_))
        ));
    }
}
