//! Postgres implementation of the Cinema media ports (tables in migrations 0420..0423):
//!
//! * [`sr_media::worker_api::JobStore`]: tokens, workers, the transcode job queue with
//!   `FOR UPDATE SKIP LOCKED` claims, leases extended by progress, three attempts, requeue of
//!   expired leases, and the effects of a finished job on the asset (renditions, tracks, the
//!   published prefix) and on Patron download cache entries;
//! * [`sr_media::assets::MediaRepository`]: assets, admin views of jobs, tokens and workers,
//!   playback data, retention facts;
//! * [`sr_media::downloads::DownloadRepository`] and [`PgDownloadEntitlements`].
//!
//! Tests: `tests/media_repo.rs` (needs `SR_TEST_DATABASE_URL`).

mod assets;
mod downloads;
mod jobs;

pub use downloads::{EntitlementDownloads, PgDownloadEntitlements};

use sqlx::PgPool;
use sr_media::assets::MediaError;
use sr_media::worker_api::StoreError;
use std::sync::Arc;

pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

pub struct PgMediaRepo {
    pub(crate) pool: PgPool,
    pub(crate) ids: IdSource,
}

impl PgMediaRepo {
    pub fn new(pool: PgPool, ids: IdSource) -> Self {
        Self { pool, ids }
    }
}

pub(crate) fn be(e: sqlx::Error) -> MediaError {
    MediaError::Backend(e.to_string())
}

pub(crate) fn se(e: sqlx::Error) -> StoreError {
    StoreError::Backend(e.to_string())
}

pub(crate) fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|c| c == "23505")
}

/// `(extract(epoch FROM col) * 1000)::bigint AS name`.
pub(crate) fn ms(col: &str, name: &str) -> String {
    format!("(extract(epoch FROM {col}) * 1000)::bigint AS {name}")
}

pub(crate) fn dynq(
    sql: String,
) -> sqlx::query::Query<'static, sqlx::Postgres, <sqlx::Postgres as sqlx::Database>::Arguments> {
    sqlx::query(sqlx::AssertSqlSafe(sql))
}
