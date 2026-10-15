//! Postgres implementation of [`GuildAdminRepository`] for [`PgChatRepo`].
//! Tests: `tests/guild_admin_repo.rs` (needs `SR_TEST_DATABASE_URL`).

use async_trait::async_trait;
use sqlx::Row;
use sqlx::postgres::PgRow;

use sr_chat::admin_repo::{Ban, GuildAdminRepository, MemberFilter, PruneCriteria};
use sr_chat::domain::{Channel, Guild, Member};
use sr_chat::repo::RepoResult;
use sr_core::permissions::{Overwrite, OverwriteKind};
use sr_core::{ChannelId, GuildId, RoleId, Snowflake, UserId};

use crate::accounts::NOT_SYSTEM_MEMBER;
use crate::chat_repo::{GUILD_COLS, PgChatRepo, db, guild_from, i, u};

fn lim(limit: usize) -> i64 {
    i64::try_from(limit).unwrap_or(i64::MAX)
}

/// `%text%` for `ILIKE ... ESCAPE '!'`, with the LIKE metacharacters in `text` escaped.
fn like_pattern(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('%');
    for c in text.chars() {
        if matches!(c, '!' | '%' | '_') {
            out.push('!');
        }
        out.push(c);
    }
    out.push('%');
    out
}

const MEMBER_SELECT: &str = "SELECT gm.guild_id, gm.user_id, gm.nick, gm.pending, \
    gm.avatar_key, gm.banner_key, gm.bio, gm.pronouns, gm.theme_primary, gm.theme_accent, \
    gm.profile_effect, gm.avatar_decoration, \
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
        avatar_key: r.try_get("avatar_key")?,
        banner_key: r.try_get("banner_key")?,
        bio: r.try_get("bio")?,
        extras: crate::chat_repo::extras_from(r)?,
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
    async fn set_vanity(&self, guild: GuildId, code: Option<&str>) -> RepoResult<bool> {
        match sqlx::query("UPDATE guilds SET vanity_code = $2, updated_at = now() WHERE id = $1")
            .bind(i(guild.get()))
            .bind(code)
            .execute(&self.pool)
            .await
        {
            Ok(_) => Ok(true),
            // The code is unique across guilds (CITEXT UNIQUE); a lost race reads as taken.
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Ok(false),
            Err(e) => Err(db(e)),
        }
    }

    async fn update_guild(&self, g: &Guild) -> RepoResult<()> {
        sqlx::query(
            "UPDATE guilds SET name = $2, description = $3, icon_key = $4, owner_id = $5, \
             system_channel_id = $6, verification_level = $7, default_message_notifications = $8, \
             explicit_content_filter = $9, preferred_locale = $10, banner_key = $11, splash_key = $12, \
             afk_channel_id = $13, afk_timeout = $14, \
             updated_at = now() WHERE id = $1",
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
        .bind(g.banner_key.as_deref())
        .bind(g.splash_key.as_deref())
        .bind(g.afk_channel_id.map(|c| i(c.get())))
        .bind(i32::try_from(g.afk_timeout).unwrap_or(300))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_guild(&self, id: GuildId) -> RepoResult<()> {
        // LNG-SSET-073: soft delete. Messages become `user_deleted` with the 30-day
        // purge window (held ones keep no purge time, D-122); the guild row and its
        // channels stay until the purge job finds nothing left (see sr-server jobs).
        let mut tx = self.pool.begin().await.map_err(db)?;
        // The vanity code is released at once: vanity lookups do not filter deleted
        // servers, and another server may want the code.
        let stamped = sqlx::query(
            "UPDATE guilds SET deleted_at = now(), vanity_code = NULL \
             WHERE id = $1 AND NOT is_staff_guild AND deleted_at IS NULL",
        )
        .bind(i(id.get()))
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        if stamped == 0 {
            return tx.commit().await.map_err(db);
        }
        sqlx::query(
            "DELETE FROM pins WHERE channel_id IN (SELECT id FROM channels WHERE guild_id = $1)",
        )
        .bind(i(id.get()))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query(
            "UPDATE messages SET visibility = 'user_deleted', pinned = FALSE, deleted_at = now(), \
             purge_after = CASE WHEN hold_id IS NULL THEN now() + interval '30 days' END \
             WHERE visibility = 'visible' \
               AND channel_id IN (SELECT id FROM channels WHERE guild_id = $1)",
        )
        .bind(i(id.get()))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        for table in [
            "guild_members",
            "guild_invites",
            "webhooks",
            "application_installs",
            // A deleted server leaves the Server Directory at once (LNG-GLD-013).
            "guild_directory",
        ] {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "DELETE FROM {table} WHERE guild_id = $1"
            )))
            .bind(i(id.get()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)
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
        filter: &MemberFilter,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<Member>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{MEMBER_SELECT} WHERE gm.guild_id = $1 AND gm.user_id > $2 AND {NOT_SYSTEM_MEMBER} \
             AND ($4::bigint IS NULL OR $4 = $1 OR EXISTS (SELECT 1 FROM member_roles mr \
                  WHERE mr.guild_id = gm.guild_id AND mr.user_id = gm.user_id AND mr.role_id = $4)) \
             AND ($5::text IS NULL OR gm.nick ILIKE $5 ESCAPE '!' OR EXISTS (SELECT 1 FROM accounts a \
                  WHERE a.id = gm.user_id AND (a.username::text ILIKE $5 ESCAPE '!' \
                  OR a.display_name ILIKE $5 ESCAPE '!'))) \
             ORDER BY gm.user_id LIMIT $3"
        )))
        .bind(i(guild.get()))
        .bind(after.map_or(0, |a| i(a.get())))
        .bind(lim(limit))
        .bind(filter.role.map(|r| i(r.get())))
        .bind(filter.query.as_deref().map(like_pattern))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(member_from)
            .collect::<Result<_, _>>()
            .map_err(db)
    }

    async fn prune_candidates(
        &self,
        guild: GuildId,
        criteria: &PruneCriteria,
        limit: usize,
    ) -> RepoResult<Vec<Member>> {
        let include: Vec<i64> = criteria.include_roles.iter().map(|r| i(r.get())).collect();
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{MEMBER_SELECT} WHERE gm.guild_id = $1 AND {NOT_SYSTEM_MEMBER} \
             AND gm.joined_at < to_timestamp($2::bigint::double precision / 1000.0) \
             AND gm.user_id <> (SELECT owner_id FROM guilds WHERE id = $1) \
             AND NOT EXISTS (SELECT 1 FROM member_roles mr WHERE mr.guild_id = gm.guild_id \
                  AND mr.user_id = gm.user_id AND NOT (mr.role_id = ANY($4))) \
             AND NOT EXISTS (SELECT 1 FROM messages msg WHERE msg.author_id = gm.user_id \
                  AND msg.guild_id = gm.guild_id AND msg.id >= sr_snowflake_floor( \
                  to_timestamp($3::bigint::double precision / 1000.0))) \
             AND NOT EXISTS (SELECT 1 FROM accounts a WHERE a.id = gm.user_id \
                  AND a.last_seen_at >= to_timestamp($6::bigint::double precision / 1000.0)) \
             ORDER BY gm.user_id LIMIT $5"
        )))
        .bind(i(guild.get()))
        .bind(criteria.joined_before_ms)
        .bind(criteria.active_since_ms)
        .bind(&include)
        .bind(lim(limit))
        .bind(criteria.seen_before_ms)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(member_from)
            .collect::<Result<_, _>>()
            .map_err(db)
    }

    async fn remove_members(&self, guild: GuildId, users: &[UserId]) -> RepoResult<u64> {
        let ids: Vec<i64> = users.iter().map(|u| i(u.get())).collect();
        let r = sqlx::query("DELETE FROM guild_members WHERE guild_id = $1 AND user_id = ANY($2)")
            .bind(i(guild.get()))
            .bind(&ids)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(r.rows_affected())
    }

    async fn update_member(&self, m: &Member) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "UPDATE guild_members SET nick = $3, \
             communication_disabled_until = to_timestamp($4::bigint::double precision / 1000.0), \
             pending = $5, avatar_key = $6, banner_key = $7, bio = $8, \
             pronouns = $9, theme_primary = $10, theme_accent = $11, profile_effect = $12, \
             avatar_decoration = $13, updated_at = now() WHERE guild_id = $1 AND user_id = $2",
        )
        .bind(i(m.guild_id.get()))
        .bind(i(m.user_id.get()))
        .bind(m.nick.as_deref())
        .bind(m.communication_disabled_until_ms)
        .bind(m.pending)
        .bind(m.avatar_key.as_deref())
        .bind(m.banner_key.as_deref())
        .bind(m.bio.as_deref())
        .bind(m.extras.pronouns.as_deref())
        .bind(m.extras.theme_colors.map(|c| c[0] as i32))
        .bind(m.extras.theme_colors.map(|c| c[1] as i32))
        .bind(m.extras.profile_effect.as_deref())
        .bind(m.extras.avatar_decoration.as_deref())
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

    async fn remove_member_kicked(&self, guild: GuildId, user: UserId) -> RepoResult<()> {
        // The staff-member-left trigger reads this to record `kicked` instead of `left` (G-23).
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("SELECT set_config('sr.member_removal', 'kicked', true)")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("DELETE FROM guild_members WHERE guild_id = $1 AND user_id = $2")
            .bind(i(guild.get()))
            .bind(i(user.get()))
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
        sqlx::query("SELECT set_config('sr.member_removal', 'kicked', true)")
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
