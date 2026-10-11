//! Postgres implementation of the `sr-chat` repository traits (docs/04 sections 2-3).
//!
//! Queries are runtime-checked. Timestamps cross the boundary as Unix milliseconds
//! (`to_timestamp($n / 1000.0)` in, `extract(epoch ...) * 1000` out) so no date-time
//! crate is needed. Tests live in `tests/chat_repo.rs` and need `SR_TEST_DATABASE_URL`.

use async_trait::async_trait;
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use sr_chat::domain::{
    Channel, ChannelType, Guild, Member, Message, MessageReference, MessageType, Role, Visibility,
};
use sr_chat::error::RepoError;
use sr_chat::guild_rules::GuildBlueprint;
use sr_chat::message_rules::{DeletePlan, EditPlan};
use sr_chat::repo::{ChannelRepository, GuildRepository, MessageRepository, RepoResult};
use sr_core::permissions::{Overwrite, OverwriteKind, Permissions};
use sr_core::{AttachmentId, ChannelId, GuildId, MessageId, RoleId, Snowflake, UserId};

/// Postgres-backed chat repository.
#[derive(Clone)]
pub struct PgChatRepo {
    pub(crate) pool: PgPool,
}

impl PgChatRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

pub(crate) fn db(e: sqlx::Error) -> RepoError {
    RepoError(e.to_string())
}

fn bad(what: &str) -> RepoError {
    RepoError(format!("unexpected value in column {what}"))
}

// Ids are positive Snowflakes (< 2^63), so these casts are lossless.
pub(crate) fn i(id: u64) -> i64 {
    id as i64
}
pub(crate) fn u(v: i64) -> u64 {
    v as u64
}

fn vis_str(v: Visibility) -> &'static str {
    match v {
        Visibility::Visible => "visible",
        Visibility::UserDeleted => "user_deleted",
        Visibility::StaffRemoved => "staff_removed",
    }
}

fn vis_parse(s: &str) -> RepoResult<Visibility> {
    Ok(match s {
        "visible" => Visibility::Visible,
        "user_deleted" => Visibility::UserDeleted,
        "staff_removed" => Visibility::StaffRemoved,
        _ => return Err(bad("visibility")),
    })
}

pub(crate) const GUILD_COLS: &str = "id, name, description, icon_key, owner_id, is_staff_guild, \
    system_channel_id, verification_level, default_message_notifications, \
    explicit_content_filter, preferred_locale";

pub(crate) fn guild_from(r: &PgRow) -> Result<Guild, sqlx::Error> {
    Ok(Guild {
        id: GuildId::new(u(r.try_get("id")?)),
        name: r.try_get("name")?,
        description: r.try_get("description")?,
        icon_key: r.try_get("icon_key")?,
        owner_id: UserId::new(u(r.try_get("owner_id")?)),
        is_staff_guild: r.try_get("is_staff_guild")?,
        system_channel_id: r
            .try_get::<Option<i64>, _>("system_channel_id")?
            .map(|v| ChannelId::new(u(v))),
        verification_level: u8::try_from(r.try_get::<i16, _>("verification_level")?).unwrap_or(0),
        default_message_notifications: u8::try_from(
            r.try_get::<i16, _>("default_message_notifications")?,
        )
        .unwrap_or(0),
        explicit_content_filter: u8::try_from(r.try_get::<i16, _>("explicit_content_filter")?)
            .unwrap_or(0),
        preferred_locale: r.try_get("preferred_locale")?,
    })
}

const ROLE_COLS: &str = "id, guild_id, name, color, hoist, position, permissions, mentionable, \
    managed, unicode_emoji, icon_key";

fn role_from(r: &PgRow) -> Result<Role, sqlx::Error> {
    Ok(Role {
        id: RoleId::new(u(r.try_get("id")?)),
        guild_id: GuildId::new(u(r.try_get("guild_id")?)),
        name: r.try_get("name")?,
        color: u32::try_from(r.try_get::<i32, _>("color")?).unwrap_or(0),
        hoist: r.try_get("hoist")?,
        position: r.try_get("position")?,
        permissions: Permissions::from_bits_truncate(u(r.try_get("permissions")?)),
        mentionable: r.try_get("mentionable")?,
        managed: r.try_get("managed")?,
        unicode_emoji: r.try_get("unicode_emoji")?,
        icon_key: r.try_get("icon_key")?,
    })
}

