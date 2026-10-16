//! The Activity proxy (DEV-ACT-001): `https://<application-id>.activities.<host>/...` serves an approved
//! Activity from its own origin, so it can never touch the app shell's cookies or storage.
//!
//! * Only when `[activities] enabled`; any other host name under `.activities.<host>` is a plain 404.
//! * `GET` and `HEAD` only. `/` is the entry URL and must hash to the SHA-256 staff approved; a path under a
//!   declared URL-mapping prefix is fetched from the mapping's target; anything else from the entry URL's
//!   origin. Fetches go through the SSRF-safe client; the application's cookies and headers never reach the
//!   browser (only the content type is relayed).
//! * Every response carries a strict `Content-Security-Policy` (`frame-ancestors` is the app shell only),
//!   `nosniff`, no referrer and a `Permissions-Policy` with the powerful features off.

use super::{ActivitiesDeps, MAX_PROXIED_BYTES};
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
};
use data_encoding::HEXLOWER;
use sha2::{Digest, Sha256};
use sr_activities::{model::ConfigView, rules};
use std::sync::Arc;

/// Put the proxy in front of `app`: requests whose `Host` is under `.activities.<host>` are answered here,
/// everything else goes on to the app.
pub fn wrap_activity_host(app: Router, deps: Arc<ActivitiesDeps>) -> Router {
    app.layer(from_fn_with_state(deps, activity_host_layer))
}

/// The middleware behind [`wrap_activity_host`].
pub async fn activity_host_layer(
    State(d): State<Arc<ActivitiesDeps>>,
    req: Request,
    next: Next,
) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_default();
    if d.base_host.is_empty() || !rules::is_activity_host(&host, &d.base_host) {
        return next.run(req).await;
    }
    let Some(app) = d
        .enabled
        .then(|| rules::app_of_host(&host, &d.base_host))
        .flatten()
    else {
        return page(&d, StatusCode::NOT_FOUND, "There is no activity here.");
    };
    if !matches!(*req.method(), Method::GET | Method::HEAD) {
        let mut r = page(
            &d,
            StatusCode::METHOD_NOT_ALLOWED,
            "Activities are read-only.",
        );
        r.headers_mut()
            .insert(header::ALLOW, HeaderValue::from_static("GET, HEAD"));
        return r;
    }
    let cfg = match d.service.approved(app).await {
        Ok(Some(c)) => c,
        Ok(None) => return page(&d, StatusCode::NOT_FOUND, "This activity is not available."),
        Err(e) => {
            tracing::error!(error = %e, "activity lookup failed");
            return page(
                &d,
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong.",
            );
        }
    };
    let path_and_query = req
        .uri()
        .path_and_query()
        .map_or("/", |p| p.as_str())
        .to_owned();
    let head = req.method() == Method::HEAD;
    let mut resp = serve(&d, &cfg, &path_and_query).await;
    if head {
        *resp.body_mut() = Body::empty();
    }
    resp
}

/// Fetch and relay one path of an approved Activity.
pub(crate) async fn serve(d: &ActivitiesDeps, cfg: &ConfigView, path_and_query: &str) -> Response {
    let (Some(entry), Some(fetcher)) = (cfg.entry_url.as_deref(), d.fetcher.as_ref()) else {
        return page(
            d,
            StatusCode::SERVICE_UNAVAILABLE,
            "This activity is not available.",
        );
    };
    let target = match rules::target_for(entry, &cfg.url_mappings, path_and_query) {
        Ok(t) => t,
        Err(rules::Refusal::BadPath) => {
            return page(d, StatusCode::BAD_REQUEST, "That path is not allowed.");
        }
        Err(rules::Refusal::NoEntry) => {
            return page(
                d,
                StatusCode::SERVICE_UNAVAILABLE,
                "This activity is not available.",
            );
        }
    };
    let is_entry = path_and_query.split('?').next() == Some("/");
    let doc = match fetcher.get(target.as_str(), MAX_PROXIED_BYTES).await {
        Ok(doc) => doc,
        Err(why) => {
            tracing::info!(app = %cfg.application_id, reason = %why, "activity fetch failed");
            return page(
                d,
                StatusCode::BAD_GATEWAY,
                "The activity's server did not answer.",
            );
        }
    };
    if is_entry {
        let sha = HEXLOWER.encode(Sha256::digest(&doc.body).as_slice());
        if cfg.reviewed_sha256.as_deref() != Some(sha.as_str()) {
            return page(
                d,
                StatusCode::SERVICE_UNAVAILABLE,
                "This activity changed since it was reviewed and is paused until it is reviewed again.",
            );
        }
    }
    let ct = doc
        .content_type
        .as_deref()
        .and_then(|c| HeaderValue::from_str(c).ok())
        .unwrap_or_else(|| HeaderValue::from_static("application/octet-stream"));
    let mut r = Response::new(Body::from(doc.body));
    r.headers_mut().insert(header::CONTENT_TYPE, ct);
    harden(d, r.headers_mut());
    r
}

/// The security headers every response of the activity origin carries.
fn harden(d: &ActivitiesDeps, h: &mut HeaderMap) {
    let csp = format!(
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: blob:; font-src 'self'; media-src 'self' blob:; connect-src 'self'; \
         object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors {}",
        d.app_origin
    );
    let set = |h: &mut HeaderMap, name: header::HeaderName, v: &str| {
        if let Ok(v) = HeaderValue::from_str(v) {
            h.insert(name, v);
        }
    };
    set(h, header::CONTENT_SECURITY_POLICY, &csp);
    set(h, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    set(h, header::REFERRER_POLICY, "no-referrer");
    set(h, header::CACHE_CONTROL, "no-cache");
    set(
        h,
        header::HeaderName::from_static("permissions-policy"),
        "camera=(), microphone=(), geolocation=(), payment=(), usb=(), display-capture=(), \
         clipboard-read=(), clipboard-write=(), fullscreen=(self)",
    );
}

/// A small error page in the same hardened envelope.
fn page(d: &ActivitiesDeps, status: StatusCode, msg: &str) -> Response {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>Activity</title><p style=\"font-family:sans-serif\">{}</p>",
        msg.replace('&', "&amp;").replace('<', "&lt;")
    );
    let mut r = (status, body).into_response();
    r.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    harden(d, r.headers_mut());
    r
}
