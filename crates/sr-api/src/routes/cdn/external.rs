//! The external image proxy (LNG-MSG-014, F-09, D-176):
//! `GET /cdn/external/{hmac}/{url_b64}`.
//!
//! The client never loads a third-party image directly. A message embed carries
//! `proxy_url` values (see [`proxy_embed_images`]) that point here; this route
//! fetches the original on the server's behalf, under these rules:
//!
//! * **Signed.** `hmac` covers the URL (`CdnSigner::external_path`); a bad
//!   signature is 403 and nothing is fetched. The route is not an open relay.
//! * **SSRF-safe.** The fetch goes through `sr_unfurl::SafeFetcher`: every
//!   address of every hop (the first request and each redirect) is resolved and
//!   checked after DNS, the connection is pinned to the checked addresses, and
//!   private, loopback, link-local, metadata and mapped/embedded forms are
//!   refused. Refusals are a bare 403 that does not say what was resolved.
//! * **Bounded.** 10 MiB and 10 s per fetch; at most [`FETCH_CONCURRENCY`]
//!   fetches and transcodes at once.
//! * **Only real images.** The declared type must be an image, the bytes must
//!   sniff as PNG, JPEG, GIF or WebP and must decode within the pixel budget.
//!   SVG, HTML, AVIF and everything else is 415.
//! * **Re-encoded.** The pixels are re-encoded (JPEG stays JPEG, an animated
//!   GIF stays animated, the rest becomes PNG; at most 2048 px a side), so EXIF,
//!   XMP, ICC and text chunks never reach the viewer.
//! * **Cached.** The result is stored under `external/` in the bucket for 30
//!   days (an 12 byte header holds the expiry; an hourly job deletes expired
//!   objects) and served with `Cache-Control: public, max-age=86400`. Failures
//!   are remembered for ten minutes so a dead link is not refetched per view.
//! * **Rate limited.** Every request counts against the per-IP bucket; a cache
//!   miss also counts against a per-user (session) or per-IP bucket and a global
//!   one, so one member cannot make the server crawl the internet.

