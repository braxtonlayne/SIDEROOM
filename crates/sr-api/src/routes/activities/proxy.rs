//! The Activity proxy (DEV-ACT-001, D-247): `https://<application-id>.activities.<host>/...` serves an approved
//! Activity from its own origin, so it can never touch the app shell's cookies or storage.
//!
//! * Only when `[activities] enabled`; any other host name under `.activities.<host>` is a plain 404.
//! * What is served is the bundle staff reviewed: files of the stored bundle (`/` is `index.html`, a
//!   directory path is the `index.html` in it), found by a path that must pass the same rules as the entries
//!   of the zip. An application that is not released, has no reviewed bundle (or lost it by changing its URL
//!   mappings) is paused (503). The only live fetch of the application's own pages is `dev_entry_url` of an
//!   application in `development`, marked by `X-SR-Activity-Mode: development`.
//! * `GET` and `HEAD` for everything. `POST`, `PUT`, `PATCH` and `DELETE` only under a declared URL-mapping
//!   prefix (never for the shell or the bundle), body at most [`MAX_WRITE_BODY`], from the Activity's own
//!   origin only. A path under a mapping prefix is fetched from the mapping's target.
//! * Fetches go through the SSRF-safe client; the application's cookies and headers never reach the browser
//!   and the browser's never reach the application (only the content type and the body travel).
//! * Every response carries a strict `Content-Security-Policy` (`frame-ancestors` is the app shell only),
//!   `nosniff`, no referrer and a `Permissions-Policy` with the powerful features off.

use super::{ActivitiesDeps, MAX_PROXIED_BYTES};
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header},
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
};
use sr_activities::{Lookup, ServeSpec, Serving, bundle, rules};
use std::sync::Arc;

/// Largest request body forwarded through a URL mapping (J-17).
pub const MAX_WRITE_BODY: usize = 1024 * 1024;

/// Response header telling the shell the Activity runs live from a development URL.
pub const MODE_HEADER: &str = "x-sr-activity-mode";

/// Put the proxy in front of `app`: requests whose `Host` is under `.activities.<host>` are answered here,
/// everything else goes on to the app.
pub fn wrap_activity_host(app: Router, deps: Arc<ActivitiesDeps>) -> Router {
    app.layer(from_fn_with_state(deps, activity_host_layer))
}

fn read_method(m: &Method) -> bool {
    matches!(*m, Method::GET | Method::HEAD)
}

