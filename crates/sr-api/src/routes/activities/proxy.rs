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
//!   origin only (for the sandboxed game: `Origin: null` with its proxy token, below). A path under a mapping
//!   prefix is fetched from the mapping's target.
//! * Fetches go through the SSRF-safe client; the application's cookies and headers never reach the browser
//!   and the browser's never reach the application (only the content type and the body travel).
//! * Every response carries a strict `Content-Security-Policy` (`frame-ancestors` is the app shell only),
//!   `nosniff`, no referrer and a `Permissions-Policy` with the powerful features off.
//! * ARC-SBX-001, the hardened sandbox: the policy starts with `sandbox allow-scripts allow-pointer-lock`, so the
//!   page has an opaque origin even when it is opened directly or framed with a weaker `sandbox` attribute, and
//!   `connect-src` is `'none'` unless the reviewed manifest named hosts ([`sr_activities::sandbox::csp`]; CSP
//!   is per host, not per URL; an application's reviewed URL mappings admit `'self'`). Static bundle files and
//!   mapped `GET`s answer `Access-Control-Allow-Origin: *` because every fetch of an opaque-origin page is a
//!   CORS fetch (they carry no credentials, and anyone can already fetch them through the proxy).
//! * D-399, developer freedom: an opaque origin sends `Origin: null`, which any sandboxed page anywhere can send
//!   too. A write under a URL mapping with `Origin: null` is accepted only with the instance's own credential,
//!   the proxy token (`X-SR-Activity-Token`, minted for a participant by
//!   `POST /activity-instances/{instance_id}/proxy-token` and checked against a running instance of this
//!   application by `ActivityService::proxy_token_valid`), never by allowing any origin. The CORS preflight
//!   under a mapping admits that header, and only the answer to a write that passed the check carries
//!   `Access-Control-Allow-Origin: null`.
//! * OPS-TDEP-001 (D-450 to D-452, threat model T49), TEST DEPLOYMENTS ONLY: with `[activities] origin_mode =
//!   "same_host_sandboxed"` the same answers are served from `/_activity/<application-id>/...` on the main host
//!   (one host name, as a quick tunnel has) and the `.activities.` hosts answer 404. The policy is the same, its
//!   `sandbox` directive included, so the game's document still has an opaque origin; only `'self'` in
//!   `connect-src` becomes the Activity's own path. A request from the main origin itself (`Sec-Fetch-Site:
//!   same-origin`) is answered only for a frame or a document, never as a script, style or worker of the app.

use super::{ActivitiesDeps, MAX_PROXIED_BYTES};
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header},
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
};
use sr_activities::{Lookup, ServeSpec, Serving, bundle, rules, rules::ActivityPath, sandbox};
use std::sync::Arc;

/// Largest request body forwarded through a URL mapping (J-17).
pub const MAX_WRITE_BODY: usize = 1024 * 1024;

/// Response header telling the shell the Activity runs live from a development URL.
pub const MODE_HEADER: &str = "x-sr-activity-mode";

/// Request header carrying the proxy token of a write from the sandboxed (opaque-origin) game (D-399).
pub const TOKEN_HEADER: &str = "x-sr-activity-token";

