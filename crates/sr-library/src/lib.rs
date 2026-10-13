//! sr-library: the Library catalog (docs/specs/library-reader.md, inventory 13).
//!
//! * [`model`]: wire and domain types.
//! * [`rules`]: validation and normalisation of publisher input.
//! * [`repo`]: the storage port; `sr-db` implements it over Postgres.
//! * [`service`]: member reads (visibility, age gate, signed URLs) and publisher writes (permissions).
//! * [`search`]: the Tantivy series index behind the instant search.

pub mod error;
pub mod ingest;
pub mod model;
pub mod repo;
pub mod rules;
pub mod search;
pub mod service;

pub use error::{LibraryError, LibraryResult};
pub use repo::LibraryRepository;
pub use search::SeriesIndex;
pub use service::{Actor, IdSource, LibraryService, Viewer};