const CHANNEL_COLS: &str = "id, guild_id, type, name, topic, position, parent_id, nsfw, \
    rate_limit_per_user, bitrate, user_limit, last_message_id, owner_id";

fn channel_from(r: &PgRow) -> RepoResult<Channel> {
    let kind = ChannelType::from_code(r.try_get("type").map_err(db)?).ok_or_else(|| bad("type"))?;
    let opt = |name: &str| -> RepoResult<Option<u64>> {
        Ok(r.try_get::<Option<i64>, _>(name).map_err(db)?.map(u))
    };
    Ok(Channel {
        id: ChannelId::new(u(r.try_get("id").map_err(db)?)),
        guild_id: opt("guild_id")?.map(GuildId::new),
        kind,
        name: r.try_get("name").map_err(db)?,
        topic: r.try_get("topic").map_err(db)?,
        position: r.try_get("position").map_err(db)?,
        parent_id: opt("parent_id")?.map(ChannelId::new),
        nsfw: r.try_get("nsfw").map_err(db)?,
        rate_limit_per_user: u32::try_from(r.try_get::<i32, _>("rate_limit_per_user").map_err(db)?)
            .unwrap_or(0),
        bitrate: r
            .try_get::<Option<i32>, _>("bitrate")
            .map_err(db)?
            .and_then(|v| u32::try_from(v).ok()),
        user_limit: r
            .try_get::<Option<i32>, _>("user_limit")
            .map_err(db)?
            .and_then(|v| u32::try_from(v).ok()),
        last_message_id: opt("last_message_id")?.map(MessageId::new),
        owner_id: opt("owner_id")?.map(UserId::new),
    })
}

const MSG_COLS: &str = "id, channel_id, guild_id, author_id, type, content, flags, \
    (extract(epoch FROM edited_at) * 1000)::bigint AS edited_ms, reference, \
    COALESCE((SELECT array_agg(a.id) FROM attachments a WHERE a.message_id = messages.id), '{}') AS attachment_ids, \
    jsonb_array_length(embeds) AS embed_count, sticker_ids, (poll IS NOT NULL) AS has_poll, \
    mention_everyone, mention_user_ids, mention_role_ids, mention_channel_ids, pinned, \
    visibility::text AS visibility, \
    (extract(epoch FROM deleted_at) * 1000)::bigint AS deleted_ms, \
    (extract(epoch FROM purge_after) * 1000)::bigint AS purge_ms, hold_id";

fn message_from(r: &PgRow) -> RepoResult<Message> {
    let ids = |name: &str| -> RepoResult<Vec<u64>> {
        Ok(r.try_get::<Vec<i64>, _>(name)
            .map_err(db)?
            .into_iter()
            .map(u)
            .collect())
    };
    let reference = r
        .try_get::<Option<serde_json::Value>, _>("reference")
        .map_err(db)?
        .and_then(|v| {
            let m = v.get("message_id")?.as_str()?.parse::<u64>().ok()?;
            let c = v.get("channel_id")?.as_str()?.parse::<u64>().ok()?;
            Some(MessageReference {
                message_id: MessageId::new(m),
                channel_id: ChannelId::new(c),
            })
        });
    Ok(Message {
        id: MessageId::new(u(r.try_get("id").map_err(db)?)),
        channel_id: ChannelId::new(u(r.try_get("channel_id").map_err(db)?)),
        guild_id: r
            .try_get::<Option<i64>, _>("guild_id")
            .map_err(db)?
            .map(|v| GuildId::new(u(v))),
        author_id: UserId::new(u(r.try_get("author_id").map_err(db)?)),
        kind: MessageType::from_code(r.try_get("type").map_err(db)?).ok_or_else(|| bad("type"))?,
        content: r.try_get("content").map_err(db)?,
        flags: u32::try_from(r.try_get::<i32, _>("flags").map_err(db)?).unwrap_or(0),
        edited_at_ms: r.try_get("edited_ms").map_err(db)?,
        reference,
        attachment_ids: ids("attachment_ids")?
            .into_iter()
            .map(AttachmentId::new)
            .collect(),
        embed_count: u8::try_from(r.try_get::<i32, _>("embed_count").map_err(db)?)
            .unwrap_or(u8::MAX),
        sticker_ids: ids("sticker_ids")?,
        has_poll: r.try_get("has_poll").map_err(db)?,
        mention_everyone: r.try_get("mention_everyone").map_err(db)?,
        mention_user_ids: ids("mention_user_ids")?
            .into_iter()
            .map(UserId::new)
            .collect(),
        mention_role_ids: ids("mention_role_ids")?
            .into_iter()
            .map(RoleId::new)
            .collect(),
        mention_channel_ids: ids("mention_channel_ids")?
            .into_iter()
            .map(ChannelId::new)
            .collect(),
        pinned: r.try_get("pinned").map_err(db)?,
        visibility: vis_parse(&r.try_get::<String, _>("visibility").map_err(db)?)?,
        deleted_at_ms: r.try_get("deleted_ms").map_err(db)?,
        purge_after_ms: r.try_get("purge_ms").map_err(db)?,
        hold_id: r.try_get::<Option<i64>, _>("hold_id").map_err(db)?.map(u),
    })
}

