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

/// A picture stored by [`MediaDeps::store_raw`].
#[derive(Debug, Clone)]
pub struct StoredImage {
    pub hash: String,
    /// Object key (`avatars/<user>/<hash>`).
    pub key: String,
    pub content_type: &'static str,
    pub width: u32,
    pub height: u32,
    pub bytes: i64,
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
        self.store_scoped(kind, owner, None, field, uri).await
    }

    /// [`Self::store`] with a hash scope (see [`media::prepare_bytes_scoped`]): per-server
    /// member images pass the guild id.
    pub async fn store_scoped(
        &self,
        kind: MediaKind,
        owner: u64,
        scope: Option<u64>,
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
            let raw = media::parse_data_uri(&uri, kind.max_upload())?;
            let prepared = media::prepare_bytes_scoped(kind, &raw, &limits, scope)?;
            if !kind.accepts_webp()
                && (prepared.format == sr_storage::img::ImgFormat::Webp
                    || uri
                        .get(..15)
                        .is_some_and(|p| p.eq_ignore_ascii_case("data:image/webp")))
            {
                return Err(MediaError::BadType);
            }
            Ok::<_, MediaError>(prepared)
        })
        .await
        .map_err(|_| sr_core::ErrorCode::SrInternal)?
        .map_err(|e| media_error(field, e, kind))?;
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

    /// Validate, normalise and store raw image bytes (a request body, not a data URI) for `owner`,
    /// with the hash scoped to `scope` so two rows with the same picture never share a file. The same
    /// pipeline as [`Self::store_scoped`]: the real type is sniffed, metadata is stripped by re-encoding,
    /// SVG and other active content are refused, and the size and animation caps of the kind apply.
    pub async fn store_raw(
        &self,
        kind: MediaKind,
        owner: u64,
        scope: u64,
        field: &str,
        raw: Vec<u8>,
    ) -> Result<StoredImage, HttpError> {
        let permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| sr_core::ErrorCode::SrInternal)?;
        let limits = self.limits;
        let prepared = tokio::task::spawn_blocking(move || {
            if raw.len() > kind.max_upload() {
                return Err(MediaError::TooLarge);
            }
            media::prepare_bytes_scoped(kind, &raw, &limits, Some(scope))
        })
        .await
        .map_err(|_| sr_core::ErrorCode::SrInternal)?
        .map_err(|e| media_error(field, e, kind))?;
        drop(permit);
        let key = media::object_key(kind, owner, &prepared.hash);
        let bytes = i64::try_from(prepared.bytes.len()).unwrap_or(i64::MAX);
        self.storage
            .put(&key, prepared.bytes.into())
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "storing an image failed");
                HttpError::from(sr_core::ErrorCode::SrInternal)
            })?;
        Ok(StoredImage {
            hash: prepared.hash,
            key,
            content_type: prepared.format.mime(),
            width: prepared.width,
            height: prepared.height,
            bytes,
        })
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

fn media_error(field: &str, e: MediaError, kind: MediaKind) -> HttpError {
    match e {
        MediaError::TooLarge => invalid(
            field,
            "BINARY_TYPE_MAX_SIZE",
            if kind == MediaKind::EventCover {
                "File cannot be larger than 2 MiB, or have more than 24 million pixels."
            } else {
                "File cannot be larger than 8 MiB, or have more than 24 million pixels."
            },
        ),
        MediaError::NotDataUri | MediaError::BadBase64 => {
            invalid(field, "IMAGE_INVALID", "Expected a base64 image data URI.")
        }
        MediaError::BadType | MediaError::Invalid => invalid(
            field,
            "IMAGE_INVALID",
            if kind.accepts_webp() {
                "Must be a valid PNG, JPEG, GIF or WebP image."
            } else {
                "Must be a valid PNG, JPEG or GIF image."
            },
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

    fn png_bytes() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(64, 48, image::Rgba([1, 2, 3, 255]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    #[tokio::test]
    async fn raw_profile_pictures_go_through_the_banner_pipeline() {
        let storage = Arc::new(ObjectStoreStorage::in_memory());
        let m = MediaDeps::new(storage.clone());
        let a = m
            .store_raw(MediaKind::ProfileMedia, 7, 1, "image", png_bytes())
            .await
            .unwrap();
        assert_eq!((a.width, a.height, a.content_type), (64, 48, "image/png"));
        assert_eq!(a.key, format!("avatars/7/{}", a.hash));
        assert!(storage.head(&a.key).await.is_ok());
        // The same picture under another row id is another object, so deleting one keeps the other.
        let b = m
            .store_raw(MediaKind::ProfileMedia, 7, 2, "image", png_bytes())
            .await
            .unwrap();
        assert_ne!(a.hash, b.hash);
        // Not an image, an SVG, and an over-large body are refused with a form error.
        for bad in [
            b"just some text".to_vec(),
            b"<svg xmlns='http://www.w3.org/2000/svg'><script>1</script></svg>".to_vec(),
            vec![0u8; MediaKind::ProfileMedia.max_upload() + 1],
        ] {
            let e = m
                .store_raw(MediaKind::ProfileMedia, 7, 3, "image", bad)
                .await
                .unwrap_err();
            assert_eq!(e.0.status(), 400);
        }
    }
}
