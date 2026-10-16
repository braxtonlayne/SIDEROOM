//! Image variants for `/cdn` (FND-STOR-003): `?size=` and `format=`.
//!
//! * `size`: a power of two in 16..=4096, the longest edge in pixels. A source
//!   smaller than `size` is returned at its own size (never upscaled).
//! * `format`: `png`, `jpg`/`jpeg`, `webp` or `gif`. Without it the extension
//!   of a public-kind URL (`.../<hash>.webp`) decides; without that, the
//!   output follows the source (JPEG stays JPEG, GIF stays GIF, else WebP when
//!   `Accept` lists `image/webp`, else PNG) and the response carries
//!   `Vary: Accept`.
//! * Variants are computed once and stored next to the source under
//!   `<key>@<size>.<ext>` (`<key>@orig.<ext>` for a format-only conversion).
//! * Work is bounded: sources over the byte/pixel budget give 413, rendering
//!   runs on the blocking pool behind a small semaphore.
//!
//! WebP output is lossless (pure-Rust encoder) and animated sources keep
//! their animation only as GIF; see `sr_storage::img` for the details.

use super::{CdnDeps, path::CdnPath};
use axum::{
    body::Body,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use sha2::{Digest, Sha256};
use sr_storage::{
    StorageError,
    img::{self, ImgError, ImgFormat},
};

/// Parsed `size` / `format` query parameters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Params {
    pub size: Option<u32>,
    pub format: Option<ImgFormat>,
}

impl Params {
    /// `Err` carries the 400 message.
    pub fn parse(query: &str) -> Result<Self, &'static str> {
        let mut out = Self::default();
        for pair in query.split('&') {
            match pair.split_once('=') {
                Some(("size", v)) => {
                    let s: u32 = v.parse().map_err(|_| "size must be a number")?;
                    if !img::valid_size(s) {
                        return Err("size must be a power of two between 16 and 4096");
                    }
                    out.size = Some(s);
                }
                Some(("format", v)) => {
                    out.format = Some(if v.eq_ignore_ascii_case("avif") {
                        // AVIF is closed (P13-FND-2): answer the WebP variant.
                        ImgFormat::Webp
                    } else {
                        ImgFormat::from_name(v)
                            .ok_or("format must be png, jpg, webp, gif or avif")?
                    });
                }
                _ => {}
            }
        }
        Ok(out)
    }

    pub fn is_empty(&self) -> bool {
        self.size.is_none() && self.format.is_none()
    }
}

pub(super) enum Outcome {
    Done(Response),
    /// Serve the stored object unchanged.
    Original,
}

fn plain(status: StatusCode, msg: &'static str) -> Response {
    let mut r = (status, msg).into_response();
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}

fn accepts_webp(h: &HeaderMap) -> bool {
    h.get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(',')
                .any(|t| t.split(';').next().map(str::trim) == Some("image/webp"))
        })
}

fn storage_err(e: &StorageError) -> Response {
    match e {
        StorageError::NotFound(_) => plain(StatusCode::NOT_FOUND, "not found"),
        _ => {
            tracing::error!(error = %e, "cdn variant storage error");
            plain(StatusCode::BAD_GATEWAY, "storage unavailable")
        }
    }
}

fn img_err(e: ImgError) -> Response {
    match e {
        ImgError::TooLarge => plain(StatusCode::PAYLOAD_TOO_LARGE, "image too large to resize"),
        ImgError::BadSize => plain(StatusCode::BAD_REQUEST, "bad size"),
        ImgError::Unsupported | ImgError::Malformed => {
            plain(StatusCode::UNSUPPORTED_MEDIA_TYPE, "not a resizable image")
        }
    }
}

/// ETag of a variant: derived from its key and the source's tag, so it
/// changes when the source object does and is known before rendering.
fn variant_etag(vkey: &str, backend: Option<&str>, size: u64) -> String {
    let mut h = Sha256::new();
    h.update(vkey.as_bytes());
    h.update([0]);
    match backend {
        Some(t) => h.update(t.as_bytes()),
        None => h.update(size.to_string().as_bytes()),
    }
    format!(
        "\"{}\"",
        data_encoding::HEXLOWER.encode(&h.finalize()[..16])
    )
}

