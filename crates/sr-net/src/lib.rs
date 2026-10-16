//! sr-net: the one place outbound HTTP to operator- or user-influenced URLs is made safe
//! (XC-SEC-003).
//!
//! * [`policy`]: which URLs and addresses may be touched (pure, table-tested).
//! * [`resolve`]: the DNS resolver seam and [`resolve_validated`], which refuses a host if
//!   any answer is non-public. Callers connect only to the returned addresses (pinning), so
//!   a DNS-rebinding answer cannot change between the check and the connection.
//! * [`client`]: [`SafeClient`], a generic capped GET with revalidated redirects, and the
//!   pinned reqwest builder every transport must use.
//!
//! Rules for callers: never build a `reqwest::Client` for a URL that came from a user, a
//! remote server or a config value without going through this crate. rustls only; proxies
//! from the environment are never read; redirects are never followed by reqwest itself.

pub mod client;
pub mod policy;
pub mod resolve;

pub use client::{
    ApiCall, ApiHop, ApiMethod, CONNECT_TIMEOUT, ForwardMethod, ForwardRequest, ForwardResponse,
    HopRequest, HopResponse, HttpTransport, MAX_REDIRECTS, NetError, PostRequest, PostResponse,
    ReqwestTransport, SafeClient, SafeResponse, TOTAL_TIMEOUT, is_redirect, pinned_builder,
};
pub use policy::{Blocked, MAX_URL_LEN, Policy, is_forbidden_ip};
pub use resolve::{Resolver, SystemResolver, resolve_validated};
