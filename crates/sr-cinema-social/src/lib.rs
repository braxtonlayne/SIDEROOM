//! sr-cinema-social: per-account Cinema state and social features (docs/specs/cinema-media.md
//! section 8): watch progress and history, "watching" presence, lists, ratings, reviews, comments,
//! trending and recommendations.
//!
//! The crate stores plain `title_id` / `episode_id` columns without foreign keys and learns about the
//! catalog only through [`catalog::CatalogLookup`], which `sr-cinema` implements.
//!
//! * [`service::CinemaService`]: the operations, over Postgres (`sqlx`), split by topic in
//!   [`progress`], [`presence`], [`lists`], [`social`] and [`discover`].
//! * [`rules`]: validation and pure logic (unit tested without a database).
//! * [`ports`]: gateway events (`SR_CINEMA_UPDATE`, `SR_ACTIVITY_UPDATE`) and the moderation hook.

pub mod catalog;
pub mod discover;
pub mod error;
pub mod hooks;
pub mod ids;
pub mod lists;
pub mod model;
pub mod ports;
pub mod presence;
pub mod progress;
pub mod rules;
pub mod service;
pub mod social;

pub use catalog::{
    CatalogEpisode, CatalogError, CatalogLookup, CatalogResult, CatalogTitle, EpisodeRef,
    MemoryCatalog, NoCatalog, TitleKind, TitleRef,
};
pub use error::{SocialError, SocialResult};
pub use hooks::{SocialObserver, row_providers};
pub use model::Viewer;
pub use ports::{
    BusEvents, CinemaEvents, ContentKind, ModerationHook, NewReport, NoEvents, NoModeration,
    Screening,
};
pub use service::{CinemaService, IdSource};
