//! sr-db: Postgres access, migrations, job queue, transaction helpers.
//! See docs/02-architecture.md section 3 and docs/04-data-model.md.
//!
//! Queries are runtime-checked (`sqlx::query`), not compile-time macros, so the
//! crate builds without a live database. DB-backed tests are `#[ignore]` and read
//! `SR_TEST_DATABASE_URL`.

pub mod accounts;
pub mod admin_repo;
pub mod auth_repo;
pub mod auth_store;
pub mod chat_repo;
mod error;
mod guild_admin_repo;
pub mod invite_join;
pub mod jobs;
pub mod library_repo;
pub mod library_state_repo;
pub mod lock;
pub mod partitions;
mod pool;
pub mod registration;
pub mod review_repo;
pub mod rich_repo;
pub mod runner;
pub mod settings_repo;
pub mod social_repo;
pub mod thread_repo;
mod tx;

pub use error::{DbError, DbResult};
pub use pool::{DbConfig, Health, connect, health_check, migrate};
pub use tx::{BoxFuture, with_tx};

pub use sqlx::{PgPool, Postgres, Transaction};
