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
use sr_chat::repo::{
    Attachment, ChannelRepository, EmojiKey, GuildRepository, MessageRepository, MessageStore,
    Page, ReactionSummary, RepoResult, StoreError, StoreResult, UploadState, UserLookup,
    UserSummary,
};
use sr_chat::threads::model::{ForumConfig, ThreadInfo};
use sr_core::permissions::{Overwrite, OverwriteKind, Permissions};
use sr_core::{AttachmentId, ChannelId, GuildId, MessageId, RoleId, Snowflake, UserId};
use std::collections::HashMap;

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

pub(crate) const GUILD_COLS: &str = "id, name, description, icon_key, banner_key, splash_key, \
    owner_id, is_staff_guild, system_channel_id, verification_level, default_message_notifications, \
    explicit_content_filter, preferred_locale, afk_channel_id, afk_timeout, tag, tag_badge, \
    vanity_code::text AS vanity_code";

pub(crate) fn guild_from(r: &PgRow) -> Result<Guild, sqlx::Error> {
    Ok(Guild {
        id: GuildId::new(u(r.try_get("id")?)),
        name: r.try_get("name")?,
        description: r.try_get("description")?,
        icon_key: r.try_get("icon_key")?,
        banner_key: r.try_get("banner_key")?,
        splash_key: r.try_get("splash_key")?,
        owner_id: UserId::new(u(r.try_get("owner_id")?)),
        is_staff_guild: r.try_get("is_staff_guild")?,
        system_channel_id: r
            .try_get::<Option<i64>, _>("system_channel_id")?
            .map(|v| ChannelId::new(u(v))),
        afk_channel_id: r
            .try_get::<Option<i64>, _>("afk_channel_id")?
            .map(|v| ChannelId::new(u(v))),
        afk_timeout: u32::try_from(r.try_get::<i32, _>("afk_timeout")?).unwrap_or(300),
        verification_level: u8::try_from(r.try_get::<i16, _>("verification_level")?).unwrap_or(0),
        default_message_notifications: u8::try_from(
            r.try_get::<i16, _>("default_message_notifications")?,
        )
        .unwrap_or(0),
        explicit_content_filter: u8::try_from(r.try_get::<i16, _>("explicit_content_filter")?)
            .unwrap_or(0),
        preferred_locale: r.try_get("preferred_locale")?,
        tag: r.try_get("tag")?,
        tag_badge: r.try_get("tag_badge")?,
        vanity_code: r.try_get("vanity_code")?,
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

/// `SELECT` list and joins for a [`Channel`]; append a `WHERE`. Thread rows join
/// `thread_metadata` and count their members.
pub(crate) const CHANNEL_SELECT: &str = "SELECT c.id, c.guild_id, c.type, c.name, c.topic, \
    c.position, c.parent_id, c.nsfw, c.rate_limit_per_user, c.bitrate, c.user_limit, \
    c.last_message_id, c.owner_id, c.flags, c.available_tags, c.default_reaction_emoji, \
    c.default_sort_order, c.default_forum_layout, c.default_thread_rate_limit, \
    c.default_auto_archive_duration, \
    tm.archived AS t_archived, tm.auto_archive_duration AS t_auto_archive, \
    (extract(epoch FROM tm.archive_timestamp) * 1000)::bigint AS t_archive_ms, \
    tm.locked AS t_locked, tm.invitable AS t_invitable, \
    (extract(epoch FROM tm.create_timestamp) * 1000)::bigint AS t_create_ms, \
    (extract(epoch FROM tm.last_activity_at) * 1000)::bigint AS t_activity_ms, \
    tm.message_count AS t_messages, tm.total_message_sent AS t_total, \
    tm.applied_tags AS t_tags, \
    (SELECT count(*) FROM thread_members m WHERE m.channel_id = c.id) AS t_members \
    FROM channels c LEFT JOIN thread_metadata tm ON tm.channel_id = c.id";

fn json_opt<T: for<'de> serde::Deserialize<'de>>(r: &PgRow, name: &str) -> RepoResult<Option<T>> {
    let v: Option<serde_json::Value> = r.try_get(name).map_err(db)?;
    Ok(v.and_then(|v| serde_json::from_value(v).ok()))
}

pub(crate) fn channel_from(r: &PgRow) -> RepoResult<Channel> {
    let kind = ChannelType::from_code(r.try_get("type").map_err(db)?).ok_or_else(|| bad("type"))?;
    let opt = |name: &str| -> RepoResult<Option<u64>> {
        Ok(r.try_get::<Option<i64>, _>(name).map_err(db)?.map(u))
    };
    let small = |name: &str| -> RepoResult<Option<u32>> {
        Ok(r.try_get::<Option<i32>, _>(name)
            .map_err(db)?
            .and_then(|v| u32::try_from(v).ok()))
    };
    let thread = match r.try_get::<Option<bool>, _>("t_archived").map_err(db)? {
        None => None,
        Some(archived) => Some(ThreadInfo {
            archived,
            auto_archive_duration: small("t_auto_archive")?.unwrap_or(1440),
            archive_timestamp_ms: r.try_get("t_archive_ms").map_err(db)?,
            locked: r.try_get("t_locked").map_err(db)?,
            invitable: r.try_get("t_invitable").map_err(db)?,
            create_timestamp_ms: r.try_get("t_create_ms").map_err(db)?,
            last_activity_ms: r.try_get("t_activity_ms").map_err(db)?,
            message_count: small("t_messages")?.unwrap_or(0),
            total_message_sent: small("t_total")?.unwrap_or(0),
            member_count: u32::try_from(r.try_get::<i64, _>("t_members").map_err(db)?)
                .unwrap_or(u32::MAX),
            applied_tags: r
                .try_get::<Option<Vec<i64>>, _>("t_tags")
                .map_err(db)?
                .unwrap_or_default()
                .into_iter()
                .map(u)
                .collect(),
        }),
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
        flags: u32::try_from(r.try_get::<i32, _>("flags").map_err(db)?).unwrap_or(0),
        thread,
        forum: ForumConfig {
            available_tags: json_opt(r, "available_tags")?.unwrap_or_default(),
            default_reaction_emoji: json_opt(r, "default_reaction_emoji")?,
            default_sort_order: r
                .try_get::<Option<i16>, _>("default_sort_order")
                .map_err(db)?
                .and_then(|v| u8::try_from(v).ok()),
            default_forum_layout: r
                .try_get::<Option<i16>, _>("default_forum_layout")
                .map_err(db)?
                .and_then(|v| u8::try_from(v).ok()),
            default_thread_rate_limit: small("default_thread_rate_limit")?,
            default_auto_archive_duration: small("default_auto_archive_duration")?,
        },
    })
}

const MSG_COLS: &str = "id, channel_id, guild_id, author_id, type, content, flags, \
    (extract(epoch FROM edited_at) * 1000)::bigint AS edited_ms, reference, \
    COALESCE((SELECT array_agg(a.id) FROM attachments a WHERE a.message_id = messages.id), '{}') AS attachment_ids, \
    embeds, components, poll, sticker_ids, \
    mention_everyone, mention_user_ids, mention_role_ids, mention_channel_ids, pinned, \
    visibility::text AS visibility, \
    (extract(epoch FROM deleted_at) * 1000)::bigint AS deleted_ms, \
    (extract(epoch FROM purge_after) * 1000)::bigint AS purge_ms, hold_id";

/// A jsonb array column decoded into `T`; malformed rows yield an empty list rather
/// than failing the read (the write path always stores valid payloads).
fn json_vec<T: for<'de> serde::Deserialize<'de>>(r: &PgRow, name: &str) -> RepoResult<Vec<T>> {
    let v: serde_json::Value = r.try_get(name).map_err(db)?;
    Ok(serde_json::from_value(v).unwrap_or_default())
}

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
                forward: v
                    .get("snapshot")
                    .and_then(|s| serde_json::from_value(s.clone()).ok()),
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
        embeds: json_vec(r, "embeds")?,
        components: json_vec(r, "components")?,
        poll: r
            .try_get::<Option<serde_json::Value>, _>("poll")
            .map_err(db)?
            .and_then(|v| serde_json::from_value(v).ok()),
        sticker_ids: ids("sticker_ids")?,
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

pub(crate) const INSERT_CHANNEL: &str = "INSERT INTO channels (id, guild_id, type, name, topic, position, \
    parent_id, nsfw, rate_limit_per_user, bitrate, user_limit, owner_id, flags, available_tags, \
    default_reaction_emoji, default_sort_order, default_forum_layout, default_thread_rate_limit, \
    default_auto_archive_duration) \
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)";

fn tags_json(c: &Channel) -> Option<serde_json::Value> {
    (!c.forum.available_tags.is_empty())
        .then(|| serde_json::to_value(&c.forum.available_tags).unwrap_or_default())
}

fn reaction_json(c: &Channel) -> Option<serde_json::Value> {
    c.forum
        .default_reaction_emoji
        .as_ref()
        .and_then(|r| serde_json::to_value(r).ok())
}

fn opt_i32(v: Option<u32>) -> Option<i32> {
    v.and_then(|v| i32::try_from(v).ok())
}

pub(crate) fn bind_channel<'q>(
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
        .bind(i32::try_from(c.flags).unwrap_or(0))
        .bind(tags_json(c))
        .bind(reaction_json(c))
        .bind(c.forum.default_sort_order.map(i16::from))
        .bind(c.forum.default_forum_layout.map(i16::from))
        .bind(opt_i32(c.forum.default_thread_rate_limit))
        .bind(opt_i32(c.forum.default_auto_archive_duration))
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

        for e in std::iter::once(&bp.everyone_role).chain(&bp.extra_roles) {
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
        }

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

        // Parents first: a category must exist before the channels under it.
        let mut ordered: Vec<&Channel> = bp.channels.iter().collect();
        ordered.sort_by_key(|c| c.parent_id.is_some());
        for c in ordered {
            bind_channel(sqlx::query(INSERT_CHANNEL), c)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        for (channel, o) in &bp.overwrites {
            sqlx::query(
                "INSERT INTO permission_overwrites (channel_id, target_id, target_type, allow, deny) \
                 VALUES ($1, $2, $3::overwrite_target, $4, $5)",
            )
            .bind(i(channel.get()))
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

    async fn guild_by_vanity(&self, code: &str) -> RepoResult<Option<GuildId>> {
        let row: Option<i64> = sqlx::query_scalar("SELECT id FROM guilds WHERE vanity_code = $1")
            .bind(code)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        Ok(row.map(|v| GuildId::new(u(v))))
    }

    async fn count_members(&self, guild: GuildId) -> RepoResult<u64> {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM guild_members WHERE guild_id = $1")
            .bind(i(guild.get()))
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    async fn page_members(
        &self,
        guild: GuildId,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<Member>> {
        let rows = sqlx::query(
            "SELECT gm.user_id, gm.nick, gm.pending, gm.avatar_key, gm.banner_key, gm.bio, \
             (extract(epoch FROM gm.joined_at) * 1000)::bigint AS joined_ms, \
             (extract(epoch FROM gm.communication_disabled_until) * 1000)::bigint AS timeout_ms, \
             COALESCE((SELECT array_agg(mr.role_id) FROM member_roles mr \
                       WHERE mr.guild_id = gm.guild_id AND mr.user_id = gm.user_id), '{}') AS role_ids \
             FROM guild_members gm WHERE gm.guild_id = $1 AND gm.user_id > $2 \
             ORDER BY gm.user_id LIMIT $3",
        )
        .bind(i(guild.get()))
        .bind(after.map_or(0, |a| i(a.get())))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|r| {
                Ok(Member {
                    guild_id: guild,
                    user_id: UserId::new(u(r.try_get("user_id").map_err(db)?)),
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
                    avatar_key: r.try_get("avatar_key").map_err(db)?,
                    banner_key: r.try_get("banner_key").map_err(db)?,
                    bio: r.try_get("bio").map_err(db)?,
                })
            })
            .collect()
    }

    async fn get_member(&self, guild: GuildId, user: UserId) -> RepoResult<Option<Member>> {
        let row = sqlx::query(
            "SELECT gm.nick, gm.pending, gm.avatar_key, gm.banner_key, gm.bio, \
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
            avatar_key: r.try_get("avatar_key").map_err(db)?,
            banner_key: r.try_get("banner_key").map_err(db)?,
            bio: r.try_get("bio").map_err(db)?,
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
            "{CHANNEL_SELECT} WHERE c.id = $1"
        )))
        .bind(i(id.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(channel_from).transpose()
    }

    async fn list_guild_channels(&self, guild: GuildId) -> RepoResult<Vec<Channel>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{CHANNEL_SELECT} WHERE c.guild_id = $1 AND c.type NOT IN (10, 11, 12) ORDER BY c.position, c.id"
        )))
        .bind(i(guild.get()))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(channel_from).collect()
    }

    async fn list_guild_threads(&self, guild: GuildId) -> RepoResult<Vec<Channel>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{CHANNEL_SELECT} WHERE c.guild_id = $1 AND c.type IN (10, 11, 12) ORDER BY c.id"
        )))
        .bind(i(guild.get()))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(channel_from).collect()
    }

    async fn get_threads(&self, ids: &[ChannelId]) -> RepoResult<Vec<Channel>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{CHANNEL_SELECT} WHERE c.id = ANY($1) AND c.type IN (10, 11, 12)"
        )))
        .bind(ids.iter().map(|c| i(c.get())).collect::<Vec<_>>())
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
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "UPDATE channels SET name = $2, topic = $3, position = $4, parent_id = $5, nsfw = $6, \
             rate_limit_per_user = $7, bitrate = $8, user_limit = $9, flags = $10, \
             available_tags = $11, default_reaction_emoji = $12, default_sort_order = $13, \
             default_forum_layout = $14, default_thread_rate_limit = $15, \
             default_auto_archive_duration = $16, updated_at = now() WHERE id = $1",
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
        .bind(i32::try_from(c.flags).unwrap_or(0))
        .bind(tags_json(c))
        .bind(reaction_json(c))
        .bind(c.forum.default_sort_order.map(i16::from))
        .bind(c.forum.default_forum_layout.map(i16::from))
        .bind(opt_i32(c.forum.default_thread_rate_limit))
        .bind(opt_i32(c.forum.default_auto_archive_duration))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        if c.kind.is_forum_like() {
            // Posts lose the tags the forum no longer offers.
            let keep: Vec<i64> = c.forum.available_tags.iter().map(|t| i(t.id)).collect();
            sqlx::query(
                "UPDATE thread_metadata tm SET applied_tags = COALESCE(( \
                     SELECT array_agg(t) FROM unnest(tm.applied_tags) t WHERE t = ANY($2)), '{}') \
                 FROM channels th \
                 WHERE th.id = tm.channel_id AND th.parent_id = $1 AND cardinality(tm.applied_tags) > 0",
            )
            .bind(i(c.id.get()))
            .bind(keep)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    async fn delete_channel(&self, id: ChannelId) -> RepoResult<()> {
        // Threads die with their parent (channels.parent_id is only SET NULL);
        // category children are orphaned, not deleted.
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("DELETE FROM channels WHERE parent_id = $1 AND type IN (10, 11, 12)")
            .bind(i(id.get()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("DELETE FROM channels WHERE id = $1")
            .bind(i(id.get()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn is_thread_member(&self, channel: ChannelId, user: UserId) -> RepoResult<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM thread_members WHERE channel_id = $1 AND user_id = $2)",
        )
        .bind(i(channel.get()))
        .bind(i(user.get()))
        .fetch_one(&self.pool)
        .await
        .map_err(db)
    }

    async fn list_thread_member_ids(
        &self,
        channel: ChannelId,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<UserId>> {
        let rows: Vec<i64> = sqlx::query_scalar(
            "SELECT user_id FROM thread_members WHERE channel_id = $1 AND user_id > $2 \
             ORDER BY user_id LIMIT $3",
        )
        .bind(i(channel.get()))
        .bind(after.map_or(0, |a| i(a.get())))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows.into_iter().map(|v| UserId::new(u(v))).collect())
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

    async fn list_recipients(&self, channel: ChannelId) -> RepoResult<Vec<UserId>> {
        let rows: Vec<i64> = sqlx::query_scalar(
            "SELECT user_id FROM channel_recipients WHERE channel_id = $1 ORDER BY user_id",
        )
        .bind(i(channel.get()))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows.into_iter().map(|v| UserId::new(u(v))).collect())
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
            let mut v = serde_json::json!({
                "message_id": r.message_id.to_string(),
                "channel_id": r.channel_id.to_string(),
            });
            // A forward keeps the copy it was made from (`message_reference.type = 1`).
            if let Some(f) = &r.forward
                && let Ok(s) = serde_json::to_value(f)
            {
                v["type"] = serde_json::json!(1);
                v["snapshot"] = s;
            }
            v
        });
        let poll_expires_ms = m
            .poll
            .as_ref()
            .filter(|p| !p.finalized)
            .and_then(|p| p.expiry_ms);
        let embeds = serde_json::to_value(&m.embeds).map_err(|e| RepoError(e.to_string()))?;
        let components =
            serde_json::to_value(&m.components).map_err(|e| RepoError(e.to_string()))?;
        let poll = m
            .poll
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(|e| RepoError(e.to_string()))?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "INSERT INTO messages (id, channel_id, guild_id, author_id, type, content, flags, \
             reference, embeds, components, sticker_ids, poll, mention_everyone, \
             mention_user_ids, mention_role_ids, mention_channel_ids, pinned, poll_expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, \
             to_timestamp($18::double precision / 1000.0))",
        )
        .bind(i(m.id.get()))
        .bind(i(m.channel_id.get()))
        .bind(m.guild_id.map(|g| i(g.get())))
        .bind(i(m.author_id.get()))
        .bind(m.kind.code())
        .bind(&m.content)
        .bind(i32::try_from(m.flags).unwrap_or(0))
        .bind(reference)
        .bind(embeds)
        .bind(components)
        .bind(m.sticker_ids.iter().map(|s| i(*s)).collect::<Vec<_>>())
        .bind(poll)
        .bind(m.mention_everyone)
        .bind(ids_i64(&m.mention_user_ids, UserId::get))
        .bind(ids_i64(&m.mention_role_ids, RoleId::get))
        .bind(ids_i64(&m.mention_channel_ids, ChannelId::get))
        .bind(m.pinned)
        .bind(poll_expires_ms.map(|ms| ms as f64))
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
        let mut tx = self.pool.begin().await.map_err(db)?;
        // A deleted message stops counting towards its thread (checked before the
        // visibility changes).
        sqlx::query(
            "UPDATE thread_metadata SET message_count = GREATEST(message_count - 1, 0) \
             WHERE channel_id = (SELECT channel_id FROM messages \
                                 WHERE id = $1 AND visibility = 'visible')",
        )
        .bind(i(plan.message_id.get()))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
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
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn last_sent_ms(&self, channel: ChannelId, user: UserId) -> RepoResult<Option<i64>> {
        // Snowflake ids sort by time, so the newest id carries the send time.
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT max(id) FROM messages WHERE channel_id = $1 AND author_id = $2 AND type IN (0, 19)",
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

    async fn hide_user_messages(
        &self,
        author: UserId,
        channels: &[ChannelId],
        since_ms: i64,
        now_ms: i64,
        purge_after_ms: i64,
    ) -> RepoResult<Vec<(ChannelId, Vec<MessageId>)>> {
        let channel_ids: Vec<i64> = channels.iter().map(|c| i(c.get())).collect();
        let mut tx = self.pool.begin().await.map_err(db)?;
        // Pins of the hidden messages go first; the message row keeps its pinned flag
        // cleared by the update below.
        sqlx::query(
            "DELETE FROM pins WHERE message_id IN ( \
                SELECT id FROM messages \
                WHERE author_id = $1 AND channel_id = ANY($2) AND visibility = 'visible' \
                  AND id >= sr_snowflake_floor(to_timestamp($3::double precision / 1000.0)))",
        )
        .bind(i(author.get()))
        .bind(&channel_ids)
        .bind(since_ms)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        let rows = sqlx::query(
            "UPDATE messages SET visibility = 'user_deleted', pinned = FALSE, \
             deleted_at = to_timestamp($4::double precision / 1000.0), \
             purge_after = CASE WHEN hold_id IS NULL \
                 THEN to_timestamp($5::double precision / 1000.0) END \
             WHERE author_id = $1 AND channel_id = ANY($2) AND visibility = 'visible' \
               AND id >= sr_snowflake_floor(to_timestamp($3::double precision / 1000.0)) \
             RETURNING id, channel_id",
        )
        .bind(i(author.get()))
        .bind(&channel_ids)
        .bind(since_ms)
        .bind(now_ms)
        .bind(purge_after_ms)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        let mut out: Vec<(ChannelId, Vec<MessageId>)> =
            channels.iter().map(|c| (*c, Vec::new())).collect();
        for r in &rows {
            let id = MessageId::new(u(r.try_get("id").map_err(db)?));
            let ch = ChannelId::new(u(r.try_get("channel_id").map_err(db)?));
            if let Some(slot) = out.iter_mut().find(|(c, _)| *c == ch) {
                slot.1.push(id);
            }
        }
        Ok(out)
    }

    async fn set_message_flags(
        &self,
        _channel: ChannelId,
        message: MessageId,
        flags: u32,
    ) -> RepoResult<()> {
        sqlx::query("UPDATE messages SET flags = $2 WHERE id = $1")
            .bind(i(message.get()))
            .bind(i32::try_from(flags).unwrap_or(0))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
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

/// Discord's attachment spoiler flag, kept in `attachments.flags`.
pub const ATTACHMENT_SPOILER_FLAG: i32 = 1 << 3;

const ATTACHMENT_COLS: &str = "id, channel_id, uploader_id, filename, size_bytes, storage_key, \
    declared_content_type, content_type, description, flags, message_id, \
    state::text AS state, (extract(epoch FROM expires_at) * 1000)::bigint AS expires_ms, \
    (extract(epoch FROM created_at) * 1000)::bigint AS created_ms, \
    round(duration_secs * 1000)::integer AS duration_ms, waveform, width, height, blurhash";

fn attachment_from(r: &PgRow) -> Result<Attachment, sqlx::Error> {
    let message_id: Option<i64> = r.try_get("message_id")?;
    let state_text: String = r.try_get("state")?;
    let state = match (message_id, state_text.as_str()) {
        (Some(_), _) => UploadState::Attached,
        (None, "pending") => UploadState::Pending,
        (None, _) => UploadState::Ready,
    };
    let flags: i32 = r.try_get("flags")?;
    Ok(Attachment {
        id: AttachmentId::new(u(r.try_get("id")?)),
        channel_id: ChannelId::new(u(r.try_get::<Option<i64>, _>("channel_id")?.unwrap_or(0))),
        uploader: UserId::new(u(r.try_get("uploader_id")?)),
        filename: r.try_get("filename")?,
        size: u(r.try_get("size_bytes")?),
        key: r.try_get("storage_key")?,
        declared_content_type: r.try_get("declared_content_type")?,
        content_type: r.try_get("content_type")?,
        description: r.try_get("description")?,
        spoiler: flags & ATTACHMENT_SPOILER_FLAG != 0,
        state,
        message_id: message_id.map(|m| MessageId::new(u(m))),
        expires_at_ms: r.try_get("expires_ms")?,
        created_at_ms: r.try_get("created_ms")?,
        duration_ms: r
            .try_get::<Option<i32>, _>("duration_ms")?
            .and_then(|v| u32::try_from(v).ok()),
        waveform: r.try_get("waveform")?,
        width: r
            .try_get::<Option<i32>, _>("width")?
            .and_then(|v| u32::try_from(v).ok()),
        height: r
            .try_get::<Option<i32>, _>("height")?
            .and_then(|v| u32::try_from(v).ok()),
        blurhash: r.try_get("blurhash")?,
    })
}

/// Milliseconds as the `attachments.duration_secs` REAL column.
fn duration_secs(ms: u32) -> f32 {
    ms as f32 / 1000.0
}

fn sdb(e: sqlx::Error) -> StoreError {
    StoreError(e.to_string())
}

fn upload_state_text(s: UploadState) -> &'static str {
    match s {
        UploadState::Pending => "pending",
        UploadState::Ready | UploadState::Attached => "ready",
    }
}

/// Postgres-backed [`MessageStore`]: history paging, nonces, pins, reactions and
/// attachment rows (open question 148). Tests are `#[ignore]` and need
/// `SR_TEST_DATABASE_URL`.
#[derive(Clone)]
pub struct PgMessageStore {
    pool: PgPool,
}

impl PgMessageStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MessageStore for PgMessageStore {
    async fn fetch(&self, channel: ChannelId, page: Page) -> StoreResult<Vec<Message>> {
        let order = if page.ascending { "ASC" } else { "DESC" };
        let sql = format!(
            "SELECT {MSG_COLS} FROM messages WHERE channel_id = $1 AND visibility = 'visible' \
             AND ($2::bigint IS NULL OR id < $2) AND ($3::bigint IS NULL OR id > $3) \
             ORDER BY id {order} LIMIT $4"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(channel.get()))
            .bind(page.before.map(|m| i(m.get())))
            .bind(page.after.map(|m| i(m.get())))
            .bind(i64::try_from(page.limit).unwrap_or(i64::MAX))
            .fetch_all(&self.pool)
            .await
            .map_err(sdb)?;
        rows.iter()
            .map(|r| message_from(r).map_err(StoreError::from))
            .collect()
    }

    async fn list_pins(&self, channel: ChannelId) -> StoreResult<Vec<(Message, i64)>> {
        let sql = format!(
            "SELECT {MSG_COLS}, (SELECT (extract(epoch FROM p.pinned_at) * 1000)::bigint \
             FROM pins p WHERE p.channel_id = messages.channel_id \
             AND p.message_id = messages.id) AS pinned_ms \
             FROM messages WHERE channel_id = $1 AND visibility = 'visible' AND pinned \
             ORDER BY pinned_ms DESC NULLS LAST"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(channel.get()))
            .fetch_all(&self.pool)
            .await
            .map_err(sdb)?;
        rows.iter()
            .map(|r| {
                let m = message_from(r).map_err(StoreError::from)?;
                let at: i64 = r.try_get("pinned_ms").map_err(sdb)?;
                Ok((m, at))
            })
            .collect()
    }

    async fn find_nonce(
        &self,
        channel: ChannelId,
        author: UserId,
        nonce: &str,
        since_ms: i64,
    ) -> StoreResult<Option<MessageId>> {
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM messages WHERE channel_id = $1 AND author_id = $2 AND nonce = $3 \
             AND id >= sr_snowflake_floor(to_timestamp($4::double precision / 1000.0)) \
             ORDER BY id DESC LIMIT 1",
        )
        .bind(i(channel.get()))
        .bind(i(author.get()))
        .bind(nonce)
        .bind(since_ms)
        .fetch_optional(&self.pool)
        .await
        .map_err(sdb)?;
        Ok(id.map(|v| MessageId::new(u(v))))
    }

    async fn put_nonce(
        &self,
        channel: ChannelId,
        _author: UserId,
        nonce: &str,
        message: MessageId,
        _now_ms: i64,
    ) -> StoreResult<()> {
        sqlx::query("UPDATE messages SET nonce = $3 WHERE channel_id = $1 AND id = $2")
            .bind(i(channel.get()))
            .bind(i(message.get()))
            .bind(nonce)
            .execute(&self.pool)
            .await
            .map_err(sdb)?;
        Ok(())
    }

    async fn nonce_of(&self, message: MessageId) -> StoreResult<Option<String>> {
        let v: Option<Option<String>> =
            sqlx::query_scalar("SELECT nonce FROM messages WHERE id = $1")
                .bind(i(message.get()))
                .fetch_optional(&self.pool)
                .await
                .map_err(sdb)?;
        Ok(v.flatten())
    }

    async fn add_reaction(
        &self,
        _channel: ChannelId,
        message: MessageId,
        user: UserId,
        emoji: &EmojiKey,
        now_ms: i64,
    ) -> StoreResult<bool> {
        let res = sqlx::query(
            "INSERT INTO reactions (message_id, emoji_key, user_id, created_at) \
             VALUES ($1, $2, $3, to_timestamp($4::double precision / 1000.0)) \
             ON CONFLICT DO NOTHING",
        )
        .bind(i(message.get()))
        .bind(emoji.key())
        .bind(i(user.get()))
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(sdb)?;
        Ok(res.rows_affected() == 1)
    }

    async fn remove_reaction(
        &self,
        message: MessageId,
        user: UserId,
        emoji: &EmojiKey,
    ) -> StoreResult<bool> {
        let res = sqlx::query(
            "DELETE FROM reactions WHERE message_id = $1 AND emoji_key = $2 AND user_id = $3",
        )
        .bind(i(message.get()))
        .bind(emoji.key())
        .bind(i(user.get()))
        .execute(&self.pool)
        .await
        .map_err(sdb)?;
        Ok(res.rows_affected() > 0)
    }

    async fn clear_reactions(&self, message: MessageId) -> StoreResult<()> {
        sqlx::query("DELETE FROM reactions WHERE message_id = $1")
            .bind(i(message.get()))
            .execute(&self.pool)
            .await
            .map_err(sdb)?;
        Ok(())
    }

    async fn clear_emoji(&self, message: MessageId, emoji: &EmojiKey) -> StoreResult<bool> {
        let res = sqlx::query("DELETE FROM reactions WHERE message_id = $1 AND emoji_key = $2")
            .bind(i(message.get()))
            .bind(emoji.key())
            .execute(&self.pool)
            .await
            .map_err(sdb)?;
        Ok(res.rows_affected() > 0)
    }

    async fn reactions_for(
        &self,
        messages: &[MessageId],
        viewer: UserId,
    ) -> StoreResult<HashMap<MessageId, Vec<ReactionSummary>>> {
        let ids: Vec<i64> = messages.iter().map(|m| i(m.get())).collect();
        let rows = sqlx::query(
            "SELECT message_id, emoji_key, count(*) AS n, bool_or(user_id = $2) AS me \
             FROM reactions WHERE message_id = ANY($1) \
             GROUP BY message_id, emoji_key ORDER BY message_id, min(created_at), emoji_key",
        )
        .bind(ids)
        .bind(i(viewer.get()))
        .fetch_all(&self.pool)
        .await
        .map_err(sdb)?;
        let mut out: HashMap<MessageId, Vec<ReactionSummary>> = HashMap::new();
        for r in &rows {
            let key: String = r.try_get("emoji_key").map_err(sdb)?;
            let Some(emoji) = EmojiKey::parse(&key) else {
                continue;
            };
            let mid = MessageId::new(u(r.try_get("message_id").map_err(sdb)?));
            let n: i64 = r.try_get("n").map_err(sdb)?;
            out.entry(mid).or_default().push(ReactionSummary {
                emoji,
                count: u32::try_from(n).unwrap_or(u32::MAX),
                me: r.try_get("me").map_err(sdb)?,
            });
        }
        Ok(out)
    }

    async fn reaction_users(
        &self,
        message: MessageId,
        emoji: &EmojiKey,
        after: Option<UserId>,
        limit: usize,
    ) -> StoreResult<Vec<UserId>> {
        let rows: Vec<i64> = sqlx::query_scalar(
            "SELECT user_id FROM reactions WHERE message_id = $1 AND emoji_key = $2 \
             AND ($3::bigint IS NULL OR user_id > $3) ORDER BY user_id LIMIT $4",
        )
        .bind(i(message.get()))
        .bind(emoji.key())
        .bind(after.map(|u| i(u.get())))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(sdb)?;
        Ok(rows.into_iter().map(|v| UserId::new(u(v))).collect())
    }

    async fn insert_attachment(&self, a: &Attachment) -> StoreResult<()> {
        let expires_secs = a.expires_at_ms.map(|ms| ms as f64 / 1000.0);
        sqlx::query(
            "INSERT INTO attachments (id, channel_id, uploader_id, filename, size_bytes, \
             storage_key, declared_content_type, content_type, description, flags, is_large, \
             expires_at, state, duration_secs, waveform, width, height, blurhash) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, \
             to_timestamp($12::double precision), $13::attachment_state, $14, $15, $16, $17, $18)",
        )
        .bind(i(a.id.get()))
        .bind(i(a.channel_id.get()))
        .bind(i(a.uploader.get()))
        .bind(&a.filename)
        .bind(i64::try_from(a.size).unwrap_or(i64::MAX))
        .bind(&a.key)
        .bind(a.declared_content_type.as_deref())
        .bind(a.content_type.as_deref())
        .bind(a.description.as_deref())
        .bind(if a.spoiler {
            ATTACHMENT_SPOILER_FLAG
        } else {
            0
        })
        .bind(a.expires_at_ms.is_some())
        .bind(expires_secs)
        .bind(upload_state_text(a.state))
        .bind(a.duration_ms.map(duration_secs))
        .bind(a.waveform.as_deref())
        .bind(a.width.and_then(|v| i32::try_from(v).ok()))
        .bind(a.height.and_then(|v| i32::try_from(v).ok()))
        .bind(a.blurhash.as_deref())
        .execute(&self.pool)
        .await
        .map_err(sdb)?;
        Ok(())
    }

    async fn get_attachments(&self, ids: &[AttachmentId]) -> StoreResult<Vec<Attachment>> {
        let ids: Vec<i64> = ids.iter().map(|a| i(a.get())).collect();
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {ATTACHMENT_COLS} FROM attachments WHERE id = ANY($1)"
        )))
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(sdb)?;
        rows.iter()
            .map(|r| attachment_from(r).map_err(sdb))
            .collect()
    }

    async fn update_attachment(&self, a: &Attachment) -> StoreResult<()> {
        let expires_secs = a.expires_at_ms.map(|ms| ms as f64 / 1000.0);
        sqlx::query(
            "UPDATE attachments SET message_id = $2, state = $3::attachment_state, \
             description = $4, content_type = $5, flags = $6, \
             expires_at = to_timestamp($7::double precision), size_bytes = $8, \
             duration_secs = $9, waveform = $10, updated_at = now() \
             WHERE id = $1",
        )
        .bind(i(a.id.get()))
        .bind(a.message_id.map(|m| i(m.get())))
        .bind(upload_state_text(a.state))
        .bind(a.description.as_deref())
        .bind(a.content_type.as_deref())
        .bind(if a.spoiler {
            ATTACHMENT_SPOILER_FLAG
        } else {
            0
        })
        .bind(expires_secs)
        // The EXIF/metadata strip rewrites the object, so the stored size must follow it.
        .bind(i(a.size))
        .bind(a.duration_ms.map(duration_secs))
        .bind(a.waveform.as_deref())
        .execute(&self.pool)
        .await
        .map_err(sdb)?;
        Ok(())
    }

    async fn attachments_of(
        &self,
        messages: &[MessageId],
    ) -> StoreResult<HashMap<MessageId, Vec<Attachment>>> {
        let ids: Vec<i64> = messages.iter().map(|m| i(m.get())).collect();
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {ATTACHMENT_COLS} FROM attachments WHERE message_id = ANY($1) ORDER BY id"
        )))
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(sdb)?;
        let mut out: HashMap<MessageId, Vec<Attachment>> = HashMap::new();
        for r in &rows {
            let a = attachment_from(r).map_err(sdb)?;
            if let Some(m) = a.message_id {
                out.entry(m).or_default().push(a);
            }
        }
        Ok(out)
    }
}

