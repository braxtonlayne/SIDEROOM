//! The `/cdn/{*path}` handler.

use super::{CdnDeps, MAX_CACHE, content, path, variant};
use crate::{rate_limit::Decision, state::AppState};
use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use futures::TryStreamExt;
use sha2::{Digest, Sha256};
use sr_storage::{
    StorageError,
    cdn::{ByteRange, RangeError, VerifyError},
};

/// Leading bytes read to sniff a type when the extension says nothing.
const SNIFF_BYTES: u64 = 512;

const ROUTE_KEY: &str = "GET /cdn/{*path}";

pub(super) fn plain(status: StatusCode, msg: &'static str) -> Response {
    let mut r = (status, msg).into_response();
    let h = r.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}

/// Per-IP rate limit for the CDN (same limiter and 429 shape as the API).
pub(super) async fn rate_limit(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    // The external image proxy has its own, tighter buckets.
    let (route, who) = if req.uri().path().starts_with("/cdn/external/") {
        (
            super::external::ROUTE_KEY,
            super::external::ip_identity(&req, state.settings().trust_forwarded_for),
        )
    } else {
        (
            ROUTE_KEY,
            crate::rate_limit::client_ip(&req, state.settings().trust_forwarded_for)
                .map_or_else(|| "ip:unknown".to_owned(), |ip| format!("cdn-ip:{ip}")),
        )
    };
    match state.rate_limiter().check(route, "", &who) {
        Decision::Allowed(_) => next.run(req).await,
        Decision::Limited { retry_after, .. } => {
            let mut r = plain(StatusCode::TOO_MANY_REQUESTS, "rate limited");
            if let Ok(v) = HeaderValue::from_str(&(retry_after.max(0.0).ceil() as u64).to_string())
            {
                r.headers_mut().insert(header::RETRY_AFTER, v);
            }
            r
        }
    }
}

/// Strong ETag from the key and the backend's tag (or size) so it never leaks
/// backend-internal values.
fn etag(key: &str, backend: Option<&str>, size: u64) -> String {
    let mut h = Sha256::new();
    h.update(key.as_bytes());
    h.update([0]);
    match backend {
        Some(t) => h.update(t.as_bytes()),
        None => h.update(size.to_string().as_bytes()),
    }
    let d = h.finalize();
    format!("\"{}\"", data_encoding::HEXLOWER.encode(&d[..16]))
}

/// RFC 9110 weak comparison of an `If-None-Match` list against `etag`.
pub(super) fn matches_etag(list: &str, etag: &str) -> bool {
    list.split(',').any(|t| {
        let t = t.trim();
        t == "*" || t.strip_prefix("W/").unwrap_or(t) == etag
    })
}

pub(super) fn header_str(h: &HeaderMap, name: HeaderName) -> Option<&str> {
    h.get(name).and_then(|v| v.to_str().ok())
}

pub(super) fn put(h: &mut HeaderMap, n: HeaderName, v: &str) {
    if let Ok(v) = HeaderValue::from_str(v) {
        h.insert(n, v);
    }
}

