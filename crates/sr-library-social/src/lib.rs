//! sr-library-social: per-account social features of the Library (docs/specs/library-reader.md
//! section 4): ratings per scope, reviews with helpful votes, reading lists, rankings and unread
//! counts. Modelled on `sr-cinema-social`: plain SQL over the shared pool, no catalog port because
//! the series tables live in the same database.
//!
//! * [`service::LibrarySocial`]: the operations, split by topic in [`ratings`], [`reviews`],
//!   [`lists`] and [`discover`].
//! * [`rules`]: validation and pure logic (unit tested without a database).

pub mod comments;
pub mod discover;
pub mod downloads;
pub mod error;
pub mod ids;
pub mod lists;
pub mod model;
pub mod notifications;
pub mod presence;
pub mod ratings;
pub mod recommend;
pub mod reviews;
pub mod rules;
pub mod service;

pub use error::{SocialError, SocialResult};
pub use model::Viewer;
pub use service::{IdSource, LibrarySocial};
