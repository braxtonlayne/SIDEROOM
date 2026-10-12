//! Profile and guild images: data-URI ingest, normalisation, hashing, keys.
//!
//! An avatar/banner/icon/splash arrives as `data:image/<type>;base64,<payload>`.
//! [`prepare`] checks the size, sniffs the real type (the declared one is never
//! trusted), strips metadata by re-encoding, crops or bounds it per
//! [`MediaKind`] and names it by a content hash:
//!
//! * `hash` = first 32 hex characters of `SHA-256(kind || normalised bytes)`,
//!   prefixed `a_` when animated (Discord's convention). The kind salt keeps an
//!   icon and a banner made from the same file in separate objects, so removing
//!   one never deletes the other.
//! * Users: `avatars/<user_id>/<hash>` for both avatar and banner.
//!   Guilds: `icons/<guild_id>/<hash>` for icon, banner and splash.
//! * Public URL: `/cdn/avatars/<user_id>/<hash>.<ext>` and
//!   `/cdn/icons/<guild_id>/<hash>.<ext>`, `ext` in png/jpg/webp/gif (see the
//!   `/cdn` contract, docs/05 section 5.2).

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::img::{Fit, ImgError, ImgFormat, Limits, normalize};

/// Longest decoded upload accepted.
pub const MAX_UPLOAD_BYTES: usize = 8 * 1024 * 1024;

/// What an image is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    UserAvatar,
    UserBanner,
    GuildIcon,
    GuildBanner,
    GuildSplash,
}

impl MediaKind {
    /// Square crop vs aspect-preserving bound, and the largest stored size.
    pub const fn fit(self) -> Fit {
        match self {
            Self::UserAvatar | Self::GuildIcon => Fit::Square { max: 1024 },
            Self::UserBanner | Self::GuildBanner | Self::GuildSplash => {
                Fit::Bound { w: 1920, h: 1080 }
            }
        }
    }

    const fn salt(self) -> &'static str {
        match self {
            Self::UserAvatar => "avatar",
            Self::UserBanner => "banner",
            Self::GuildIcon => "icon",
            Self::GuildBanner => "guild-banner",
            Self::GuildSplash => "splash",
        }
    }

    /// First path segment under `/cdn` and prefix in object storage.
    pub const fn namespace(self) -> &'static str {
        match self {
            Self::UserAvatar | Self::UserBanner => "avatars",
            Self::GuildIcon | Self::GuildBanner | Self::GuildSplash => "icons",
        }
    }

    /// Field name used in error trees.
    pub const fn field(self) -> &'static str {
        match self {
            Self::UserAvatar | Self::GuildIcon => "icon_or_avatar",
            Self::UserBanner | Self::GuildBanner => "banner",
            Self::GuildSplash => "splash",
        }
    }
}

/// Object key of a stored image: `<namespace>/<owner>/<hash>`.
pub fn object_key(kind: MediaKind, owner: u64, hash: &str) -> String {
    format!("{}/{owner}/{hash}", kind.namespace())
}

/// Canonical (unsigned) CDN path of a stored image with extension `ext`.
pub fn cdn_path(kind: MediaKind, owner: u64, hash: &str, ext: &str) -> String {
    format!("/cdn/{}/{owner}/{hash}.{ext}", kind.namespace())
}

/// True for a hash this module produced (so it is safe in a key).
pub fn is_valid_hash(h: &str) -> bool {
    let body = h.strip_prefix("a_").unwrap_or(h);
    body.len() == 32 && body.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Why an upload was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MediaError {
    /// Not `data:image/...;base64,...`.
    #[error("expected a base64 image data URI")]
    NotDataUri,
    /// Declared type is not png/jpeg/gif/webp.
    #[error("only PNG, JPEG, GIF and WebP images are accepted")]
    BadType,
    /// Payload is not valid base64.
    #[error("invalid base64 data")]
    BadBase64,
    /// Decoded payload over the size limit or over the pixel budget.
    #[error("image is too large")]
    TooLarge,
    /// Bytes are not the image they claim to be.
    #[error("the file is not a valid image")]
    Invalid,
}

impl From<ImgError> for MediaError {
    fn from(e: ImgError) -> Self {
        match e {
            ImgError::TooLarge => Self::TooLarge,
            ImgError::Unsupported => Self::BadType,
            ImgError::Malformed | ImgError::BadSize => Self::Invalid,
        }
    }
}

/// Decode `data:image/<png|jpeg|jpg|gif|webp>;base64,<payload>` to bytes of at
/// most `max_bytes`.
pub fn parse_data_uri(uri: &str, max_bytes: usize) -> Result<Vec<u8>, MediaError> {
    use base64::Engine;
    let rest = uri.strip_prefix("data:").ok_or(MediaError::NotDataUri)?;
    let (meta, payload) = rest.split_once(',').ok_or(MediaError::NotDataUri)?;
    let mime = meta
        .strip_suffix(";base64")
        .ok_or(MediaError::NotDataUri)?
        .to_ascii_lowercase();
    if !matches!(
        mime.as_str(),
        "image/png" | "image/jpeg" | "image/jpg" | "image/gif" | "image/webp"
    ) {
        return Err(MediaError::BadType);
    }
    // Length check first: never allocate for an oversized payload.
    if payload.len() > max_bytes.div_ceil(3) * 4 + 8 {
        return Err(MediaError::TooLarge);
    }
    let clean: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(clean.as_bytes())
        .map_err(|_| MediaError::BadBase64)?;
    if bytes.len() > max_bytes {
        return Err(MediaError::TooLarge);
    }
    Ok(bytes)
}

/// A normalised image ready to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedMedia {
    /// Content hash (also the last key segment).
    pub hash: String,
    /// Normalised bytes.
    pub bytes: Vec<u8>,
    /// Stored format.
    pub format: ImgFormat,
    pub animated: bool,
}

