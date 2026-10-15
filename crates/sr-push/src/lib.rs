//! sr-push: Web Push (LNG-NTF-003). VAPID application-server keys (RFC 8292), message
//! encryption (RFC 8291 / RFC 8188 `aes128gcm`) and delivery to the browser vendors' push
//! services. Pure Rust on RustCrypto and rustls; the subscription endpoint is
//! user-supplied, so every request goes through `sr-net`'s SSRF policy.

pub mod ece;
pub mod sender;
pub mod vapid;

pub use sender::{
    Notification, PushSender, SendOutcome, Subscription, Urgency, WebPushClient, parse_endpoint,
    topic_for, validate_subscription,
};
pub use vapid::VapidKey;

/// Why a push could not be built or sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PushError {
    /// The subscription's endpoint or keys are unusable.
    #[error("bad push subscription: {0}")]
    BadSubscription(&'static str),
    /// The payload does not fit one 4096-byte record.
    #[error("push payload is too large")]
    PayloadTooLarge,
    /// A cryptographic step failed.
    #[error("push crypto error: {0}")]
    Crypto(String),
    /// The request could not be made (policy refusal, DNS, connection).
    #[error("push network error: {0}")]
    Network(String),
}
