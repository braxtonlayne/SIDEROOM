//! Postgres implementation of [`GuildAdminRepository`] for [`PgChatRepo`].
//! Tests: `tests/guild_admin_repo.rs` (needs `SR_TEST_DATABASE_URL`).

use async_trait::async_trait;
use sqlx::Row;
use sqlx::postgres::PgRow;

use sr_chat::admin_repo::{Ban, GuildAdminRepository};
use sr_chat::domain::{Channel, Guild, Member};
use sr_chat::repo::RepoResult;
use sr_core::permissions::{Overwrite, OverwriteKind};
use sr_core::{ChannelId, GuildId, RoleId, Snowflake, UserId};

use crate::chat_repo::{GUILD_COLS, PgChatRepo, db, guild_from, i, u};

fn lim(limit: usize) -> i64 {
    i64::try_from(limit).unwrap_or(i64::MAX)
}

const MEMBER_SELECT: &str = "SELECT gm.guild_id, gm.user_id, gm.nick, gm.pending, \
    (extract(epoch FROM gm.joined_at) * 1000)::bigint AS joined_ms, \
    (extract(epoch FROM gm.communication_disabled_until) * 1000)::bigint AS timeout_ms, \
    COALESCE((SELECT array_agg(mr.role_id) FROM member_roles mr \
              WHERE mr.guild_id = gm.guild_id AND mr.user_id = gm.user_id), '{}') AS role_ids \
    FROM guild_members gm";

fn member_from(r: &PgRow) -> Result<Member, sqlx::Error> {
    Ok(Member {
        guild_id: GuildId::new(u(r.try_get("guild_id")?)),
        user_id: UserId::new(u(r.try_get("user_id")?)),
        nick: r.try_get("nick")?,
        role_ids: r
            .try_get::<Vec<i64>, _>("role_ids")?
            .into_iter()
            .map(|v| RoleId::new(u(v)))
            .collect(),
        joined_at_ms: r.try_get("joined_ms")?,
        communication_disabled_until_ms: r.try_get("timeout_ms")?,
        pending: r.try_get("pending")?,
    })
}

fn ban_from(r: &PgRow) -> Result<Ban, sqlx::Error> {
    Ok(Ban {
        guild_id: GuildId::new(u(r.try_get("guild_id")?)),
        user_id: UserId::new(u(r.try_get("user_id")?)),
        reason: r.try_get("reason")?,
        banned_by: r
            .try_get::<Option<i64>, _>("banned_by")?
            .map(|v| UserId::new(u(v))),
        created_at_ms: r.try_get("created_ms")?,
    })
}

const BAN_SELECT: &str = "SELECT guild_id, user_id, reason, banned_by, \
    (extract(epoch FROM created_at) * 1000)::bigint AS created_ms FROM guild_bans";

fn kind_str(k: OverwriteKind) -> &'static str {
    match k {
        OverwriteKind::Role => "role",
        OverwriteKind::Member => "member",
    }
}