use super::serve::{header_str, matches_etag, put};
use crate::{
    Principal,
    rate_limit::{Decision, client_ip},
    state::AppState,
};
use axum::{
    extract::{FromRequestParts, Path, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use moka::future::Cache;
use sha2::{Digest, Sha256};
use sr_storage::{
    ObjectStorage, StorageError,
    cdn::{CdnSigner, VerifyError},
    img::{self, Fit, ImgFormat, Limits},
};
use sr_unfurl::{FetchError, SafeFetcher, fetch::MAX_PROXY_IMAGE_BYTES};
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

/// Rate-limit bucket of every request (per client address).
pub const ROUTE_KEY: &str = "GET /cdn/external/{hmac}/{url_b64}";
/// Bucket of cache misses, per user (or per address when anonymous).
pub const FETCH_KEY: &str = "GET /cdn/external (fetch)";
/// Bucket of cache misses across all users.
pub const GLOBAL_KEY: &str = "GET /cdn/external (global)";
/// Identity of the global bucket.
const GLOBAL_IDENTITY: &str = "cdn-external-global";

/// Object key prefix of the cache.
pub const CACHE_PREFIX: &str = "external/";
/// How long a cached copy lives.
pub const CACHE_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// How long a failed fetch is remembered.
pub const FAILURE_TTL: Duration = Duration::from_secs(10 * 60);
/// Browser cache lifetime of a served image.
pub const BROWSER_MAX_AGE: u64 = 24 * 60 * 60;
/// Concurrent upstream fetches plus transcodes.
pub const FETCH_CONCURRENCY: usize = 6;
/// Longest edge of a served image.
pub const MAX_EDGE: u32 = 2048;

const CACHE_MAGIC: &[u8; 4] = b"SRX1";
const CACHE_HEADER_LEN: usize = 12;
/// Cached objects examined per garbage collection run.
const GC_MAX_OBJECTS: usize = 20_000;

/// Services the proxy needs; register with `AppStateBuilder::extension`.
#[derive(Clone)]
pub struct ExternalImageDeps {
    /// Validating fetcher (policy, resolver and transport inside).
    pub fetcher: SafeFetcher,
    /// Where the cache lives.
    pub storage: Arc<dyn ObjectStorage>,
    /// Verifies the `hmac` path segment.
    pub signer: CdnSigner,
    /// Decode budget.
    pub limits: Limits,
    permits: Arc<Semaphore>,
    failed: Cache<String, u16>,
    now_ms: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl ExternalImageDeps {
    /// Bundle the services with the default budget and the system clock.
    pub fn new(fetcher: SafeFetcher, storage: Arc<dyn ObjectStorage>, signer: CdnSigner) -> Self {
        Self {
            fetcher,
            storage,
            signer,
            limits: Limits {
                max_bytes: MAX_PROXY_IMAGE_BYTES as u64,
                ..Limits::default()
            },
            permits: Arc::new(Semaphore::new(FETCH_CONCURRENCY)),
            failed: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(FAILURE_TTL)
                .build(),
            now_ms: Arc::new(system_now_ms),
        }
    }

    /// Replace the clock (tests).
    pub fn with_clock(mut self, now_ms: Arc<dyn Fn() -> i64 + Send + Sync>) -> Self {
        self.now_ms = now_ms;
        self
    }
}

fn system_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn plain(status: StatusCode, msg: &'static str) -> Response {
    let mut r = (status, msg).into_response();
    let h = r.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}

fn limited(retry_after: f64) -> Response {
    let mut r = plain(StatusCode::TOO_MANY_REQUESTS, "rate limited");
    if let Ok(v) = HeaderValue::from_str(&(retry_after.max(0.0).ceil() as u64).max(1).to_string()) {
        r.headers_mut().insert(header::RETRY_AFTER, v);
    }
    r
}

/// Per-IP limit on every request to the proxy.
pub(super) fn ip_identity(req: &Request, trust_forwarded_for: bool) -> String {
    client_ip(req, trust_forwarded_for).map_or_else(
        || "ip:unknown".to_owned(),
        |ip| format!("cdn-external-ip:{ip}"),
    )
}

/// The cache key of `url`.
pub fn cache_key(url: &str) -> String {
    format!(
        "{CACHE_PREFIX}{}",
        data_encoding::HEXLOWER.encode(&Sha256::digest(url.as_bytes()))
    )
}

/// `SRX1`, the expiry in unix milliseconds (big endian), then the image bytes.
fn pack(expires_at_ms: i64, image: &[u8]) -> Bytes {
    let mut v = Vec::with_capacity(CACHE_HEADER_LEN + image.len());
    v.extend_from_slice(CACHE_MAGIC);
    v.extend_from_slice(&expires_at_ms.to_be_bytes());
    v.extend_from_slice(image);
    Bytes::from(v)
}

/// The expiry stored in a cached object's first bytes.
fn unpack_expiry(head: &[u8]) -> Option<i64> {
    if head.len() < CACHE_HEADER_LEN || &head[..4] != CACHE_MAGIC {
        return None;
    }
    let raw: [u8; 8] = head[4..CACHE_HEADER_LEN].try_into().ok()?;
    Some(i64::from_be_bytes(raw))
}

/// Delete expired (and unreadable) cache objects. Returns how many were removed.
/// Run hourly by the job runner.
pub async fn gc_expired(storage: &dyn ObjectStorage, now_ms: i64) -> Result<usize, StorageError> {
    let mut removed = 0;
    for info in storage
        .list(CACHE_PREFIX)
        .await?
        .into_iter()
        .take(GC_MAX_OBJECTS)
    {
        let head = match storage
            .get_range(&info.key, 0, (CACHE_HEADER_LEN as u64).min(info.size))
            .await
        {
            Ok(h) => h,
            Err(StorageError::NotFound(_)) => continue,
            Err(e) => return Err(e),
        };
        let expired = unpack_expiry(&head).is_none_or(|exp| exp <= now_ms);
        if expired {
            storage.delete(&info.key).await?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// How a failed fetch is reported. Never includes what was resolved.
fn status_for(e: &FetchError) -> u16 {
    match e {
        FetchError::Blocked(_) | FetchError::BadUrl(_) => 403,
        FetchError::Timeout => 504,
        FetchError::TooLarge => 413,
        FetchError::UnsupportedType => 415,
        _ => 502,
    }
}

fn failure(status: u16) -> Response {
    match status {
        403 => plain(StatusCode::FORBIDDEN, "this address cannot be proxied"),
        413 => plain(StatusCode::PAYLOAD_TOO_LARGE, "image too large"),
        415 => plain(StatusCode::UNSUPPORTED_MEDIA_TYPE, "not a supported image"),
        504 => plain(StatusCode::GATEWAY_TIMEOUT, "the image host timed out"),
        _ => plain(StatusCode::BAD_GATEWAY, "the image could not be fetched"),
    }
}

/// Strong ETag of a cached copy: its URL hash and size.
fn etag(key: &str, size: usize) -> String {
    let mut h = Sha256::new();
    h.update(key.as_bytes());
    h.update([0]);
    h.update(size.to_string().as_bytes());
    format!(
        "\"{}\"",
        data_encoding::HEXLOWER.encode(&h.finalize()[..16])
    )
}

fn image_response(key: &str, image: Bytes, req_headers: &HeaderMap, head_only: bool) -> Response {
    let Some(format) = ImgFormat::sniff(&image) else {
        // A cached object that no longer sniffs as an image is never served.
        return plain(StatusCode::BAD_GATEWAY, "the image could not be fetched");
    };
    let tag = etag(key, image.len());
    let mut headers = HeaderMap::new();
    put(&mut headers, header::ETAG, &tag);
    put(
        &mut headers,
        header::CACHE_CONTROL,
        &format!("public, max-age={BROWSER_MAX_AGE}"),
    );
    put(&mut headers, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    put(
        &mut headers,
        header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; sandbox",
    );
    put(&mut headers, header::REFERRER_POLICY, "no-referrer");
    put(
        &mut headers,
        header::HeaderName::from_static("cross-origin-resource-policy"),
        "same-site",
    );
    put(&mut headers, header::VARY, "Origin");
    if header_str(req_headers, header::IF_NONE_MATCH).is_some_and(|l| matches_etag(l, &tag)) {
        return (StatusCode::NOT_MODIFIED, headers).into_response();
    }
    put(&mut headers, header::CONTENT_TYPE, format.mime());
    put(
        &mut headers,
        header::CONTENT_DISPOSITION,
        &format!("inline; filename=\"image.{}\"", format.ext()),
    );
    put(
        &mut headers,
        header::CONTENT_LENGTH,
        &image.len().to_string(),
    );
    if head_only {
        return (StatusCode::OK, headers).into_response();
    }
    (StatusCode::OK, headers, image).into_response()
}

/// The handler.
pub(super) async fn serve(
    State(state): State<AppState>,
    Path((sig, url_b64)): Path<(String, String)>,
    req: Request,
) -> Response {
    let Some(deps) = state.extension::<ExternalImageDeps>() else {
        return plain(StatusCode::NOT_FOUND, "not found");
    };
    let url_text = match deps.signer.verify_external(&sig, &url_b64) {
        Ok(u) => u,
        Err(VerifyError::Malformed) => return plain(StatusCode::BAD_REQUEST, "bad link"),
        Err(_) => return plain(StatusCode::FORBIDDEN, "invalid signature"),
    };
    let head_only = req.method() == axum::http::Method::HEAD;
    let key = cache_key(&url_text);
    let now_ms = (deps.now_ms)();

    // Cache hit.
    match deps.storage.get(&key).await {
        Ok(raw) => {
            if unpack_expiry(&raw).is_some_and(|exp| exp > now_ms) {
                return image_response(
                    &key,
                    raw.slice(CACHE_HEADER_LEN..),
                    req.headers(),
                    head_only,
                );
            }
        }
        Err(StorageError::NotFound(_)) => {}
        Err(e) => {
            tracing::error!(error = %e, "external image cache read failed");
            return plain(StatusCode::BAD_GATEWAY, "storage unavailable");
        }
    }
    if let Some(status) = deps.failed.get(&key).await {
        return failure(status);
    }

    // Cache miss: the expensive path. Identify the caller for the fetch buckets.
    let ip = ip_identity(&req, state.settings().trust_forwarded_for);
    let (mut parts, _) = req.into_parts();
    let who = match Principal::from_request_parts(&mut parts, &state).await {
        Ok(p) => format!("cdn-external-user:{}", p.user_id),
        Err(_) => ip,
    };
    for (route, identity) in [(FETCH_KEY, who.as_str()), (GLOBAL_KEY, GLOBAL_IDENTITY)] {
        if let Decision::Limited { retry_after, .. } =
            state.rate_limiter().check(route, "", identity)
        {
            return limited(retry_after);
        }
    }
    let Ok(Ok(_permit)) =
        tokio::time::timeout(Duration::from_secs(2), deps.permits.clone().acquire_owned()).await
    else {
        let mut r = plain(StatusCode::SERVICE_UNAVAILABLE, "busy, try again");
        r.headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("5"));
        return r;
    };

    let image = match fetch_and_clean(&deps, &url_text).await {
        Ok(img) => img,
        Err(status) => {
            deps.failed.insert(key.clone(), status).await;
            return failure(status);
        }
    };
    let expires = now_ms.saturating_add(i64::try_from(CACHE_TTL.as_millis()).unwrap_or(i64::MAX));
    if let Err(e) = deps.storage.put(&key, pack(expires, &image)).await {
        // Serving still works without the cache.
        tracing::warn!(error = %e, "external image cache write failed");
    }
    image_response(&key, Bytes::from(image), &parts.headers, head_only)
}

/// Download, sniff, decode and re-encode. `Err` carries the status to answer with.
async fn fetch_and_clean(deps: &ExternalImageDeps, url_text: &str) -> Result<Vec<u8>, u16> {
    let url = url::Url::parse(url_text).map_err(|_| 403_u16)?;
    let fetched = deps
        .fetcher
        .fetch_image(&url, MAX_PROXY_IMAGE_BYTES)
        .await
        .map_err(|e| {
            tracing::debug!(error = %e, "external image fetch refused");
            status_for(&e)
        })?;
    let body = fetched.body;
    if ImgFormat::sniff(&body).is_none() {
        return Err(415);
    }
    let limits = deps.limits;
    tokio::task::spawn_blocking(move || {
        img::normalize(
            &body,
            Fit::Bound {
                w: MAX_EDGE,
                h: MAX_EDGE,
            },
            &limits,
        )
    })
    .await
    .map_err(|_| 502_u16)?
    .map(|n| n.bytes)
    .map_err(|e| match e {
        img::ImgError::TooLarge => 413,
        _ => 415,
    })
}

/// Add `proxy_url` to the third-party images of embed JSON values (Discord's
/// shape): `image`, `thumbnail` and `video` get `proxy_url`, `author` and
/// `footer` get `proxy_icon_url`. Any `proxy_*` value a client supplied is
/// removed first. URLs that are not absolute http(s) (our own relative CDN paths,
/// `attachment://`) are left alone.
pub fn proxy_embed_images(embeds: &mut [serde_json::Value], signer: &CdnSigner) {
    use serde_json::Value;
    fn set(parent: &mut Value, url_key: &str, proxy_key: &str, signer: &CdnSigner) {
        let Some(obj) = parent.as_object_mut() else {
            return;
        };
        obj.remove(proxy_key);
        let proxied = obj
            .get(url_key)
            .and_then(Value::as_str)
            .and_then(|u| signer.external_path(u));
        if let Some(p) = proxied {
            obj.insert(proxy_key.to_owned(), Value::String(p));
        }
    }
    for e in embeds {
        for k in ["image", "thumbnail"] {
            if let Some(m) = e.get_mut(k) {
                set(m, "url", "proxy_url", signer);
            }
        }
        // A video is not proxied (the proxy only serves images), but its client
        // supplied proxy_url must not survive.
        if let Some(obj) = e.get_mut("video").and_then(Value::as_object_mut) {
            obj.remove("proxy_url");
        }
        if let Some(a) = e.get_mut("author") {
            set(a, "icon_url", "proxy_icon_url", signer);
        }
        if let Some(f) = e.get_mut("footer") {
            set(f, "icon_url", "proxy_icon_url", signer);
        }
    }
}