#[async_trait]
impl UserLookup for PgChatRepo {
    async fn users(&self, ids: &[UserId]) -> RepoResult<Vec<UserSummary>> {
        let ids: Vec<i64> = ids.iter().map(|u| i(u.get())).collect();
        // The server tag shows only while the user still belongs to that server, the
        // server still has a tag, and the user kept showing it (LNG-GLD-011).
        let rows = sqlx::query(
            "SELECT a.id, a.username::text AS username, a.display_name, a.avatar_key, \
             a.banner_key, a.is_bot, pg.id AS pg_id, pg.tag AS pg_tag, pg.tag_badge AS pg_badge \
             FROM accounts a \
             LEFT JOIN guilds pg ON pg.id = a.primary_guild_id AND a.primary_guild_enabled \
                  AND pg.tag IS NOT NULL \
                  AND EXISTS (SELECT 1 FROM guild_members gm \
                              WHERE gm.guild_id = pg.id AND gm.user_id = a.id) \
             WHERE a.id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let primary_guild = match r.try_get::<Option<i64>, _>("pg_id").map_err(db)? {
                Some(g) => Some(sr_chat::server_tag::PrimaryGuild {
                    guild_id: GuildId::new(u(g)),
                    tag: r.try_get("pg_tag").map_err(db)?,
                    badge: r.try_get("pg_badge").map_err(db)?,
                }),
                None => None,
            };
            out.push(UserSummary {
                id: UserId::new(u(r.try_get("id").map_err(db)?)),
                username: r.try_get("username").map_err(db)?,
                display_name: r.try_get("display_name").map_err(db)?,
                avatar_key: r.try_get("avatar_key").map_err(db)?,
                banner_key: r.try_get("banner_key").map_err(db)?,
                bot: r.try_get("is_bot").map_err(db)?,
                primary_guild,
            });
        }
        Ok(out)
    }
}

/// Upload slots still pending after `cutoff_ms` (Unix ms) that no message references
/// (open question 153). Returns the id and storage key of each.
pub async fn stale_pending_attachments(
    pool: &PgPool,
    cutoff_ms: i64,
    limit: i64,
) -> crate::DbResult<Vec<(AttachmentId, String)>> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, storage_key FROM attachments \
         WHERE state = 'pending' AND message_id IS NULL AND hold_id IS NULL \
           AND created_at < to_timestamp($1::double precision / 1000.0) \
         ORDER BY created_at LIMIT $2",
    )
    .bind(cutoff_ms)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, key)| (AttachmentId::new(u(id)), key))
        .collect())
}

/// Delete attachment rows by id; returns how many were removed.
pub async fn delete_attachment_rows(pool: &PgPool, ids: &[AttachmentId]) -> crate::DbResult<u64> {
    let ids: Vec<i64> = ids.iter().map(|a| i(a.get())).collect();
    let res = sqlx::query(
        "DELETE FROM attachments WHERE id = ANY($1) AND message_id IS NULL AND hold_id IS NULL",
    )
    .bind(ids)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}
