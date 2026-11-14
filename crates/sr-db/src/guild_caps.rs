//! The two server limits (D-659, D-660, D-661), checked inside the caller's transaction.
//!
//! Each check takes a transaction-scoped advisory lock (keys `guild-cap:user:<id>` and `guild-cap:guild:<id>`),
//! reads its setting in the same transaction (no cache, so a change applies to the next request), counts, and
//! compares. The caller inserts the row in the same transaction, so parallel requests cannot pass a limit.
//!
//! Lock order everywhere: the membership lock (`lock_membership`), then the user lock, then the guild lock.
//! Tests: `tests/guild_caps.rs` (needs `SR_TEST_DATABASE_URL`).

use sqlx::{Postgres, Transaction};

pub use sr_core::guild_caps::CapHit;
use sr_core::guild_caps::{
    GUILD_MEMBERS_SETTING, GUILDS_PER_USER_SETTING, guild_members_in_force,
    guilds_per_user_in_force,
};

/// A cap check that did not pass: the limit was hit, or the database failed.
#[derive(Debug)]
pub enum CapError {
    Hit(CapHit),
    Db(sqlx::Error),
}

impl From<sqlx::Error> for CapError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e)
    }
}

type Tx<'a> = Transaction<'a, Postgres>;

async fn stored(tx: &mut Tx<'_>, key: &str) -> Result<Option<serde_json::Value>, sqlx::Error> {
    sqlx::query_scalar::<_, serde_json::Value>("SELECT value FROM platform_settings WHERE key = $1")
        .bind(key)
        .fetch_optional(&mut **tx)
        .await
}

/// The servers-per-account limit in force (an unreadable stored value gives the default).
pub async fn guilds_per_user_limit(tx: &mut Tx<'_>) -> Result<i64, sqlx::Error> {
    Ok(guilds_per_user_in_force(
        stored(tx, GUILDS_PER_USER_SETTING).await?.as_ref(),
    ))
}

/// The members-per-server limit in force (an unreadable stored value gives the default).
pub async fn guild_members_limit(tx: &mut Tx<'_>) -> Result<i64, sqlx::Error> {
    Ok(guild_members_in_force(
        stored(tx, GUILD_MEMBERS_SETTING).await?.as_ref(),
    ))
}

/// May `user` be in one more server? Bots and the system user always may (their reach is their installs).
/// Counts the account's memberships in servers that are not deleted and are not the Staff Server, pending
/// rows included.
pub async fn check_user_cap(tx: &mut Tx<'_>, user: i64) -> Result<(), CapError> {
    let exempt: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT is_bot OR (flags & 1) <> 0 FROM accounts WHERE id = $1), FALSE)",
    )
    .bind(user)
    .fetch_one(&mut **tx)
    .await?;
    if exempt {
        return Ok(());
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('guild-cap:user:' || $1::text, 0))")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    let limit = guilds_per_user_limit(tx).await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM guild_members gm JOIN guilds g ON g.id = gm.guild_id \
         WHERE gm.user_id = $1 AND g.deleted_at IS NULL AND NOT g.is_staff_guild",
    )
    .bind(user)
    .fetch_one(&mut **tx)
    .await?;
    if count >= limit {
        return Err(CapError::Hit(CapHit::UserServers(limit)));
    }
    Ok(())
}

/// May `guild` take one more member? The Staff Server always may. Counts every membership row of the
/// server (people, pending members, bots) except the system user, who is no visible member (D-188).
pub async fn check_guild_cap(tx: &mut Tx<'_>, guild: i64) -> Result<(), CapError> {
    let staff: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT is_staff_guild FROM guilds WHERE id = $1), FALSE)",
    )
    .bind(guild)
    .fetch_one(&mut **tx)
    .await?;
    if staff {
        return Ok(());
    }
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('guild-cap:guild:' || $1::text, 0))",
    )
    .bind(guild)
    .execute(&mut **tx)
    .await?;
    let limit = guild_members_limit(tx).await?;
    let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM guild_members gm WHERE gm.guild_id = $1 AND {}",
        crate::accounts::NOT_SYSTEM_MEMBER
    )))
    .bind(guild)
    .fetch_one(&mut **tx)
    .await?;
    if count >= limit {
        return Err(CapError::Hit(CapHit::GuildMembers(limit)));
    }
    Ok(())
}

/// Turn a failed cap check into a `RepoError` for repositories whose error type is `RepoError`: a limit hit
/// becomes a marker the service turns back into the typed error ([`sr_chat::error::cap_hit_from_repo`]).
pub fn repo_err(e: CapError) -> sr_chat::error::RepoError {
    match e {
        CapError::Hit(h) => sr_chat::error::cap_hit_to_repo(h),
        CapError::Db(e) => crate::chat_repo::db(e),
    }
}

/// The members-per-server limit in force, read outside a transaction (the guild object's `max_members`).
pub async fn guild_members_limit_now(pool: &sqlx::PgPool) -> Result<i64, sqlx::Error> {
    let v = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT value FROM platform_settings WHERE key = $1",
    )
    .bind(GUILD_MEMBERS_SETTING)
    .fetch_optional(pool)
    .await?;
    Ok(guild_members_in_force(v.as_ref()))
}
