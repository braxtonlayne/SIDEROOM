//! sr-unfurl: SSRF-safe link previews (LNG-MSG-014).
//!
//! * [`policy`]: which URLs and addresses may be fetched (table-tested).
//! * [`embed`]: the privacy-enhanced embed and direct-media offers ("Play here", PRF-UNF-001).
//! * [`fetch`]: resolve, validate, pin and follow at most three redirects; the DNS
//!   resolver and the single-hop HTTP exchange are injectable.
//! * [`transport`]: the reqwest/rustls transport (no OpenSSL, no proxies).
//! * [`parse`]: OpenGraph, Twitter card and oEmbed readers over a pure Rust HTML parser.
//! * [`unfurl`]: building embeds, the 24 hour cache and the per-host rate limit.

pub mod embed;
pub mod fetch;
pub mod parse;
pub use sr_net::policy;
pub mod transport;
pub mod unfurl;

pub use fetch::{FetchError, Resolver, SafeFetcher, SystemResolver};
pub use policy::{Blocked, Policy, is_forbidden_ip};
pub use unfurl::{UnfurlError, UnfurledEmbed, Unfurler};

use std::sync::Arc;

/// A fetcher wired to the system resolver and the reqwest transport, for the image
/// proxy (the unfurler's own fetcher is private).
pub fn production_fetcher(policy: Policy) -> SafeFetcher {
    SafeFetcher::new(
        policy,
        Arc::new(SystemResolver),
        Arc::new(transport::ReqwestTransport),
    )
}

/// An unfurler wired to the system resolver and the reqwest transport.
pub fn production(policy: Policy, per_host_per_minute: u32) -> Unfurler {
    Unfurler::new(
        SafeFetcher::new(
            policy,
            Arc::new(SystemResolver),
            Arc::new(transport::ReqwestTransport),
        ),
        per_host_per_minute,
    )
}
