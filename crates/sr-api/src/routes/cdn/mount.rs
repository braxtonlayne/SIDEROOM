//! `/cdn/mount/<id>/<key>`: media that lives in a member's or a server's bucket (BYOS).
//!
//! The URL is minted and signed by the catalog services only for rows the viewer may see (the signature is the
//! credential, exactly like every other CDN kind), and this handler answers it with a `302` to a short presigned
//! URL of the bucket: the server never streams a mount's bytes. The presigned URL lives no longer than what is
//! left of the signed link, at most 15 minutes, and is never logged. A disabled or unknown mount is a 404; a
//! takedown that blocked `mount/<id>/<key>` is a 451.

use super::{CdnDeps, content, serve::plain};
use axum::{
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use sr_storage::{ObjectStorage, ResponseAs, StorageError};
use std::time::Duration;

/// Longest a redirect target stays valid.
const MAX_REDIRECT_TTL: u64 = 15 * 60;
const MIN_REDIRECT_TTL: u64 = 60;

pub(super) async fn redirect(deps: &CdnDeps, mount_id: i64, key: &str, left_secs: u64) -> Response {
    if let Some(list) = deps.blocklist.as_ref()
        && list
            .is_blocked_key(&format!("mount/{mount_id}/{key}"))
            .await
    {
        return plain(
            StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,
            "removed after a legal notice",
        );
    }
    let Some(mounts) = deps.mounts.as_ref() else {
        return plain(StatusCode::NOT_FOUND, "not found");
    };
    let storage = match mounts.for_serving(mount_id).await {
        Ok(s) => s,
        Err(StorageError::NotFound(_)) => return plain(StatusCode::NOT_FOUND, "not found"),
        Err(e) => {
            tracing::warn!(mount = mount_id, error = %e, "cdn mount lookup failed");
            return plain(StatusCode::BAD_GATEWAY, "storage unavailable");
        }
    };
    let ttl = Duration::from_secs(left_secs.clamp(MIN_REDIRECT_TTL, MAX_REDIRECT_TTL));
    presigned_redirect(storage.as_ref(), mount_id, key, ttl).await
}

/// What the bucket must answer a link to `key` with (D-637): an extension on the app's inline media list keeps
/// its own type and `inline`; everything else (HTML, SVG, XML, scripts, unknown or no extension) is a download
/// as `application/octet-stream`, whatever the object's stored type says.
fn response_for(key: &str) -> (&'static str, String) {
    let name = key.rsplit('/').next().unwrap_or(key);
    let p = content::from_hls_extension(name).unwrap_or(content::DOWNLOAD);
    (p.content_type, content::disposition(p, name))
}

/// The redirect to a presigned URL that carries [`response_for`]. Without the overrides there is no redirect:
/// a storage that cannot sign them answers 502 like any other storage error. The URL is never logged.
pub(super) async fn presigned_redirect(
    storage: &dyn ObjectStorage,
    mount_id: i64,
    key: &str,
    ttl: Duration,
) -> Response {
    let (content_type, disposition) = response_for(key);
    let response = ResponseAs {
        content_type,
        disposition: &disposition,
    };
    let url = match storage.presign_get_as(key, ttl, &response).await {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!(mount = mount_id, error = %e, "cdn mount presign failed");
            return plain(StatusCode::BAD_GATEWAY, "storage unavailable");
        }
    };
    let Ok(location) = HeaderValue::from_str(&url) else {
        return plain(StatusCode::BAD_GATEWAY, "storage unavailable");
    };
    let mut r = StatusCode::FOUND.into_response();
    let h = r.headers_mut();
    h.insert(header::LOCATION, location);
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}