/// Serve a variant of `parsed.key`. `cache_control` is the header value to
/// send. Range requests are ignored (variants are small and whole).
pub(super) async fn serve(
    deps: &CdnDeps,
    parsed: &CdnPath,
    params: Params,
    req_headers: &HeaderMap,
    head_only: bool,
    cache_control: &str,
) -> Outcome {
    let info = match deps.storage.head(&parsed.key).await {
        Ok(i) => i,
        Err(e) => return Outcome::Done(storage_err(&e)),
    };
    if info.size > deps.limits.max_bytes {
        return Outcome::Done(img_err(ImgError::TooLarge));
    }
    // Cheap format decision from the magic bytes alone, so cache hits and 304s
    // never read the whole source.
    let head = match deps
        .storage
        .get_range(&parsed.key, 0, info.size.min(32))
        .await
    {
        Ok(b) => b,
        Err(e) => return Outcome::Done(storage_err(&e)),
    };
    let Some(source) = ImgFormat::sniff(&head) else {
        return Outcome::Done(img_err(ImgError::Unsupported));
    };
    let requested = params.format.or(parsed.format_hint);
    if params.size.is_none() && requested.is_none_or(|f| f == source) {
        // Nothing to change: the stored object is already normalised.
        return Outcome::Original;
    }
    let auto = requested.is_none();
    let fmt = img::pick_format(
        requested,
        source,
        source == ImgFormat::Gif || img::webp_header_animated(&head),
        accepts_webp(req_headers),
    );
    let vkey = img::variant_key(&parsed.key, params.size, fmt);
    let tag = variant_etag(&vkey, info.etag.as_deref(), info.size);

    let mut headers = HeaderMap::new();
    let mut put = |n: header::HeaderName, v: &str| {
        if let Ok(v) = HeaderValue::from_str(v) {
            headers.insert(n, v);
        }
    };
    put(header::ETAG, &tag);
    put(header::CACHE_CONTROL, cache_control);
    put(header::CONTENT_TYPE, fmt.mime());
    put(header::CONTENT_DISPOSITION, "inline");
    put(header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    put(
        header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; sandbox",
    );
    put(header::REFERRER_POLICY, "no-referrer");
    put(header::VARY, if auto { "Origin, Accept" } else { "Origin" });

    let not_modified = req_headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|l| {
            l.split(',')
                .any(|t| t.trim() == "*" || t.trim().trim_start_matches("W/") == tag)
        });
    if not_modified {
        headers.remove(header::CONTENT_TYPE);
        headers.remove(header::CONTENT_DISPOSITION);
        return Outcome::Done((StatusCode::NOT_MODIFIED, headers).into_response());
    }

    // Cached variant?
    let body: Bytes = match deps.storage.get(&vkey).await {
        Ok(b) => b,
        Err(StorageError::NotFound(_)) => {
            match render_and_cache(deps, &parsed.key, &vkey, params.size, fmt).await {
                Ok(b) => b,
                Err(r) => return Outcome::Done(*r),
            }
        }
        Err(e) => return Outcome::Done(storage_err(&e)),
    };
    if let Ok(v) = HeaderValue::from_str(&body.len().to_string()) {
        headers.insert(header::CONTENT_LENGTH, v);
    }
    if head_only {
        return Outcome::Done((StatusCode::OK, headers).into_response());
    }
    Outcome::Done((StatusCode::OK, headers, Body::from(body)).into_response())
}

async fn render_and_cache(
    deps: &CdnDeps,
    key: &str,
    vkey: &str,
    size: Option<u32>,
    fmt: ImgFormat,
) -> Result<Bytes, Box<Response>> {
    let src = deps
        .storage
        .get(key)
        .await
        .map_err(|e| Box::new(storage_err(&e)))?;
    let _permit = deps
        .render_permits
        .acquire()
        .await
        .map_err(|_| Box::new(plain(StatusCode::SERVICE_UNAVAILABLE, "busy")))?;
    let limits = deps.limits;
    let rendered = tokio::task::spawn_blocking(move || img::render(&src, size, fmt, &limits))
        .await
        .map_err(|_| Box::new(plain(StatusCode::INTERNAL_SERVER_ERROR, "render failed")))?
        .map_err(|e| Box::new(img_err(e)))?;
    let bytes = Bytes::from(rendered.bytes);
    // Best effort: a failed write only means the next request renders again.
    if let Err(e) = deps.storage.put(vkey, bytes.clone()).await {
        tracing::warn!(error = %e, "could not cache image variant");
    }
    Ok(bytes)
}