/// Normalise decoded upload bytes and name them.
pub fn prepare_bytes(
    kind: MediaKind,
    raw: &[u8],
    limits: &Limits,
) -> Result<PreparedMedia, MediaError> {
    let n = normalize(raw, kind.fit(), limits)?;
    let mut h = Sha256::new();
    h.update(kind.salt().as_bytes());
    h.update([0]);
    h.update(&n.bytes);
    let digest = h.finalize();
    let mut hex = String::with_capacity(34);
    if n.animated {
        hex.push_str("a_");
    }
    for b in &digest[..16] {
        hex.push_str(&format!("{b:02x}"));
    }
    Ok(PreparedMedia {
        hash: hex,
        bytes: n.bytes,
        format: n.format,
        animated: n.animated,
    })
}

/// Parse a data URI and normalise it.
pub fn prepare(kind: MediaKind, uri: &str) -> Result<PreparedMedia, MediaError> {
    let raw = parse_data_uri(uri, MAX_UPLOAD_BYTES)?;
    prepare_bytes(kind, &raw, &Limits::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn png_uri(w: u32, h: u32, mime: &str) -> String {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 200, 30, 255]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(out)
        )
    }

    #[test]
    fn prepares_square_avatar() {
        let p = prepare(MediaKind::UserAvatar, &png_uri(300, 200, "image/png")).unwrap();
        assert!(is_valid_hash(&p.hash));
        assert_eq!(p.format, ImgFormat::Png);
        assert!(!p.animated);
        let (w, h) = image::ImageReader::new(std::io::Cursor::new(&p.bytes))
            .with_guessed_format()
            .unwrap()
            .into_dimensions()
            .unwrap();
        assert_eq!((w, h), (200, 200));
        assert_eq!(
            object_key(MediaKind::UserAvatar, 7, &p.hash),
            format!("avatars/7/{}", p.hash)
        );
        assert_eq!(
            cdn_path(MediaKind::GuildIcon, 9, &p.hash, "png"),
            format!("/cdn/icons/9/{}.png", p.hash)
        );
    }

    #[test]
    fn hash_is_salted_per_kind_and_stable() {
        let u = png_uri(64, 64, "image/png");
        let a = prepare(MediaKind::GuildIcon, &u).unwrap();
        let b = prepare(MediaKind::GuildSplash, &u).unwrap();
        let a2 = prepare(MediaKind::GuildIcon, &u).unwrap();
        assert_ne!(a.hash, b.hash);
        assert_eq!(a.hash, a2.hash);
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(
            prepare(MediaKind::UserAvatar, "http://x/y.png").unwrap_err(),
            MediaError::NotDataUri
        );
        assert_eq!(
            prepare(MediaKind::UserAvatar, "data:image/svg+xml;base64,PHN2Zz4=").unwrap_err(),
            MediaError::BadType
        );
        assert_eq!(
            prepare(MediaKind::UserAvatar, "data:image/png;base64,!!!").unwrap_err(),
            MediaError::BadBase64
        );
        // Declared PNG, actually HTML.
        let html = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(b"<html><script>")
        );
        assert_eq!(
            prepare(MediaKind::UserAvatar, &html).unwrap_err(),
            MediaError::BadType
        );
        assert_eq!(
            parse_data_uri(&png_uri(64, 64, "image/png"), 10).unwrap_err(),
            MediaError::TooLarge
        );
        assert!(!is_valid_hash("../etc"));
        assert!(!is_valid_hash(""));
    }
}