fn ids_i64<T: Copy>(v: &[T], f: impl Fn(T) -> u64) -> Vec<i64> {
    v.iter().map(|x| i(f(*x))).collect()
}

const INSERT_CHANNEL: &str = "INSERT INTO channels (id, guild_id, type, name, topic, position, \
    parent_id, nsfw, rate_limit_per_user, bitrate, user_limit, owner_id) \
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)";

fn bind_channel<'q>(
    q: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    c: &'q Channel,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    q.bind(i(c.id.get()))
        .bind(c.guild_id.map(|g| i(g.get())))
        .bind(c.kind.code())
        .bind(c.name.as_deref())
        .bind(c.topic.as_deref())
        .bind(c.position)
        .bind(c.parent_id.map(|p| i(p.get())))
        .bind(c.nsfw)
        .bind(i32::try_from(c.rate_limit_per_user).unwrap_or(0))
        .bind(c.bitrate.and_then(|v| i32::try_from(v).ok()))
        .bind(c.user_limit.and_then(|v| i32::try_from(v).ok()))
        .bind(c.owner_id.map(|o| i(o.get())))
}

#[async_trait]
impl GuildRepository for PgChatRepo {
    async fn insert_guild(&self, bp: &GuildBlueprint) -> RepoResult<()> {
        let g = &bp.guild;
        let mut tx = self.pool.begin().await.map_err(db)?;
        // The system channel FK is set after the channels exist.
        sqlx::query(
            "INSERT INTO guilds (id, name, description, icon_key, owner_id, is_staff_guild, \
             verification_level, default_message_notifications, explicit_content_filter, \
             preferred_locale) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(i(g.id.get()))
        .bind(&g.name)
        .bind(g.description.as_deref())
        .bind(g.icon_key.as_deref())
        .bind(i(g.owner_id.get()))
        .bind(g.is_staff_guild)
        .bind(i16::from(g.verification_level))
        .bind(i16::from(g.default_message_notifications))
        .bind(i16::from(g.explicit_content_filter))
        .bind(&g.preferred_locale)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        let e = &bp.everyone_role;
        sqlx::query(
            "INSERT INTO roles (id, guild_id, name, color, hoist, position, permissions, \
             mentionable, managed) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(i(e.id.get()))
        .bind(i(e.guild_id.get()))
        .bind(&e.name)
        .bind(i32::try_from(e.color).unwrap_or(0))
        .bind(e.hoist)
        .bind(e.position)
        .bind(i(e.permissions.bits()))
        .bind(e.mentionable)
        .bind(e.managed)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        let m = &bp.owner_member;
        sqlx::query(
            "INSERT INTO guild_members (guild_id, user_id, joined_at) \
             VALUES ($1, $2, to_timestamp($3::double precision / 1000.0))",
        )
        .bind(i(m.guild_id.get()))
        .bind(i(m.user_id.get()))
        .bind(m.joined_at_ms)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        for c in &bp.channels {
            bind_channel(sqlx::query(INSERT_CHANNEL), c)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        if let Some(sys) = g.system_channel_id {
            sqlx::query("UPDATE guilds SET system_channel_id = $2 WHERE id = $1")
                .bind(i(g.id.get()))
                .bind(i(sys.get()))
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    async fn get_guild(&self, id: GuildId) -> RepoResult<Option<Guild>> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {GUILD_COLS} FROM guilds WHERE id = $1"
        )))
        .bind(i(id.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(guild_from).transpose().map_err(db)
    }

    async fn get_member(&self, guild: GuildId, user: UserId) -> RepoResult<Option<Member>> {
        let row = sqlx::query(
            "SELECT gm.nick, gm.pending, \
             (extract(epoch FROM gm.joined_at) * 1000)::bigint AS joined_ms, \
             (extract(epoch FROM gm.communication_disabled_until) * 1000)::bigint AS timeout_ms, \
             COALESCE((SELECT array_agg(mr.role_id) FROM member_roles mr \
                       WHERE mr.guild_id = gm.guild_id AND mr.user_id = gm.user_id), '{}') AS role_ids \
             FROM guild_members gm WHERE gm.guild_id = $1 AND gm.user_id = $2",
        )
        .bind(i(guild.get()))
        .bind(i(user.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        let Some(r) = row else { return Ok(None) };
        Ok(Some(Member {
            guild_id: guild,
            user_id: user,
            nick: r.try_get("nick").map_err(db)?,
            role_ids: r
                .try_get::<Vec<i64>, _>("role_ids")
                .map_err(db)?
                .into_iter()
                .map(|v| RoleId::new(u(v)))
                .collect(),
            joined_at_ms: r.try_get("joined_ms").map_err(db)?,
            communication_disabled_until_ms: r.try_get("timeout_ms").map_err(db)?,
            pending: r.try_get("pending").map_err(db)?,
        }))
    }

    async fn list_roles(&self, guild: GuildId) -> RepoResult<Vec<Role>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {ROLE_COLS} FROM roles WHERE guild_id = $1 ORDER BY position, id"
        )))
        .bind(i(guild.get()))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(role_from)
            .collect::<Result<_, _>>()
            .map_err(db)
    }

    async fn insert_role(&self, role: &Role) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "UPDATE roles SET position = position + 1 WHERE guild_id = $1 AND position >= $2",
        )
        .bind(i(role.guild_id.get()))
        .bind(role.position)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query(
            "INSERT INTO roles (id, guild_id, name, color, hoist, position, permissions, \
             mentionable, managed, unicode_emoji, icon_key) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(i(role.id.get()))
        .bind(i(role.guild_id.get()))
        .bind(&role.name)
        .bind(i32::try_from(role.color).unwrap_or(0))
        .bind(role.hoist)
        .bind(role.position)
        .bind(i(role.permissions.bits()))
        .bind(role.mentionable)
        .bind(role.managed)
        .bind(role.unicode_emoji.as_deref())
        .bind(role.icon_key.as_deref())
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn update_role(&self, role: &Role) -> RepoResult<()> {
        sqlx::query(
            "UPDATE roles SET name = $2, color = $3, hoist = $4, position = $5, permissions = $6, \
             mentionable = $7, unicode_emoji = $8, icon_key = $9, updated_at = now() WHERE id = $1",
        )
        .bind(i(role.id.get()))
        .bind(&role.name)
        .bind(i32::try_from(role.color).unwrap_or(0))
        .bind(role.hoist)
        .bind(role.position)
        .bind(i(role.permissions.bits()))
        .bind(role.mentionable)
        .bind(role.unicode_emoji.as_deref())
        .bind(role.icon_key.as_deref())
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_role(&self, guild: GuildId, role: RoleId) -> RepoResult<()> {
        // member_roles rows cascade.
        sqlx::query("DELETE FROM roles WHERE id = $1 AND guild_id = $2")
            .bind(i(role.get()))
            .bind(i(guild.get()))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }
}

#[async_trait]
impl ChannelRepository for PgChatRepo {
    async fn get_channel(&self, id: ChannelId) -> RepoResult<Option<Channel>> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {CHANNEL_COLS} FROM channels WHERE id = $1"
        )))
        .bind(i(id.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(channel_from).transpose()
    }

    async fn list_guild_channels(&self, guild: GuildId) -> RepoResult<Vec<Channel>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {CHANNEL_COLS} FROM channels WHERE guild_id = $1 ORDER BY position, id"
        )))
        .bind(i(guild.get()))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(channel_from).collect()
    }

    async fn get_overwrites(&self, channel: ChannelId) -> RepoResult<Vec<Overwrite>> {
        let rows = sqlx::query(
            "SELECT target_id, target_type::text AS target_type, allow, deny \
             FROM permission_overwrites WHERE channel_id = $1",
        )
        .bind(i(channel.get()))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|r| {
                let kind = match r.try_get::<String, _>("target_type").map_err(db)?.as_str() {
                    "role" => OverwriteKind::Role,
                    "member" => OverwriteKind::Member,
                    _ => return Err(bad("target_type")),
                };
                Ok(Overwrite {
                    id: Snowflake::new(u(r.try_get("target_id").map_err(db)?)),
                    kind,
                    allow: Permissions::from_bits_truncate(u(r.try_get("allow").map_err(db)?)),
                    deny: Permissions::from_bits_truncate(u(r.try_get("deny").map_err(db)?)),
                })
            })
            .collect()
    }

    async fn insert_channel(&self, channel: &Channel, overwrites: &[Overwrite]) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        bind_channel(sqlx::query(INSERT_CHANNEL), channel)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        for o in overwrites {
            sqlx::query(
                "INSERT INTO permission_overwrites (channel_id, target_id, target_type, allow, deny) \
                 VALUES ($1, $2, $3::overwrite_target, $4, $5)",
            )
            .bind(i(channel.id.get()))
            .bind(i(o.id.get()))
            .bind(match o.kind {
                OverwriteKind::Role => "role",
                OverwriteKind::Member => "member",
            })
            .bind(i(o.allow.bits()))
            .bind(i(o.deny.bits()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    async fn update_channel(&self, c: &Channel) -> RepoResult<()> {
        sqlx::query(
            "UPDATE channels SET name = $2, topic = $3, position = $4, parent_id = $5, nsfw = $6, \
             rate_limit_per_user = $7, bitrate = $8, user_limit = $9, updated_at = now() WHERE id = $1",
        )
        .bind(i(c.id.get()))
        .bind(c.name.as_deref())
        .bind(c.topic.as_deref())
        .bind(c.position)
        .bind(c.parent_id.map(|p| i(p.get())))
        .bind(c.nsfw)
        .bind(i32::try_from(c.rate_limit_per_user).unwrap_or(0))
        .bind(c.bitrate.and_then(|v| i32::try_from(v).ok()))
        .bind(c.user_limit.and_then(|v| i32::try_from(v).ok()))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_channel(&self, id: ChannelId) -> RepoResult<()> {
        // parent_id is ON DELETE SET NULL, so category children are orphaned.
        sqlx::query("DELETE FROM channels WHERE id = $1")
            .bind(i(id.get()))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn is_recipient(&self, channel: ChannelId, user: UserId) -> RepoResult<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM channel_recipients WHERE channel_id = $1 AND user_id = $2)",
        )
        .bind(i(channel.get()))
        .bind(i(user.get()))
        .fetch_one(&self.pool)
        .await
        .map_err(db)
    }
}

#[async_trait]
impl MessageRepository for PgChatRepo {
    async fn get_message(&self, channel: ChannelId, id: MessageId) -> RepoResult<Option<Message>> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {MSG_COLS} FROM messages WHERE channel_id = $1 AND id = $2"
        )))
        .bind(i(channel.get()))
        .bind(i(id.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(message_from).transpose()
    }

    async fn get_messages(
        &self,
        channel: ChannelId,
        ids: &[MessageId],
    ) -> RepoResult<Vec<Message>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {MSG_COLS} FROM messages WHERE channel_id = $1 AND id = ANY($2)"
        )))
        .bind(i(channel.get()))
        .bind(ids_i64(ids, MessageId::get))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(message_from).collect()
    }

    async fn insert_message(&self, m: &Message) -> RepoResult<()> {
        let reference = m.reference.as_ref().map(|r| {
            serde_json::json!({
                "message_id": r.message_id.to_string(),
                "channel_id": r.channel_id.to_string(),
            })
        });
        let mut tx = self.pool.begin().await.map_err(db)?;
        // Embed and poll payloads are not part of the domain type yet: embeds are
        // stored empty and a poll as an empty object (see docs/open-questions.md).
        sqlx::query(
            "INSERT INTO messages (id, channel_id, guild_id, author_id, type, content, flags, \
             reference, sticker_ids, poll, mention_everyone, mention_user_ids, mention_role_ids, \
             mention_channel_ids, pinned) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, \
             CASE WHEN $10 THEN '{}'::jsonb END, $11, $12, $13, $14, $15)",
        )
        .bind(i(m.id.get()))
        .bind(i(m.channel_id.get()))
        .bind(m.guild_id.map(|g| i(g.get())))
        .bind(i(m.author_id.get()))
        .bind(m.kind.code())
        .bind(&m.content)
        .bind(i32::try_from(m.flags).unwrap_or(0))
        .bind(reference)
        .bind(m.sticker_ids.iter().map(|s| i(*s)).collect::<Vec<_>>())
        .bind(m.has_poll)
        .bind(m.mention_everyone)
        .bind(ids_i64(&m.mention_user_ids, UserId::get))
        .bind(ids_i64(&m.mention_role_ids, RoleId::get))
        .bind(ids_i64(&m.mention_channel_ids, ChannelId::get))
        .bind(m.pinned)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        if !m.attachment_ids.is_empty() {
            sqlx::query(
                "UPDATE attachments SET message_id = $1, state = 'ready', updated_at = now() \
                 WHERE id = ANY($2) AND uploader_id = $3 AND message_id IS NULL",
            )
            .bind(i(m.id.get()))
            .bind(ids_i64(&m.attachment_ids, AttachmentId::get))
            .bind(i(m.author_id.get()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        sqlx::query("UPDATE channels SET last_message_id = $2 WHERE id = $1")
            .bind(i(m.channel_id.get()))
            .bind(i(m.id.get()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn apply_edit(&self, plan: &EditPlan) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        if let Some(prev) = &plan.history_previous_content {
            sqlx::query(
                "INSERT INTO message_edits (message_id, edited_at, previous_content) \
                 VALUES ($1, to_timestamp($2::double precision / 1000.0), $3)",
            )
            .bind(i(plan.message_id.get()))
            .bind(plan.edited_at_ms)
            .bind(prev)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        sqlx::query(
            "UPDATE messages SET content = $2, flags = $3, \
             edited_at = to_timestamp($4::double precision / 1000.0), mention_everyone = $5, \
             mention_user_ids = $6, mention_role_ids = $7, mention_channel_ids = $8 WHERE id = $1",
        )
        .bind(i(plan.message_id.get()))
        .bind(&plan.content)
        .bind(i32::try_from(plan.flags).unwrap_or(0))
        .bind(plan.edited_at_ms)
        .bind(plan.mentions.everyone)
        .bind(ids_i64(&plan.mentions.users, UserId::get))
        .bind(ids_i64(&plan.mentions.roles, RoleId::get))
        .bind(ids_i64(&plan.mentions.channels, ChannelId::get))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn apply_delete(&self, plan: &DeletePlan) -> RepoResult<()> {
        sqlx::query(
            "UPDATE messages SET visibility = $2::message_visibility, \
             deleted_at = to_timestamp($3::double precision / 1000.0), \
             purge_after = CASE WHEN $4::bigint IS NULL THEN NULL \
                           ELSE to_timestamp($4::double precision / 1000.0) END WHERE id = $1",
        )
        .bind(i(plan.message_id.get()))
        .bind(vis_str(plan.visibility))
        .bind(plan.deleted_at_ms)
        .bind(plan.purge_after_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn last_sent_ms(&self, channel: ChannelId, user: UserId) -> RepoResult<Option<i64>> {
        // Snowflake ids sort by time, so the newest id carries the send time.
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT max(id) FROM messages WHERE channel_id = $1 AND author_id = $2",
        )
        .bind(i(channel.get()))
        .bind(i(user.get()))
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        Ok(id.map(|v| Snowflake::new(u(v)).unix_ms()))
    }

    async fn count_pins(&self, channel: ChannelId) -> RepoResult<usize> {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM pins WHERE channel_id = $1")
            .bind(i(channel.get()))
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        Ok(usize::try_from(n).unwrap_or(0))
    }

    async fn set_pinned(
        &self,
        channel: ChannelId,
        message: MessageId,
        pinned_by: UserId,
        pinned: bool,
        now_ms: i64,
    ) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        if pinned {
            sqlx::query(
                "INSERT INTO pins (channel_id, message_id, pinned_by, pinned_at) \
                 VALUES ($1, $2, $3, to_timestamp($4::double precision / 1000.0)) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(i(channel.get()))
            .bind(i(message.get()))
            .bind(i(pinned_by.get()))
            .bind(now_ms)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        } else {
            sqlx::query("DELETE FROM pins WHERE channel_id = $1 AND message_id = $2")
                .bind(i(channel.get()))
                .bind(i(message.get()))
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        sqlx::query("UPDATE messages SET pinned = $2 WHERE id = $1")
            .bind(i(message.get()))
            .bind(pinned)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)
    }
}
