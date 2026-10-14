//! Baseline security response headers for the API listener (XC-SEC-001).
//!
//! The single-page app's own document headers (CSP with fonts, `/cdn` images,
//! the voice WebSocket and WASM) are set by the reverse proxy (see
//! `deploy/compose/caddy/Caddyfile.prod`) and by the Vite dev/preview servers;
//! the same policy lives in `apps/web/src/security/csp.ts`. This layer covers
//! everything `sr-server` itself answers: JSON API, gateway upgrade, health
//! endpoints. Handlers that set a header themselves (the `/cdn` routes send a
//! sandboxing CSP) are never overridden.

use axum::{
    extract::{Request, State},
    http::{HeaderName, HeaderValue, header},
    middleware::Next,
    response::Response,
};

/// Policy for responses that are data, not documents: nothing may load, and
/// nobody may frame them.
const API_CSP: &str =
    "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";
/// One year, subdomains included. Only sent when the public URL is https.
const HSTS: &str = "max-age=31536000; includeSubDomains";
/// Features the app never uses on the API origin; `camera`/`microphone`/
/// `display-capture` are allowed for the page itself via the proxy policy.
const PERMISSIONS: &str = "accelerometer=(), geolocation=(), gyroscope=(), magnetometer=(), payment=(), usb=(), interest-cohort=()";

/// Settings for [`security_headers`].
#[derive(Debug, Clone, Copy)]
pub struct SecurityHeaderConfig {
    /// Send `Strict-Transport-Security` (the public URL is https).
    pub hsts: bool,
}

/// Adds the baseline headers to every response that does not already carry them.
pub async fn security_headers(
    State(cfg): State<SecurityHeaderConfig>,
    req: Request,
    next: Next,
) -> Response {
    // The API reference page is a script-bearing document; give it no CSP of ours.
    let is_docs = req.uri().path().starts_with("/developers/");
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    let mut set = |name: HeaderName, value: &'static str| {
        if !h.contains_key(&name) {
            h.insert(name, HeaderValue::from_static(value));
        }
    };
    set(header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    set(header::REFERRER_POLICY, "no-referrer");
    set(HeaderName::from_static("permissions-policy"), PERMISSIONS);
    set(
        HeaderName::from_static("cross-origin-opener-policy"),
        "same-origin",
    );
    set(
        HeaderName::from_static("cross-origin-resource-policy"),
        "same-site",
    );
    if !is_docs {
        set(header::CONTENT_SECURITY_POLICY, API_CSP);
    }
    if cfg.hsts {
        set(header::STRICT_TRANSPORT_SECURITY, HSTS);
    }
    res
}