#[async_trait]
impl GuildAdminRepository for PgChatRepo {
    async fn update_guild(&self, g: &Guild) -> RepoResult<()> {
        sqlx::query(
            "UPDATE guilds SET name = $2, description = $3, icon_key = $4, owner_id = $5, \
             system_channel_id = $6, verification_level = $7, default_message_notifications = $8, \
             explicit_content_filter = $9, preferred_locale = $10, updated_at = now() WHERE id = $1",
        )
        .bind(i(g.id.get()))
        .bind(&g.name)
        .bind(g.description.as_deref())
        .bind(g.icon_key.as_deref())
        .bind(i(g.owner_id.get()))
        .bind(g.system_channel_id.map(|c| i(c.get())))
        .bind(i16::from(g.verification_level))
        .bind(i16::from(g.default_message_notifications))
        .bind(i16::from(g.explicit_content_filter))
        .bind(&g.preferred_locale)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_guild(&self, id: GuildId) -> RepoResult<()> {
        // Roles, members, bans and channels cascade.
        sqlx::query("DELETE FROM guilds WHERE id = $1 AND NOT is_staff_guild")
            .bind(i(id.get()))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn list_user_guilds(&self, user: UserId) -> RepoResult<Vec<Guild>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {GUILD_COLS} FROM guilds WHERE id IN \
             (SELECT guild_id FROM guild_members WHERE user_id = $1) ORDER BY id"
        )))
        .bind(i(user.get()))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(guild_from)
            .collect::<Result<_, _>>()
            .map_err(db)
    }

    async fn list_members(
        &self,
        guild: GuildId,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<Member>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{MEMBER_SELECT} WHERE gm.guild_id = $1 AND gm.user_id > $2 \
             ORDER BY gm.user_id LIMIT $3"
        )))
        .bind(i(guild.get()))
        .bind(after.map_or(0, |a| i(a.get())))
        .bind(lim(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(member_from)
            .collect::<Result<_, _>>()
            .map_err(db)
    }

    async fn update_member(&self, m: &Member) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "UPDATE guild_members SET nick = $3, \
             communication_disabled_until = to_timestamp($4::bigint::double precision / 1000.0), \
             updated_at = now() WHERE guild_id = $1 AND user_id = $2",
        )
        .bind(i(m.guild_id.get()))
        .bind(i(m.user_id.get()))
        .bind(m.nick.as_deref())
        .bind(m.communication_disabled_until_ms)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        let roles: Vec<i64> = m.role_ids.iter().map(|r| i(r.get())).collect();
        sqlx::query(
            "DELETE FROM member_roles WHERE guild_id = $1 AND user_id = $2 \
             AND NOT (role_id = ANY($3))",
        )
        .bind(i(m.guild_id.get()))
        .bind(i(m.user_id.get()))
        .bind(&roles)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query(
            "INSERT INTO member_roles (guild_id, user_id, role_id) \
             SELECT $1, $2, r FROM unnest($3::bigint[]) AS r ON CONFLICT DO NOTHING",
        )
        .bind(i(m.guild_id.get()))
        .bind(i(m.user_id.get()))
        .bind(&roles)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn remove_member(&self, guild: GuildId, user: UserId) -> RepoResult<()> {
        // member_roles rows cascade.
        sqlx::query("DELETE FROM guild_members WHERE guild_id = $1 AND user_id = $2")
            .bind(i(guild.get()))
            .bind(i(user.get()))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn set_role_positions(
        &self,
        guild: GuildId,
        positions: &[(RoleId, i32)],
    ) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        for (id, pos) in positions {
            sqlx::query(
                "UPDATE roles SET position = $3, updated_at = now() \
                 WHERE id = $1 AND guild_id = $2",
            )
            .bind(i(id.get()))
            .bind(i(guild.get()))
            .bind(*pos)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    async fn update_channel_layout(&self, channels: &[Channel]) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        for c in channels {
            sqlx::query(
                "UPDATE channels SET position = $2, parent_id = $3, updated_at = now() \
                 WHERE id = $1",
            )
            .bind(i(c.id.get()))
            .bind(c.position)
            .bind(c.parent_id.map(|p| i(p.get())))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    async fn put_overwrite(&self, channel: ChannelId, o: &Overwrite) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO permission_overwrites (channel_id, target_id, target_type, allow, deny) \
             VALUES ($1, $2, $3::overwrite_target, $4, $5) \
             ON CONFLICT (channel_id, target_id) DO UPDATE SET \
             target_type = EXCLUDED.target_type, allow = EXCLUDED.allow, \
             deny = EXCLUDED.deny, updated_at = now()",
        )
        .bind(i(channel.get()))
        .bind(i(o.id.get()))
        .bind(kind_str(o.kind))
        .bind(i(o.allow.bits()))
        .bind(i(o.deny.bits()))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_overwrite(
        &self,
        channel: ChannelId,
        target: Snowflake,
        kind: OverwriteKind,
    ) -> RepoResult<()> {
        sqlx::query(
            "DELETE FROM permission_overwrites \
             WHERE channel_id = $1 AND target_id = $2 AND target_type = $3::overwrite_target",
        )
        .bind(i(channel.get()))
        .bind(i(target.get()))
        .bind(kind_str(kind))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn replace_overwrites(&self, channel: ChannelId, list: &[Overwrite]) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("DELETE FROM permission_overwrites WHERE channel_id = $1")
            .bind(i(channel.get()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        for o in list {
            sqlx::query(
                "INSERT INTO permission_overwrites \
                 (channel_id, target_id, target_type, allow, deny) \
                 VALUES ($1, $2, $3::overwrite_target, $4, $5)",
            )
            .bind(i(channel.get()))
            .bind(i(o.id.get()))
            .bind(kind_str(o.kind))
            .bind(i(o.allow.bits()))
            .bind(i(o.deny.bits()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    async fn insert_ban(&self, ban: &Ban) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "INSERT INTO guild_bans (guild_id, user_id, reason, banned_by, created_at) \
             VALUES ($1, $2, $3, $4, to_timestamp($5::bigint::double precision / 1000.0)) \
             ON CONFLICT (guild_id, user_id) DO UPDATE SET reason = EXCLUDED.reason, \
             banned_by = EXCLUDED.banned_by",
        )
        .bind(i(ban.guild_id.get()))
        .bind(i(ban.user_id.get()))
        .bind(ban.reason.as_deref())
        .bind(ban.banned_by.map(|b| i(b.get())))
        .bind(ban.created_at_ms)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query("DELETE FROM guild_members WHERE guild_id = $1 AND user_id = $2")
            .bind(i(ban.guild_id.get()))
            .bind(i(ban.user_id.get()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn delete_ban(&self, guild: GuildId, user: UserId) -> RepoResult<bool> {
        let r = sqlx::query("DELETE FROM guild_bans WHERE guild_id = $1 AND user_id = $2")
            .bind(i(guild.get()))
            .bind(i(user.get()))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(r.rows_affected() > 0)
    }

    async fn get_ban(&self, guild: GuildId, user: UserId) -> RepoResult<Option<Ban>> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{BAN_SELECT} WHERE guild_id = $1 AND user_id = $2"
        )))
        .bind(i(guild.get()))
        .bind(i(user.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(ban_from).transpose().map_err(db)
    }

    async fn list_bans(
        &self,
        guild: GuildId,
        before: Option<UserId>,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<Ban>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{BAN_SELECT} WHERE guild_id = $1 AND user_id > $2 AND user_id < $3 \
             ORDER BY user_id LIMIT $4"
        )))
        .bind(i(guild.get()))
        .bind(after.map_or(0, |a| i(a.get())))
        .bind(before.map_or(i64::MAX, |b| i(b.get())))
        .bind(lim(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(ban_from)
            .collect::<Result<_, _>>()
            .map_err(db)
    }
}