/// `Origin` of an opaque-origin page (the sandboxed game).
const OPAQUE_ORIGIN: &[u8] = b"null";

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
    let activity_host = !d.base_host.is_empty() && rules::is_activity_host(&host, &d.base_host);
    let path_and_query = req
        .uri()
        .path_and_query()
        .map_or("/", |p| p.as_str())
        .to_owned();
    if d.same_host {
        // OPS-TDEP-001: one host; the per-application hosts are not served in this mode.
        if activity_host {
            return page(&d, StatusCode::NOT_FOUND, "There is no activity here.");
        }
        return match rules::app_of_path(&path_and_query) {
            ActivityPath::NotActivity => next.run(req).await,
            ActivityPath::App { app, rest } if d.enabled => {
                if let Some(refused) = main_origin_refusal(&d, req.headers()) {
                    return refused;
                }
                let mut r = answer(&d, app, req, rest).await;
                // The browser's cache must not hand the app a copy the game's frame fetched (D-452): the answer
                // depends on who asks.
                r.headers_mut().append(
                    header::VARY,
                    HeaderValue::from_static("sec-fetch-site, sec-fetch-dest"),
                );
                r
            }
            ActivityPath::NeedsSlash { location, .. } if d.enabled => {
                let mut r = page(&d, StatusCode::PERMANENT_REDIRECT, "Moved.");
                if let Ok(v) = HeaderValue::from_str(&location) {
                    r.headers_mut().insert(header::LOCATION, v);
                }
                r
            }
            _ => page(&d, StatusCode::NOT_FOUND, "There is no activity here."),
        };
    }
    if !activity_host {
        return next.run(req).await;
    }
    let Some(app) = d
        .enabled
        .then(|| rules::app_of_host(&host, &d.base_host))
        .flatten()
    else {
        return page(&d, StatusCode::NOT_FOUND, "There is no activity here.");
    };
    answer(&d, app, req, path_and_query).await
}

/// D-452: in the same-host mode a request the main origin makes itself (`Sec-Fetch-Site: same-origin`) is answered
/// only when it loads a frame or a document. The app shell (or anything injected into it) must never pull a
/// game's file in as its own script, style, worker or fetch, where it would run with the app's origin. The game's
/// own requests come from its opaque origin (`cross-site`), and a browser without fetch metadata is answered as
/// before (the policy's `sandbox` still applies to any document).
fn main_origin_refusal(d: &ActivitiesDeps, headers: &HeaderMap) -> Option<Response> {
    let site = headers.get("sec-fetch-site")?;
    if site.as_bytes() != b"same-origin" {
        return None;
    }
    let dest = headers
        .get("sec-fetch-dest")
        .map_or(&b""[..], HeaderValue::as_bytes);
    if matches!(dest, b"iframe" | b"frame" | b"document") {
        return None;
    }
    Some(page(
        d,
        StatusCode::FORBIDDEN,
        "Activity files are not served to the app itself.",
    ))
}

/// One request for application `app`, `path_and_query` being the path inside the Activity (`/` is its root).
async fn answer(d: &ActivitiesDeps, app: i64, req: Request, path_and_query: String) -> Response {
    // A CORS preflight is answered only for the sandboxed game (`Origin: null`); any other OPTIONS is refused
    // before the lookup like every method that is never forwarded.
    let preflight = req.method() == Method::OPTIONS
        && req
            .headers()
            .get(header::ORIGIN)
            .is_some_and(|o| o.as_bytes() == OPAQUE_ORIGIN);
    if !preflight && !read_method(req.method()) && !write_method(req.method()) {
        return not_allowed(d);
    }
    let spec = match d.service.lookup(app).await {
        Ok(Lookup::Ready(s)) => s,
        Ok(Lookup::Paused) => {
            return page(
                d,
                StatusCode::SERVICE_UNAVAILABLE,
                "This activity is paused until it is reviewed again.",
            );
        }
        Ok(Lookup::Unknown) => {
            return page(d, StatusCode::NOT_FOUND, "This activity is not available.");
        }
        Err(e) => {
            tracing::error!(error = %e, "activity lookup failed");
            return page(
                d,
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong.",
            );
        }
    };
    if preflight {
        return preflight_answer(d, &spec, req.headers(), &path_and_query);
    }
    if read_method(req.method()) {
        let head = req.method() == Method::HEAD;
        let mut resp = serve_with(
            d,
            app,
            &spec,
            &path_and_query,
            req.headers().get(header::RANGE),
            head,
        )
        .await;
        if head {
            *resp.body_mut() = Body::empty();
        }
        return resp;
    }
    let (parts, body) = req.into_parts();
    forward_write(
        d,
        app,
        &spec,
        &parts.method,
        &parts.headers,
        &path_and_query,
        body,
    )
    .await
}

