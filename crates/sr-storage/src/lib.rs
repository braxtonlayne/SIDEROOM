//! sr-storage: object storage abstraction, signed CDN URLs and upload hygiene.
//! See docs/02-architecture.md section 3 and ADR-0002.
//!
//! - [`storage`]: the [`ObjectStorage`] trait over `object_store` (S3/Garage or in-memory).
//! - [`cdn`]: framework-agnostic HMAC signed URL scheme and `Range` parsing.
//! - [`upload`]: size-limit policy, content-type sniffing and EXIF/metadata stripping.

pub mod browse;
pub mod cdn;
pub mod cors_check;
pub mod error;
pub mod guard;
pub mod img;
pub mod media;
pub mod mount_cache;
pub mod mount_policy;
pub mod mount_storage;
pub mod mounts;
pub mod path;
pub mod probe;
pub mod recheck;
pub mod s3admin;
pub mod s3multipart;
pub mod scopes;
pub mod storage;
pub mod upload;
pub mod uploads;

pub use error::{Result, StorageError};
pub use storage::{
    MultipartSession, ObjectInfo, ObjectStorage, ObjectStoreStorage, ObjectStream, PresignMethod,
};
