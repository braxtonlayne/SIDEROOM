//! sr-packages: declarative packages (docs/11 section 4, EXT-PKG) and customised profiles (EXT-PRF).
//! Data only, never code (D-063). This crate holds
//!
//! * [`zipfiles`]: the safe reader for the uploaded zip;
//! * [`validate`]: one validator for every package kind (themes through `sr-theme`);
//! * [`profile_doc`]: the profile document schema and its validator;
//! * [`service`]: packages, versions, review state, enablements, profile documents and the guestbook
//!   over Postgres and object storage.
//!
//! The HTTP layer is `sr_api::routes::packages`.

pub mod error;
pub mod profile_doc;
pub mod profile_privacy;
pub mod service;
pub mod validate;
pub mod zipfiles;

pub use error::{PackageError, PackageResult};
pub use validate::{Checks, PackageKind, Validated};