/// Replace the default policy with the one this application's reviewed manifest earns it. Error answers keep
/// the strictest policy.
fn apply_csp(d: &ActivitiesDeps, app: i64, spec: &ServeSpec, resp: &mut Response) {
    if resp.status().is_client_error() || resp.status().is_server_error() {
        return;
    }
    let own = d.connect_self(app);
    let policy = sandbox::csp(&sandbox::CspInput {
        app_origin: &d.app_origin,
        development: matches!(spec.serving, Serving::Dev { .. }),
        has_mappings: !spec.url_mappings.is_empty(),
        connect: &spec.connect,
        self_source: own.as_deref(),
    });
    if let Ok(v) = HeaderValue::from_str(&policy) {
        resp.headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, v);
    }
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
    serve_with(d, app, spec, path_and_query, None, false).await
}

/// [`serve`] with the request's `Range` header (a single range of a bundle file is answered `206`, D-657) and
/// whether the request is a `HEAD` (the body is never fetched).
pub(crate) async fn serve_with(
    d: &ActivitiesDeps,
    app: i64,
    spec: &ServeSpec,
    path_and_query: &str,
    range: Option<&HeaderValue>,
    head: bool,
) -> Response {
    let Ok(mapped) = rules::mapping_target(&spec.url_mappings, path_and_query) else {
        return page(d, StatusCode::BAD_REQUEST, "That path is not allowed.");
    };
    let mut resp = match (mapped, &spec.serving) {
        (Some(target), _) => {
            let mut r = fetch(d, app, target.as_str()).await;
            // The sandboxed game's GET of its own mapping is a CORS fetch; it carries no credentials.
            r.headers_mut().insert(
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static("*"),
            );
            r
        }
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
    apply_csp(d, app, spec, &mut resp);
    resp
}

/// What a `Range` header asks of a file of `len` bytes (RFC 9110 section 14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RangeAsk {
    /// No usable single range (absent, several ranges, an unknown unit, a malformed value): the whole file, `200`.
    Whole,
    /// One range that can be met: bytes `start..=end`, `206`.
    Partial { start: u64, end: u64 },
    /// One range that cannot be met: `416`.
    Unsatisfiable,
}

/// Parse a `Range` header against the file's length. Only `bytes=a-b`, `a-` and `-n` in a single range are honoured;
/// several ranges are answered with the whole file (allowed by the RFC, and it spares the server multipart bodies).
pub(crate) fn range_ask(header: Option<&HeaderValue>, len: u64) -> RangeAsk {
    let Some(v) = header.and_then(|h| h.to_str().ok()) else {
        return RangeAsk::Whole;
    };
    let Some(spec) = v.trim().strip_prefix("bytes=") else {
        return RangeAsk::Whole;
    };
    if spec.contains(',') {
        return RangeAsk::Whole;
    }
    let Some((a, b)) = spec.trim().split_once('-') else {
        return RangeAsk::Whole;
    };
    let (a, b) = (a.trim(), b.trim());
    let num = |s: &str| s.parse::<u64>().ok();
    match (a.is_empty(), b.is_empty()) {
        // `-n`: the last n bytes.
        (true, false) => match num(b) {
            Some(0) => RangeAsk::Unsatisfiable,
            Some(_) if len == 0 => RangeAsk::Unsatisfiable,
            Some(n) => RangeAsk::Partial {
                start: len.saturating_sub(n),
                end: len - 1,
            },
            None => RangeAsk::Whole,
        },
        // `a-`: from a to the end.
        (false, true) => match num(a) {
            Some(s) if s < len => RangeAsk::Partial {
                start: s,
                end: len - 1,
            },
            Some(_) => RangeAsk::Unsatisfiable,
            None => RangeAsk::Whole,
        },
        // `a-b`.
        (false, false) => match (num(a), num(b)) {
            (Some(s), Some(e)) if s > e => RangeAsk::Whole,
            (Some(s), Some(e)) if s < len => RangeAsk::Partial {
                start: s,
                end: e.min(len - 1),
            },
            (Some(_), Some(_)) => RangeAsk::Unsatisfiable,
            _ => RangeAsk::Whole,
        },
        (true, true) => RangeAsk::Whole,
    }
}

