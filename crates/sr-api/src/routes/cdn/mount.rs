//! `/cdn/mount/<id>/<key>`: media that lives in a member's or a server's bucket (BYOS).
//!
//! The URL is minted and signed by the catalog services only for rows the viewer may see (the signature is the
//! credential, exactly like every other CDN kind), and this handler answers it with a `302` to a short presigned
//! URL of the bucket: the server never streams a mount's bytes. The presigned URL lives no longer than what is
//! left of the signed link, at most 15 minutes, and is never logged. A disabled or unknown mount is a 404; a
//! takedown that blocked `mount/<id>/<key>` is a 451.

use super::{CdnDeps, serve::plain};
use axum::{
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use sr_storage::{PresignMethod, StorageError};
use std::time::Duration;

/// Longest a redirect target stays valid.
const MAX_REDIRECT_TTL: u64 = 15 * 60;
const MIN_REDIRECT_TTL: u64 = 60;

pub(super) async fn redirect(deps: &CdnDeps, mount_id: i64, key: &str, left_secs: u64) -> Response {
    if let Some(list) = deps.blocklist.as_ref()
        && list.is_blocked_key(&format!("mount/{mount_id}/{key}")).await
    {
        return plain(
            StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,
            "removed after a legal notice",
        );
    }
    let Some(mounts) = deps.mounts.as_ref() else {
        return plain(StatusCode::NOT_FOUND, "not found");
    };
    let storage = match mounts.for_mount(mount_id).await {
        Ok(s) => s,
        Err(StorageError::NotFound(_)) => return plain(StatusCode::NOT_FOUND, "not found"),
        Err(e) => {
            tracing::warn!(mount = mount_id, error = %e, "cdn mount lookup failed");
            return plain(StatusCode::BAD_GATEWAY, "storage unavailable");
        }
    };
    let ttl = Duration::from_secs(left_secs.clamp(MIN_REDIRECT_TTL, MAX_REDIRECT_TTL));
    let url = match storage.presign(PresignMethod::Get, key, ttl).await {
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
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}