pub(super) async fn serve(State(state): State<AppState>, req: Request) -> Response {
    let Some(deps) = state.extension::<CdnDeps>() else {
        tracing::error!("cdn route needs CdnDeps registered on AppState");
        return plain(StatusCode::INTERNAL_SERVER_ERROR, "cdn not configured");
    };
    let head_only = req.method() == Method::HEAD;
    let parsed = match path::parse(req.uri().path()) {
        Ok(p) => p,
        Err(path::PathError::UnknownKind) => return plain(StatusCode::NOT_FOUND, "not found"),
        Err(path::PathError::Invalid) => return plain(StatusCode::BAD_REQUEST, "bad path"),
    };

    // The signature is the credential; nothing below runs without it. Public
    // kinds (avatars, icons) are addressed by an unguessable content hash and
    // may omit it, but a signature that is present must still be valid.
    let now = u64::try_from(state.now_secs()).unwrap_or(0);
    let query = req.uri().query().unwrap_or("");
    let has_sig = query
        .split('&')
        .any(|p| p.starts_with("sig=") || p.starts_with("ex="));
    let unsigned_public = parsed.public && !has_sig;
    if !unsigned_public && let Err(e) = deps.signer.verify_query(&parsed.canonical, query, now) {
        return match e {
            VerifyError::Expired => plain(StatusCode::GONE, "link expired"),
            VerifyError::Malformed | VerifyError::BadSignature => {
                plain(StatusCode::FORBIDDEN, "invalid signature")
            }
        };
    }
    // A member's or a server's bucket: redirect to a presigned URL (BYOS), no bytes through here.
    if let Some(mount_id) = parsed.mount {
        let left = query
            .split('&')
            .find_map(|p| p.strip_prefix("ex=").and_then(|v| v.parse::<u64>().ok()))
            .unwrap_or(now)
            .saturating_sub(now);
        return super::mount::redirect(deps.as_ref(), mount_id, &parsed.key, left).await;
    }
    // A takedown blocked this object (spec 5, step 3). The signature was checked first, so only a holder of a
    // valid link learns that it is gone for legal reasons.
    if let Some(list) = deps.blocklist.as_ref() {
        let asset = parsed
            .canonical
            .strip_prefix("/cdn/hls/")
            .and_then(|r| r.split('/').next())
            .and_then(|a| a.parse::<i64>().ok());
        let blocked = list.is_blocked_key(&parsed.key).await
            || match asset {
                Some(a) => list.is_blocked_asset(a).await,
                None => false,
            };
        if blocked {
            return plain(
                StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,
                "removed after a legal notice",
            );
        }
    }
    let params = match variant::Params::parse(query) {
        Ok(p) => p,
        Err(m) => return plain(StatusCode::BAD_REQUEST, m),
    };
    let expires_at = query
        .split('&')
        .find_map(|p| p.strip_prefix("ex=").and_then(|v| v.parse::<u64>().ok()))
        .unwrap_or(now);
    let max_age = expires_at.saturating_sub(now).min(MAX_CACHE.as_secs());
    // Hash-addressed objects never change: cache them for a year.
    let cache_control = if unsigned_public {
        "public, max-age=31536000, immutable".to_owned()
    } else {
        format!("private, max-age={max_age}")
    };

    // Playlists of an HLS asset are rendered on request, so their inner URLs carry fresh signatures.
    if parsed.kind == "hls"
        && let Some(renderer) = deps.playlists.as_ref()
        && let Some((asset, rel)) = parsed
            .canonical
            .strip_prefix("/cdn/hls/")
            .and_then(|r| r.split_once('/'))
        && let Ok(asset) = asset.parse::<i64>()
        && let Some(out) = renderer.render(asset, rel, now).await
    {
        let mut headers = HeaderMap::new();
        put(&mut headers, header::CONTENT_TYPE, out.content_type);
        put(
            &mut headers,
            header::CACHE_CONTROL,
            &format!("private, max-age={}", max_age.min(60)),
        );
        put(&mut headers, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
        put(&mut headers, header::VARY, "Origin");
        put(
            &mut headers,
            header::CONTENT_LENGTH,
            &out.body.len().to_string(),
        );
        if head_only {
            return (StatusCode::OK, headers).into_response();
        }
        return (StatusCode::OK, headers, out.body).into_response();
    }

    if !params.is_empty() || parsed.format_hint.is_some() {
        match variant::serve(
            deps.as_ref(),
            &parsed,
            params,
            req.headers(),
            head_only,
            &cache_control,
        )
        .await
        {
            variant::Outcome::Done(r) => return r,
            variant::Outcome::Original => {}
        }
    }

    // Stored files of an HLS asset live under the asset's own storage prefix (transcode outputs are kept where the
    // worker wrote them), which the renderer knows; everything else uses the key derived from the path.
    let mapped_key = if parsed.kind == "hls"
        && let Some(renderer) = deps.playlists.as_ref()
        && let Some((asset, rel)) = parsed
            .canonical
            .strip_prefix("/cdn/hls/")
            .and_then(|r| r.split_once('/'))
        && let Ok(asset) = asset.parse::<i64>()
    {
        renderer.storage_key(asset, rel).await
    } else {
        None
    };
    let key: &str = mapped_key.as_deref().unwrap_or(&parsed.key);
    let info = match deps.storage.head(key).await {
        Ok(i) => i,
        Err(StorageError::NotFound(_)) => return plain(StatusCode::NOT_FOUND, "not found"),
        Err(e) => {
            tracing::error!(error = %e, "cdn head failed");
            return plain(StatusCode::BAD_GATEWAY, "storage unavailable");
        }
    };
    let tag = etag(key, info.etag.as_deref(), info.size);

    let mut headers = HeaderMap::new();
    put(&mut headers, header::ETAG, &tag);
    put(&mut headers, header::CACHE_CONTROL, &cache_control);
    put(&mut headers, header::ACCEPT_RANGES, "bytes");
    put(&mut headers, header::VARY, "Origin");

    let req_headers = req.headers();
    if header_str(req_headers, header::IF_NONE_MATCH).is_some_and(|l| matches_etag(l, &tag)) {
        return (StatusCode::NOT_MODIFIED, headers).into_response();
    }

    // Type: extension allow-list, else a sniff of the first bytes, else download.
    let by_extension = if parsed.kind == "hls" {
        content::from_hls_extension(&parsed.filename)
    } else {
        content::from_extension(&parsed.filename)
    };
    let mut presentation = match by_extension {
        Some(p) => p,
        None if info.size == 0 => content::from_mime(""),
        None => match deps
            .storage
            .get_range(key, 0, info.size.min(SNIFF_BYTES))
            .await
        {
            Ok(b) => content::from_sniffed(&b),
            Err(StorageError::NotFound(_)) => return plain(StatusCode::NOT_FOUND, "not found"),
            Err(e) => {
                tracing::error!(error = %e, "cdn sniff read failed");
                return plain(StatusCode::BAD_GATEWAY, "storage unavailable");
            }
        },
    };
    // `dl=1` asks for a download instead of inline playback (Patron downloads).
    if query.split('&').any(|p| p == "dl=1") {
        presentation.inline = false;
    }
    put(
        &mut headers,
        header::CONTENT_TYPE,
        presentation.content_type,
    );
    put(
        &mut headers,
        header::CONTENT_DISPOSITION,
        &content::disposition(presentation, &parsed.filename),
    );
    put(&mut headers, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    put(
        &mut headers,
        header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; sandbox",
    );
    put(&mut headers, header::REFERRER_POLICY, "no-referrer");

    // Range (single `bytes` range only). If-Range with a stale validator, a
    // multi-range and a malformed header all fall back to the full object.
    let if_range_ok = header_str(req_headers, header::IF_RANGE).is_none_or(|v| v.trim() == tag);
    let wanted = header_str(req_headers, header::RANGE)
        .filter(|_| if_range_ok)
        .and_then(|v| ByteRange::parse(v).ok());
    let (status, range) = match wanted {
        None => (StatusCode::OK, 0..info.size),
        Some(r) => match r.resolve(info.size) {
            Ok(res) => {
                put(&mut headers, header::CONTENT_RANGE, &res.content_range());
                (StatusCode::PARTIAL_CONTENT, res.start..res.end)
            }
            Err(RangeError::Unsatisfiable | RangeError::Malformed) => {
                put(
                    &mut headers,
                    header::CONTENT_RANGE,
                    &format!("bytes */{}", info.size),
                );
                headers.remove(header::CONTENT_TYPE);
                headers.remove(header::CONTENT_DISPOSITION);
                return (StatusCode::RANGE_NOT_SATISFIABLE, headers).into_response();
            }
        },
    };
    put(
        &mut headers,
        header::CONTENT_LENGTH,
        &(range.end - range.start).to_string(),
    );

    if head_only || range.is_empty() {
        return (status, headers).into_response();
    }
    let whole = status == StatusCode::OK;
    let stream = match deps
        .storage
        .get_stream(key, if whole { None } else { Some(range) })
        .await
    {
        Ok(s) => s,
        Err(StorageError::NotFound(_)) => return plain(StatusCode::NOT_FOUND, "not found"),
        Err(e) => {
            tracing::error!(error = %e, "cdn stream failed");
            return plain(StatusCode::BAD_GATEWAY, "storage unavailable");
        }
    };
    let body = Body::from_stream(stream.body.map_err(std::io::Error::other));
    (status, headers, body).into_response()
}
