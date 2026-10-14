//! sr-cinema: the Cinema catalog (docs/specs/cinema-media.md, inventory 14, CIN-DATA).
//!
//! * [`model`]: wire and domain types.
//! * [`rules`]: validation and normalisation of publisher input, cursors.
//! * [`repo`]: the storage port; `sr-db` implements it over Postgres.
//! * [`service`]: member reads (visibility, age gate, signed URLs), publisher writes (permissions).
//! * [`search`]: the Tantivy title index behind the instant search.
//! * [`rows`]: Discover rows, generated row definitions and the [`RowProvider`] hook for rows
//!   that need user data (implemented by the social work).
//!
//! The playable assets (renditions, tracks, transcoding) and everything a viewer does (progress,
//! lists, ratings) live elsewhere and refer to titles and episodes by plain ids.

pub mod error;
pub mod model;
pub mod repo;
pub mod rows;
pub mod rules;
pub mod search;
pub mod service;
pub mod submissions;

pub use error::{CinemaError, CinemaResult};
pub use repo::CinemaRepository;
pub use rows::{CatalogObserver, ProvidedRow, RowContext, RowEntry, RowProvider};
pub use search::TitleIndex;
pub use service::{Actor, AssetAccess, CinemaService, IdSource, PlayRef, Viewer};
