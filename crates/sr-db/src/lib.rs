//! sr-db: Postgres access, migrations, job queue, transaction helpers.
//! See docs/02-architecture.md section 3 and docs/04-data-model.md.
//!
//! Queries are runtime-checked (`sqlx::query`), not compile-time macros, so the
//! crate builds without a live database. DB-backed tests are `#[ignore]` and read
//! `SR_TEST_DATABASE_URL`.

pub mod account_deletion;
pub mod accounts;
pub mod admin_repo;
mod admin_repo_ext;
pub mod apps_repo;
pub mod auth_repo;
pub mod auth_store;
pub mod chat_repo;
pub mod cinema_repo;
pub mod data_export;
pub mod directory_repo;
mod error;
mod guild_admin_repo;
pub mod invite_join;
pub mod jobs;
pub mod library_repo;
pub mod library_state_repo;
pub mod lock;
pub mod media_repo;
pub mod member_invites;
pub mod partitions;
pub mod payments_repo;
mod pool;
pub mod registration;
pub mod review_repo;
pub mod rich_repo;
pub mod runner;
pub mod sealed;
pub mod settings_repo;
pub mod social_repo;
pub mod testing;
pub mod thread_repo;
mod tx;

pub use error::{DbError, DbResult};
pub use pool::{DbConfig, Health, connect, health_check, migrate};
pub use tx::{BoxFuture, with_tx};

pub use sqlx::{PgPool, Postgres, Transaction};
