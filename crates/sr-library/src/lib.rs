//! sr-library: the Library catalog (docs/specs/library-reader.md, inventory 13).
//!
//! * [`model`]: wire and domain types.
//! * [`rules`]: validation and normalisation of publisher input.
//! * [`repo`]: the storage port; `sr-db` implements it over Postgres.
//! * [`service`]: member reads (visibility, age gate, signed URLs) and publisher writes (permissions).
//! * [`search`]: the Tantivy series index behind the instant search.

pub mod duplicates;
pub mod error;
pub mod exports;
pub mod ingest;
pub mod model;
pub mod multipart;
pub mod reading;
mod reading_service;
pub mod repo;
pub mod rules;
pub mod search;
pub mod service;
pub mod sources;
pub mod submissions;
pub mod uploads;
mod uploads_multipart;

pub use duplicates::CoverHashes;
pub use error::{LibraryError, LibraryResult};
pub use multipart::{MemoryPartStore, MultipartWiring, PartStore};
pub use reading::{LibraryEvents, ReadingStateRepository};
pub use repo::LibraryRepository;
pub use search::SeriesIndex;
pub use service::{Actor, IdSource, LibraryService, Viewer};
pub use uploads::{
    GcReport, IngestQueue, IngestSettings, IngestSettingsSnapshot, LibraryAudit,
    StaticIngestSettings, UploadRepository,
};