/// One file of the reviewed bundle, streamed from the store (D-657): `Accept-Ranges: bytes`, a single range answered
/// `206` with `Content-Range`, one that cannot be met `416` with `Content-Range: bytes */len`. The file is never held
/// whole. Every header a whole answer carries (the sandbox policy, the content type, `nosniff`, the cache rule) is on
/// the `206` too; `HEAD` fetches nothing.
async fn from_bundle(
    d: &ActivitiesDeps,
    app: i64,
    sha256: &str,
    path_and_query: &str,
    range: Option<&HeaderValue>,
    head: bool,
) -> Response {
    let path = path_and_query.split('?').next().unwrap_or("/");
    let Some(rel) = bundle::request_to_path(path) else {
        return page(d, StatusCode::BAD_REQUEST, "That path is not allowed.");
    };
    let Some(ct) = bundle::content_type_for(&rel) else {
        return page(d, StatusCode::NOT_FOUND, "There is nothing here.");
    };
    let oops = |e: &dyn std::fmt::Display| {
        tracing::error!(error = %e, "reading an activity bundle failed");
        page(
            d,
            StatusCode::INTERNAL_SERVER_ERROR,
            "Something went wrong.",
        )
    };
    let nothing = || page(d, StatusCode::NOT_FOUND, "There is nothing here.");
    // A range needs the length first (a suffix or an open range is relative to it); so does `HEAD`.
    let mut r = if range.is_some() || head {
        let len = match d.service.bundle_file_len(app, sha256, &rel).await {
            Ok(Some(n)) => n,
            Ok(None) => return nothing(),
            Err(e) => return oops(&e),
        };
        match range_ask(range, len) {
            RangeAsk::Unsatisfiable => {
                let mut r = page(
                    d,
                    StatusCode::RANGE_NOT_SATISFIABLE,
                    "That part of the file does not exist.",
                );
                if let Ok(v) = HeaderValue::from_str(&format!("bytes */{len}")) {
                    r.headers_mut().insert(header::CONTENT_RANGE, v);
                }
                r.headers_mut()
                    .insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
                return r;
            }
            RangeAsk::Partial { start, end } => {
                let body = if head {
                    Body::empty()
                } else {
                    match d
                        .service
                        .bundle_file_stream(app, sha256, &rel, Some((start, end + 1)))
                        .await
                    {
                        Ok(Some(s)) => Body::from_stream(s.body),
                        Ok(None) => return nothing(),
                        Err(e) => return oops(&e),
                    }
                };
                let mut r = Response::new(body);
                *r.status_mut() = StatusCode::PARTIAL_CONTENT;
                if let Ok(v) = HeaderValue::from_str(&format!("bytes {start}-{end}/{len}")) {
                    r.headers_mut().insert(header::CONTENT_RANGE, v);
                }
                r.headers_mut()
                    .insert(header::CONTENT_LENGTH, HeaderValue::from(end - start + 1));
                r
            }
            RangeAsk::Whole => {
                let body = if head {
                    Body::empty()
                } else {
                    match d.service.bundle_file_stream(app, sha256, &rel, None).await {
                        Ok(Some(s)) => Body::from_stream(s.body),
                        Ok(None) => return nothing(),
                        Err(e) => return oops(&e),
                    }
                };
                let mut r = Response::new(body);
                r.headers_mut()
                    .insert(header::CONTENT_LENGTH, HeaderValue::from(len));
                r
            }
        }
    } else {
        match d.service.bundle_file_stream(app, sha256, &rel, None).await {
            Ok(Some(s)) => {
                let len = s.info.size;
                let mut r = Response::new(Body::from_stream(s.body));
                r.headers_mut()
                    .insert(header::CONTENT_LENGTH, HeaderValue::from(len));
                r
            }
            Ok(None) => return nothing(),
            Err(e) => return oops(&e),
        }
    };
    r.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(ct));
    // `harden` sets a default `Cache-Control` and the strictest policy; the cache rule and `apply_csp` below
    // replace them. It does not touch the length or range headers set above.
    harden(d, r.headers_mut());
    r.headers_mut()
        .insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    // The entry document must be revalidated so a new review shows at once; other paths have no hash in
    // their name, so a short lifetime keeps a replaced bundle from lingering.
    let cache = if rel == "index.html" || rel.ends_with("/index.html") {
        "no-cache"
    } else {
        "public, max-age=3600"
    };
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    // The page is sandboxed without `allow-same-origin`, so its module scripts, fonts and fetches of its own
    // files are cross-origin (CORS) requests. The files are the reviewed bundle, readable by anyone who knows
    // the application id, and the request carries no credentials, so `*` exposes nothing new.
    r.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    // A cross-origin reader of ranges (a video element, a fetch) may see what it needs.
    r.headers_mut().insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("content-range, accept-ranges, content-length"),
    );
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
    // The sandboxed game's origin is opaque (`null`), which any sandboxed page can claim, so then the request
    // must also carry the proxy token of a participant of a running instance of this application (D-399).
    let opaque = match headers.get(header::ORIGIN) {
        None => false,
        Some(o) if o.as_bytes() == OPAQUE_ORIGIN => true,
        // In the same-host mode no page of the Activity has an origin of its own (its policy sandboxes every
        // document), so only the opaque one with the token may write (D-452).
        Some(o) if !d.same_host && o.as_bytes() == d.activity_origin(app).as_bytes() => false,
        Some(_) => {
            return page(
                d,
                StatusCode::FORBIDDEN,
                "Requests must come from the activity itself.",
            );
        }
    };
    if opaque {
        let token = headers
            .get(TOKEN_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        match d.service.proxy_token_valid(app, token).await {
            Ok(true) => {}
            Ok(false) => {
                return page(
                    d,
                    StatusCode::FORBIDDEN,
                    "Requests must come from the activity itself.",
                );
            }
            Err(e) => {
                tracing::error!(error = %e, "activity proxy token check failed");
                return page(
                    d,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Something went wrong.",
                );
            }
        }
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
    apply_csp(d, app, spec, &mut resp);
    if opaque {
        // The token was checked: the game may read the answer.
        resp.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("null"),
        );
        resp.headers_mut()
            .insert(header::VARY, HeaderValue::from_static("origin"));
    }
    resp
}

