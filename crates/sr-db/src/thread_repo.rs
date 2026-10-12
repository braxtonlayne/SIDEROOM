//! Postgres implementation of [`ThreadRepository`] (docs/04 `thread_metadata`,
//! `thread_members`). Tests live in `tests/thread_repo.rs` and need
//! `SR_TEST_DATABASE_URL`.

use async_trait::async_trait;
use sqlx::Row;
use sqlx::postgres::PgRow;

use crate::chat_repo::{
    CHANNEL_SELECT, INSERT_CHANNEL, PgChatRepo, bind_channel, channel_from, db, i, u,
};
use sr_chat::domain::Channel;
use sr_chat::repo::RepoResult;
use sr_chat::threads::model::ThreadMember;
use sr_chat::threads::repo::{ActivityOutcome, ArchivedFilter, ThreadRepository};
use sr_core::{ChannelId, GuildId, UserId};

const MEMBER_COLS: &str = "channel_id, user_id, flags, \
    (extract(epoch FROM joined_at) * 1000)::bigint AS joined_ms";

fn member_from(r: &PgRow) -> RepoResult<ThreadMember> {
    Ok(ThreadMember {
        thread_id: ChannelId::new(u(r.try_get("channel_id").map_err(db)?)),
        user_id: UserId::new(u(r.try_get("user_id").map_err(db)?)),
        joined_at_ms: r.try_get("joined_ms").map_err(db)?,
        flags: u32::try_from(r.try_get::<i32, _>("flags").map_err(db)?).unwrap_or(0),
    })
}

/// Members per thread; mirrors `sr_chat::threads::model::MAX_THREAD_MEMBERS`.
const MAX_MEMBERS: i64 = 1000;

