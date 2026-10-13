//! Shared upload path for profile and guild images (avatar, banner, icon, splash).
//!
//! Handlers take a data URI from the request body, call [`MediaDeps::store`],
//! and persist the returned content hash on the account or guild. The original
//! (normalised, EXIF-free) is written to `avatars/<user>/<hash>` or
//! `icons/<guild>/<hash>`; `/cdn/...` derives sizes and formats from it on
//! demand (`routes::cdn`). Removing or replacing an image deletes the object
//! and every cached variant (`<key>@...`).
//!
//! Register [`MediaDeps`] on the composition root with
//! `AppStateBuilder::extension`; the routes that accept images answer 500
//! (misconfiguration) without it.

use crate::HttpError;
use axum::extract::DefaultBodyLimit;
use sr_core::ApiError;
use sr_storage::{
    ObjectStorage,
    img::Limits,
    media::{self, MediaError, MediaKind},
};
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Request body limit for the routes that take image data URIs: the largest
/// upload (8 MiB) as base64 plus JSON overhead.
pub const BODY_LIMIT: usize = 12 * 1024 * 1024;

/// Layer for routes that accept image data URIs (axum's default is 2 MiB).
pub fn body_limit() -> DefaultBodyLimit {
    DefaultBodyLimit::max(BODY_LIMIT)
}

/// Storage and processing budget for image uploads.
#[derive(Clone)]
pub struct MediaDeps {
    storage: Arc<dyn ObjectStorage>,
    limits: Limits,
    permits: Arc<Semaphore>,
}

impl MediaDeps {
    /// Uploads processed at once (CPU and memory bound).
    pub const CONCURRENCY: usize = 4;

    pub fn new(storage: Arc<dyn ObjectStorage>) -> Self {
        Self {
            storage,
            limits: Limits::default(),
            permits: Arc::new(Semaphore::new(Self::CONCURRENCY)),
        }
    }

    /// Replace the pixel/byte budget.
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Validate, normalise and store a data URI for `owner` (a user id for
    /// avatars and banners, a guild id for guild images). `field` names the
    /// request field in error trees. Returns the content hash.
    pub async fn store(
        &self,
        kind: MediaKind,
        owner: u64,
        field: &str,
        uri: String,
    ) -> Result<String, HttpError> {
        let permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| sr_core::ErrorCode::SrInternal)?;
        let limits = self.limits;
        let prepared = tokio::task::spawn_blocking(move || {
            let raw = media::parse_data_uri(&uri, media::MAX_UPLOAD_BYTES)?;
            media::prepare_bytes(kind, &raw, &limits)
        })
        .await
        .map_err(|_| sr_core::ErrorCode::SrInternal)?
        .map_err(|e| media_error(field, e))?;
        drop(permit);
        let key = media::object_key(kind, owner, &prepared.hash);
        self.storage
            .put(&key, prepared.bytes.into())
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "storing an image failed");
                HttpError::from(sr_core::ErrorCode::SrInternal)
            })?;
        Ok(prepared.hash)
    }

    /// Delete the stored object and its cached variants. Best effort: a
    /// failure only leaves an orphan, which is logged.
    pub async fn remove(&self, kind: MediaKind, owner: u64, hash: &str) {
        if !media::is_valid_hash(hash) {
            // Not one of ours (legacy value); there is nothing safe to delete.
            return;
        }
        let key = media::object_key(kind, owner, hash);
        // `list` works on whole path segments, so list the owner "directory" and
        // match the object itself plus its `<key>@<size>.<ext>` variants.
        let dir = format!("{}/{owner}", kind.namespace());
        match self.storage.list(&dir).await {
            Ok(objects) => {
                for o in objects {
                    if (o.key == key || o.key.starts_with(&format!("{key}@")))
                        && let Err(e) = self.storage.delete(&o.key).await
                    {
                        tracing::warn!(error = %e, key = %o.key, "deleting an image failed");
                    }
                }
            }
            Err(e) => tracing::warn!(error = %e, %key, "listing image variants failed"),
        }
    }
}

fn invalid(field: &str, code: &str, message: &str) -> HttpError {
    HttpError(ApiError::invalid_form().field(field, code, message))
}

fn media_error(field: &str, e: MediaError) -> HttpError {
    match e {
        MediaError::TooLarge => invalid(
            field,
            "BINARY_TYPE_MAX_SIZE",
            "File cannot be larger than 8 MiB, or have more than 24 million pixels.",
        ),
        MediaError::NotDataUri | MediaError::BadBase64 => {
            invalid(field, "IMAGE_INVALID", "Expected a base64 image data URI.")
        }
        MediaError::BadType | MediaError::Invalid => invalid(
            field,
            "IMAGE_INVALID",
            "Must be a valid PNG, JPEG, GIF or WebP image.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_storage::ObjectStoreStorage;

    fn uri() -> String {
        let img = image::RgbaImage::from_pixel(64, 48, image::Rgba([1, 2, 3, 255]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        format!(
            "data:image/png;base64,{}",
            data_encoding::BASE64.encode(&out)
        )
    }

    #[tokio::test]
    async fn store_then_remove_deletes_variants_too() {
        let storage = Arc::new(ObjectStoreStorage::in_memory());
        let m = MediaDeps::new(storage.clone());
        let hash = m
            .store(MediaKind::UserAvatar, 7, "avatar", uri())
            .await
            .unwrap();
        let key = format!("avatars/7/{hash}");
        assert!(storage.head(&key).await.is_ok());
        storage
            .put(&format!("{key}@64.webp"), bytes::Bytes::from_static(b"v"))
            .await
            .unwrap();
        // A neighbour with a shared prefix must survive.
        storage
            .put(&format!("{key}0"), bytes::Bytes::from_static(b"n"))
            .await
            .unwrap();
        m.remove(MediaKind::UserAvatar, 7, &hash).await;
        assert!(storage.head(&key).await.is_err());
        assert!(storage.head(&format!("{key}@64.webp")).await.is_err());
        assert!(storage.head(&format!("{key}0")).await.is_ok());
        // A non-hash value is never turned into a delete.
        m.remove(MediaKind::UserAvatar, 7, "../x").await;
    }

    #[tokio::test]
    async fn bad_input_is_a_form_error() {
        let m = MediaDeps::new(Arc::new(ObjectStoreStorage::in_memory()));
        for bad in [
            "banners/a.png".to_owned(),
            "data:text/html;base64,PGh0bWw+".to_owned(),
            "data:image/png;base64,PGh0bWw+".to_owned(),
        ] {
            let e = m
                .store(MediaKind::UserAvatar, 1, "avatar", bad)
                .await
                .unwrap_err();
            assert_eq!(e.0.status(), 400);
        }
    }
}