fn write_method(m: &Method) -> bool {
    matches!(
        *m,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
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
    if !read_method(req.method()) && !write_method(req.method()) {
        return not_allowed(&d);
    }
    let spec = match d.service.lookup(app).await {
        Ok(Lookup::Ready(s)) => s,
        Ok(Lookup::Paused) => {
            return page(
                &d,
                StatusCode::SERVICE_UNAVAILABLE,
                "This activity is paused until it is reviewed again.",
            );
        }
        Ok(Lookup::Unknown) => {
            return page(&d, StatusCode::NOT_FOUND, "This activity is not available.");
        }
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
    if read_method(req.method()) {
        let head = req.method() == Method::HEAD;
        let mut resp = serve(&d, app, &spec, &path_and_query).await;
        if head {
            *resp.body_mut() = Body::empty();
        }
        return resp;
    }
    let (parts, body) = req.into_parts();
    forward_write(
        &d,
        app,
        &spec,
        &parts.method,
        &parts.headers,
        &path_and_query,
        body,
    )
    .await
}

fn dev_mark(spec: &ServeSpec, resp: &mut Response) {
    if matches!(spec.serving, Serving::Dev { .. }) {
        resp.headers_mut().insert(
            HeaderName::from_static(MODE_HEADER),
            HeaderValue::from_static("development"),
        );
    }
}

/// Serve one `GET` path of an Activity: a path under a URL mapping comes from the mapping's target, anything
/// else from the reviewed bundle (or, in development, the live development URL).
pub(crate) async fn serve(
    d: &ActivitiesDeps,
    app: i64,
    spec: &ServeSpec,
    path_and_query: &str,
) -> Response {
    let Ok(mapped) = rules::mapping_target(&spec.url_mappings, path_and_query) else {
        return page(d, StatusCode::BAD_REQUEST, "That path is not allowed.");
    };
    let mut resp = match (mapped, &spec.serving) {
        (Some(target), _) => fetch(d, app, target.as_str()).await,
        (None, Serving::Dev { url }) => match rules::target_for(url, &[], path_and_query) {
            Ok(target) => fetch(d, app, target.as_str()).await,
            Err(rules::Refusal::BadPath) => {
                page(d, StatusCode::BAD_REQUEST, "That path is not allowed.")
            }
            Err(rules::Refusal::NoEntry) => page(
                d,
                StatusCode::SERVICE_UNAVAILABLE,
                "This activity is not available.",
            ),
        },
        (None, Serving::Bundle { sha256 }) => from_bundle(d, app, sha256, path_and_query).await,
    };
    dev_mark(spec, &mut resp);
    resp
}

/// One file of the reviewed bundle.
async fn from_bundle(
    d: &ActivitiesDeps,
    app: i64,
    sha256: &str,
    path_and_query: &str,
) -> Response {
    let path = path_and_query.split('?').next().unwrap_or("/");
    let Some(rel) = bundle::request_to_path(path) else {
        return page(d, StatusCode::BAD_REQUEST, "That path is not allowed.");
    };
    let Some(ct) = bundle::content_type_for(&rel) else {
        return page(d, StatusCode::NOT_FOUND, "There is nothing here.");
    };
    let data = match d.service.bundle_file(app, sha256, &rel).await {
        Ok(Some(data)) => data,
        Ok(None) => return page(d, StatusCode::NOT_FOUND, "There is nothing here."),
        Err(e) => {
            tracing::error!(error = %e, "reading an activity bundle failed");
            return page(
                d,
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong.",
            );
        }
    };
    let mut r = Response::new(Body::from(data));
    r.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(ct));
    harden(d, r.headers_mut());
    // The entry document must be revalidated so a new review shows at once; other paths have no hash in
    // their name, so a short lifetime keeps a replaced bundle from lingering.
    let cache = if rel == "index.html" || rel.ends_with("/index.html") {
        "no-cache"
    } else {
        "public, max-age=3600"
    };
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    r
}

/// `GET` a page of the application's own server and relay it.
async fn fetch(d: &ActivitiesDeps, app: i64, target: &str) -> Response {
    let Some(fetcher) = d.fetcher.as_ref() else {
        return page(
            d,
            StatusCode::SERVICE_UNAVAILABLE,
            "This activity is not available.",
        );
    };
    let doc = match fetcher.get(target, MAX_PROXIED_BYTES).await {
        Ok(doc) => doc,
        Err(why) => {
            tracing::info!(app, reason = %why, "activity fetch failed");
            return page(
                d,
                StatusCode::BAD_GATEWAY,
                "The activity's server did not answer.",
            );
        }
    };
    relay(d, StatusCode::OK, doc.content_type.as_deref(), doc.body)
}

fn relay(
    d: &ActivitiesDeps,
    status: StatusCode,
    content_type: Option<&str>,
    body: Vec<u8>,
) -> Response {
    let ct = content_type
        .and_then(|c| HeaderValue::from_str(c).ok())
        .unwrap_or_else(|| HeaderValue::from_static("application/octet-stream"));
    let mut r = Response::new(Body::from(body));
    *r.status_mut() = status;
    r.headers_mut().insert(header::CONTENT_TYPE, ct);
    harden(d, r.headers_mut());
    r
}

/// Forward a `POST`, `PUT`, `PATCH` or `DELETE` to a URL mapping's target (J-17).
pub(crate) async fn forward_write(
    d: &ActivitiesDeps,
    app: i64,
    spec: &ServeSpec,
    method: &Method,
    headers: &HeaderMap,
    path_and_query: &str,
    body: Body,
) -> Response {
    let target = match rules::mapping_target(&spec.url_mappings, path_and_query) {
        Ok(Some(t)) => t,
        Ok(None) => return not_allowed(d),
        Err(_) => return page(d, StatusCode::BAD_REQUEST, "That path is not allowed."),
    };
    // Only the Activity's own page may write: a browser names the page's origin on every cross-origin write.
    if headers
        .get(header::ORIGIN)
        .is_some_and(|o| o.as_bytes() != d.activity_origin(app).as_bytes())
    {
        return page(
            d,
            StatusCode::FORBIDDEN,
            "Requests must come from the activity itself.",
        );
    }
    if headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok())
        .is_some_and(|n| n > MAX_WRITE_BODY)
    {
        return page(
            d,
            StatusCode::PAYLOAD_TOO_LARGE,
            "That request is too large.",
        );
    }
    let Ok(bytes) = axum::body::to_bytes(body, MAX_WRITE_BODY).await else {
        return page(
            d,
            StatusCode::PAYLOAD_TOO_LARGE,
            "That request is too large.",
        );
    };
    let Some(fetcher) = d.fetcher.as_ref() else {
        return page(
            d,
            StatusCode::SERVICE_UNAVAILABLE,
            "This activity is not available.",
        );
    };
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.len() <= 200 && !v.chars().any(char::is_control))
        .map(str::to_owned);
    let sent = fetcher
        .forward(
            method,
            target.as_str(),
            content_type,
            bytes.to_vec(),
            MAX_PROXIED_BYTES,
        )
        .await;
    let mut resp = match sent {
        // The application's answer comes back as it is for success and client errors; anything else (a
        // redirect, a server error) is reported as the application not answering.
        Ok(f) if (200..300).contains(&f.status) || (400..500).contains(&f.status) => relay(
            d,
            StatusCode::from_u16(f.status).unwrap_or(StatusCode::BAD_GATEWAY),
            f.content_type.as_deref(),
            f.body,
        ),
        Ok(f) => {
            tracing::info!(app, status = f.status, "activity write status not relayed");
            page(
                d,
                StatusCode::BAD_GATEWAY,
                "The activity's server did not answer.",
            )
        }
        Err(why) => {
            tracing::info!(app, reason = %why, "activity write failed");
            page(
                d,
                StatusCode::BAD_GATEWAY,
                "The activity's server did not answer.",
            )
        }
    };
    dev_mark(spec, &mut resp);
    resp
}

fn not_allowed(d: &ActivitiesDeps) -> Response {
    let mut r = page(
        d,
        StatusCode::METHOD_NOT_ALLOWED,
        "This request is not allowed here.",
    );
    r.headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static("GET, HEAD"));
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