#[async_trait]
impl ThreadRepository for PgChatRepo {
    async fn insert_thread(
        &self,
        thread: &Channel,
        creator: UserId,
        now_ms: i64,
    ) -> RepoResult<()> {
        let info = thread
            .thread
            .as_ref()
            .ok_or_else(|| sr_chat::error::RepoError("thread without metadata".into()))?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        bind_channel(sqlx::query(INSERT_CHANNEL), thread)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query(
            "INSERT INTO thread_metadata (channel_id, archived, auto_archive_duration, \
             archive_timestamp, locked, invitable, create_timestamp, last_activity_at, \
             applied_tags) \
             VALUES ($1, $2, $3, to_timestamp($4::double precision / 1000.0), $5, $6, \
             to_timestamp($7::double precision / 1000.0), \
             to_timestamp($8::double precision / 1000.0), $9)",
        )
        .bind(i(thread.id.get()))
        .bind(info.archived)
        .bind(i32::try_from(info.auto_archive_duration).unwrap_or(1440))
        .bind(info.archive_timestamp_ms)
        .bind(info.locked)
        .bind(info.invitable)
        .bind(info.create_timestamp_ms)
        .bind(info.last_activity_ms)
        .bind(info.applied_tags.iter().map(|t| i(*t)).collect::<Vec<_>>())
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query(
            "INSERT INTO thread_members (channel_id, user_id, joined_at) \
             VALUES ($1, $2, to_timestamp($3::double precision / 1000.0))",
        )
        .bind(i(thread.id.get()))
        .bind(i(creator.get()))
        .bind(now_ms)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn update_thread(&self, thread: &Channel) -> RepoResult<()> {
        let Some(info) = thread.thread.as_ref() else {
            return Ok(());
        };
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "UPDATE channels SET name = $2, rate_limit_per_user = $3, flags = $4, \
             updated_at = now() WHERE id = $1",
        )
        .bind(i(thread.id.get()))
        .bind(thread.name.as_deref())
        .bind(i32::try_from(thread.rate_limit_per_user).unwrap_or(0))
        .bind(i32::try_from(thread.flags).unwrap_or(0))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query(
            "UPDATE thread_metadata SET archived = $2, auto_archive_duration = $3, \
             archive_timestamp = to_timestamp($4::double precision / 1000.0), locked = $5, \
             invitable = $6, last_activity_at = to_timestamp($7::double precision / 1000.0), \
             applied_tags = $8 WHERE channel_id = $1",
        )
        .bind(i(thread.id.get()))
        .bind(info.archived)
        .bind(i32::try_from(info.auto_archive_duration).unwrap_or(1440))
        .bind(info.archive_timestamp_ms)
        .bind(info.locked)
        .bind(info.invitable)
        .bind(info.last_activity_ms)
        .bind(info.applied_tags.iter().map(|t| i(*t)).collect::<Vec<_>>())
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn add_thread_member(
        &self,
        thread: ChannelId,
        user: UserId,
        now_ms: i64,
    ) -> RepoResult<bool> {
        let res = sqlx::query(
            "INSERT INTO thread_members (channel_id, user_id, joined_at) \
             VALUES ($1, $2, to_timestamp($3::double precision / 1000.0)) \
             ON CONFLICT DO NOTHING",
        )
        .bind(i(thread.get()))
        .bind(i(user.get()))
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(res.rows_affected() > 0)
    }

    async fn remove_thread_member(&self, thread: ChannelId, user: UserId) -> RepoResult<bool> {
        let res = sqlx::query("DELETE FROM thread_members WHERE channel_id = $1 AND user_id = $2")
            .bind(i(thread.get()))
            .bind(i(user.get()))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(res.rows_affected() > 0)
    }

    async fn get_thread_member(
        &self,
        thread: ChannelId,
        user: UserId,
    ) -> RepoResult<Option<ThreadMember>> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {MEMBER_COLS} FROM thread_members WHERE channel_id = $1 AND user_id = $2"
        )))
        .bind(i(thread.get()))
        .bind(i(user.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(member_from).transpose()
    }

    async fn list_thread_members(
        &self,
        thread: ChannelId,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<ThreadMember>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {MEMBER_COLS} FROM thread_members WHERE channel_id = $1 AND user_id > $2 \
             ORDER BY user_id LIMIT $3"
        )))
        .bind(i(thread.get()))
        .bind(after.map_or(0, |a| i(a.get())))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(member_from).collect()
    }

    async fn thread_memberships(
        &self,
        user: UserId,
        threads: &[ChannelId],
    ) -> RepoResult<Vec<ThreadMember>> {
        if threads.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {MEMBER_COLS} FROM thread_members WHERE user_id = $1 AND channel_id = ANY($2) \
             ORDER BY channel_id"
        )))
        .bind(i(user.get()))
        .bind(threads.iter().map(|t| i(t.get())).collect::<Vec<_>>())
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(member_from).collect()
    }

    async fn list_active_threads(&self, guild: GuildId) -> RepoResult<Vec<Channel>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{CHANNEL_SELECT} WHERE c.guild_id = $1 AND tm.archived = false ORDER BY c.id DESC"
        )))
        .bind(i(guild.get()))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(channel_from).collect()
    }

    async fn list_archived_threads(
        &self,
        parent: ChannelId,
        filter: ArchivedFilter,
        before_ms: Option<i64>,
        limit: usize,
    ) -> RepoResult<Vec<Channel>> {
        let kind_cond = match filter {
            ArchivedFilter::Public => "c.type <> 12",
            ArchivedFilter::Private => "c.type = 12",
            ArchivedFilter::JoinedPrivate(_) => {
                "c.type = 12 AND EXISTS (SELECT 1 FROM thread_members m \
                 WHERE m.channel_id = c.id AND m.user_id = $4)"
            }
        };
        let user = match filter {
            ArchivedFilter::JoinedPrivate(u) => i(u.get()),
            _ => 0,
        };
        // Compare in the millisecond domain the cursor is expressed in.
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{CHANNEL_SELECT} WHERE c.parent_id = $1 AND tm.archived AND {kind_cond} \
             AND ($2::bigint IS NULL \
                  OR (extract(epoch FROM tm.archive_timestamp) * 1000)::bigint < $2) \
             ORDER BY tm.archive_timestamp DESC, c.id DESC LIMIT $3"
        )))
        .bind(i(parent.get()))
        .bind(before_ms)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .bind(user)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(channel_from).collect()
    }

    async fn record_thread_activity(
        &self,
        thread: ChannelId,
        author: UserId,
        now_ms: i64,
    ) -> RepoResult<Option<ActivityOutcome>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let prev: Option<bool> = sqlx::query_scalar(
            "WITH prev AS (SELECT channel_id, archived FROM thread_metadata \
                           WHERE channel_id = $1 FOR UPDATE) \
             UPDATE thread_metadata tm SET archived = false, \
                 archive_timestamp = CASE WHEN prev.archived \
                     THEN to_timestamp($2::double precision / 1000.0) \
                     ELSE tm.archive_timestamp END, \
                 last_activity_at = to_timestamp($2::double precision / 1000.0), \
                 message_count = tm.message_count + 1, \
                 total_message_sent = tm.total_message_sent + 1 \
             FROM prev WHERE tm.channel_id = prev.channel_id RETURNING prev.archived",
        )
        .bind(i(thread.get()))
        .bind(now_ms)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let Some(unarchived) = prev else {
            return Ok(None);
        };
        let joined = sqlx::query(
            "INSERT INTO thread_members (channel_id, user_id, joined_at) \
             SELECT $1, $2, to_timestamp($3::double precision / 1000.0) \
             WHERE (SELECT count(*) FROM thread_members WHERE channel_id = $1) < $4 \
             ON CONFLICT DO NOTHING",
        )
        .bind(i(thread.get()))
        .bind(i(author.get()))
        .bind(now_ms)
        .bind(MAX_MEMBERS)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected()
            > 0;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{CHANNEL_SELECT} WHERE c.id = $1"
        )))
        .bind(i(thread.get()))
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let channel = channel_from(&row)?;
        let member = if joined {
            let r = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT {MEMBER_COLS} FROM thread_members WHERE channel_id = $1 AND user_id = $2"
            )))
            .bind(i(thread.get()))
            .bind(i(author.get()))
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            Some(member_from(&r)?)
        } else {
            None
        };
        tx.commit().await.map_err(db)?;
        Ok(Some(ActivityOutcome {
            thread: channel,
            unarchived,
            joined: member,
        }))
    }

    async fn archive_idle_threads(&self, now_ms: i64, limit: usize) -> RepoResult<Vec<ChannelId>> {
        // Pinned forum posts (flag bit 1<<1) never auto-archive.
        let rows: Vec<i64> = sqlx::query_scalar(
            "UPDATE thread_metadata SET archived = true, \
                 archive_timestamp = to_timestamp($1::double precision / 1000.0) \
             WHERE channel_id IN ( \
                 SELECT tm.channel_id FROM thread_metadata tm \
                 JOIN channels c ON c.id = tm.channel_id \
                 WHERE NOT tm.archived AND (c.flags & 2) = 0 \
                   AND tm.last_activity_at + make_interval(mins => tm.auto_archive_duration) \
                       <= to_timestamp($1::double precision / 1000.0) \
                 ORDER BY tm.last_activity_at LIMIT $2 FOR UPDATE OF tm SKIP LOCKED) \
             RETURNING channel_id",
        )
        .bind(now_ms)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows.into_iter().map(|v| ChannelId::new(u(v))).collect())
    }
}