/// The CORS preflight of a write from the sandboxed game: only under a URL mapping and only for `Origin: null`
/// (the activity's own origin never needs one). It admits the proxy token header; the write itself is checked
/// in [`forward_write`]. Anything else is the usual 405.
pub(crate) fn preflight_answer(
    d: &ActivitiesDeps,
    spec: &ServeSpec,
    headers: &HeaderMap,
    path_and_query: &str,
) -> Response {
    let mapped = matches!(
        rules::mapping_target(&spec.url_mappings, path_and_query),
        Ok(Some(_))
    );
    let opaque = headers
        .get(header::ORIGIN)
        .is_some_and(|o| o.as_bytes() == OPAQUE_ORIGIN);
    if !mapped || !opaque {
        return not_allowed(d);
    }
    let mut r = Response::new(Body::empty());
    *r.status_mut() = StatusCode::NO_CONTENT;
    harden(d, r.headers_mut());
    let h = r.headers_mut();
    h.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("null"),
    );
    h.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST, PUT, PATCH, DELETE"),
    );
    h.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type, x-sr-activity-token"),
    );
    h.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("600"),
    );
    h.insert(header::VARY, HeaderValue::from_static("origin"));
    r
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
    // Error pages and anything not tied to a reviewed manifest get the strictest policy (`connect-src 'none'`);
    // `apply_csp` swaps in the application's own after a bundle or mapped response is built.
    let csp = sandbox::csp(&sandbox::CspInput {
        app_origin: &d.app_origin,
        development: false,
        has_mappings: false,
        connect: &[],
        self_source: None,
    });
    let set = |h: &mut HeaderMap, name: header::HeaderName, v: &str| {
        if let Ok(v) = HeaderValue::from_str(v) {
            h.insert(name, v);
        }
    };
    set(h, header::CONTENT_SECURITY_POLICY, &csp);
    set(h, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    // OPS-IDX-001 (D-384): an activity host is never indexable.
    set(
        h,
        header::HeaderName::from_static("x-robots-tag"),
        "noindex, nofollow",
    );
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
