//! Postgres implementation of the `sr-chat` settings repository traits
//! (migrations 0030-0036). Queries are runtime-checked; timestamps cross the
//! boundary as Unix milliseconds, like `chat_repo`. Tests are `#[ignore]` and need
//! `SR_TEST_DATABASE_URL` (see `tests/settings_repo.rs`).

use async_trait::async_trait;
use sqlx::postgres::PgRow;
use sqlx::types::Json;
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};

use sr_chat::error::RepoError;
use sr_chat::repo::RepoResult;
use sr_chat::settings::applications::{
    Answer, Application, ApplicationForm, ApplicationStatus, Question,
};
use sr_chat::settings::audit::{AuditAction, AuditChange, AuditEntry, AuditQuery};
use sr_chat::settings::automod::{Action, AutoModRule, EventType, Trigger};
use sr_chat::settings::community::{
    CommunitySettings, Onboarding, OnboardingPrompt, WelcomeChannel, WelcomeScreen,
};
use sr_chat::settings::expressions::{Emoji, Sticker, StickerFormat};
use sr_chat::settings::guide::{Guide, GuideResource, GuideTodo};
use sr_chat::settings::insights::{Insights, InsightsDay, TOP_CHANNELS, TopChannel};
use sr_chat::settings::invites::GuildInvite;
use sr_chat::settings::repo::{
    ApplicationRepository, AuditRepository, AutoModRepository, CommunityRepository,
    ExpressionRepository, GuideRepository, InsightsRepository, InviteRepository,
    ScheduledEventRepository, TagRepository, TemplateRepository, WebhookRepository,
};
use sr_chat::settings::scheduled_events::{EntityType, EventStatus, ScheduledEvent};
use sr_chat::settings::soundboard::{Sound, SoundFormat};
use sr_chat::settings::tags::GuildTag;
use sr_chat::settings::templates::{GuildTemplate, TemplateSnapshot};
use sr_chat::settings::webhooks::{Webhook, WebhookKind};
use sr_core::{
    ApplicationId, ChannelId, EmojiId, GuildId, RoleId, SoundId, StickerId, UserId, WebhookId,
};

/// Postgres-backed settings repository.
#[derive(Clone)]
pub struct PgSettingsRepo {
    pool: PgPool,
}

impl PgSettingsRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn db(e: sqlx::Error) -> RepoError {
    RepoError(e.to_string())
}

fn bad(what: &str) -> RepoError {
    RepoError(format!("unexpected value in column {what}"))
}

// Ids are positive Snowflakes (< 2^63), so these casts are lossless.
fn i(id: u64) -> i64 {
    id as i64
}
fn u(v: i64) -> u64 {
    v as u64
}

fn ids_out(v: Vec<i64>) -> impl Iterator<Item = u64> {
    v.into_iter().map(u)
}

fn ids_in(v: impl IntoIterator<Item = u64>) -> Vec<i64> {
    v.into_iter().map(i).collect()
}

const MS: &str = "(extract(epoch FROM {c}) * 1000)::bigint";

fn ms(col: &str) -> String {
    MS.replace("{c}", col)
}

fn get<'r, T>(r: &'r PgRow, col: &str) -> RepoResult<T>
where
    T: sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres>,
{
    r.try_get(col).map_err(db)
}

// ---- invites ------------------------------------------------------------------

pub(crate) fn invite_cols() -> String {
    format!(
        "code, guild_id, channel_id, inviter_id, max_age_secs, max_uses, uses, temporary, \
         {} AS created_ms, {} AS revoked_ms",
        ms("created_at"),
        ms("revoked_at")
    )
}

pub(crate) fn invite_from(r: &PgRow) -> RepoResult<GuildInvite> {
    Ok(GuildInvite {
        code: get(r, "code")?,
        guild_id: GuildId::new(u(get(r, "guild_id")?)),
        channel_id: ChannelId::new(u(get(r, "channel_id")?)),
        inviter_id: UserId::new(u(get(r, "inviter_id")?)),
        max_age_secs: u32::try_from(get::<i32>(r, "max_age_secs")?)
            .map_err(|_| bad("max_age_secs"))?,
        max_uses: u32::try_from(get::<i32>(r, "max_uses")?).map_err(|_| bad("max_uses"))?,
        uses: u32::try_from(get::<i32>(r, "uses")?).map_err(|_| bad("uses"))?,
        temporary: get(r, "temporary")?,
        created_at_ms: get(r, "created_ms")?,
        revoked_at_ms: get(r, "revoked_ms")?,
    })
}

/// Usable at `$2` (ms): not revoked, not expired, uses left.
pub(crate) const INVITE_USABLE: &str = "revoked_at IS NULL \
    AND (max_age_secs = 0 OR created_at + make_interval(secs => max_age_secs) \
         > to_timestamp($2::double precision / 1000.0)) \
    AND (max_uses = 0 OR uses < max_uses)";

#[async_trait]
impl InviteRepository for PgSettingsRepo {
    async fn insert_invite(&self, inv: &GuildInvite) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO guild_invites (code, guild_id, channel_id, inviter_id, max_age_secs, \
             max_uses, uses, temporary, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, to_timestamp($9::double precision / 1000.0))",
        )
        .bind(&inv.code)
        .bind(i(inv.guild_id.get()))
        .bind(i(inv.channel_id.get()))
        .bind(i(inv.inviter_id.get()))
        .bind(inv.max_age_secs as i32)
        .bind(inv.max_uses as i32)
        .bind(inv.uses as i32)
        .bind(inv.temporary)
        .bind(inv.created_at_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn get_invite(&self, code: &str) -> RepoResult<Option<GuildInvite>> {
        let sql = format!(
            "SELECT {} FROM guild_invites WHERE code = $1",
            invite_cols()
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(code)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(invite_from)
            .transpose()
    }

    async fn list_guild_invites(&self, guild: GuildId) -> RepoResult<Vec<GuildInvite>> {
        let sql = format!(
            "SELECT {} FROM guild_invites WHERE guild_id = $1 ORDER BY created_at DESC",
            invite_cols()
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(guild.get()))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(invite_from)
            .collect()
    }

    async fn count_active_invites(&self, guild: GuildId, now_ms: i64) -> RepoResult<usize> {
        let sql =
            format!("SELECT count(*) FROM guild_invites WHERE guild_id = $1 AND {INVITE_USABLE}");
        let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind(i(guild.get()))
            .bind(now_ms)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        Ok(usize::try_from(n).unwrap_or(0))
    }

    async fn revoke_invite(&self, code: &str, now_ms: i64) -> RepoResult<bool> {
        let r = sqlx::query(
            "UPDATE guild_invites SET revoked_at = to_timestamp($2::double precision / 1000.0) \
             WHERE code = $1 AND revoked_at IS NULL",
        )
        .bind(code)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(r.rows_affected() > 0)
    }

    async fn consume_invite(&self, code: &str, now_ms: i64) -> RepoResult<Option<GuildInvite>> {
        // One conditional UPDATE keeps max_uses exact under concurrent joins.
        let sql = format!(
            "UPDATE guild_invites SET uses = uses + 1 WHERE code = $1 AND {INVITE_USABLE} \
             RETURNING {}",
            invite_cols()
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(code)
            .bind(now_ms)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(invite_from)
            .transpose()
    }

    async fn purge_dead_invites(&self, before_ms: i64) -> RepoResult<u64> {
        let r = sqlx::query(
            "DELETE FROM guild_invites \
             WHERE revoked_at < to_timestamp($1::double precision / 1000.0) \
                OR (max_age_secs > 0 AND created_at + make_interval(secs => max_age_secs) \
                    < to_timestamp($1::double precision / 1000.0))",
        )
        .bind(before_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(r.rows_affected())
    }

    async fn is_banned(&self, guild: GuildId, user: UserId) -> RepoResult<bool> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM guild_bans WHERE guild_id = $1 AND user_id = $2)",
        )
        .bind(i(guild.get()))
        .bind(i(user.get()))
        .fetch_one(&self.pool)
        .await
        .map_err(db)
    }

    async fn vanity_uses(&self, guild: GuildId) -> RepoResult<u32> {
        let n: Option<i32> = sqlx::query_scalar("SELECT vanity_uses FROM guilds WHERE id = $1")
            .bind(i(guild.get()))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        Ok(u32::try_from(n.unwrap_or(0)).unwrap_or(0))
    }
}

// ---- audit log ----------------------------------------------------------------

fn audit_cols() -> String {
    format!(
        "id, guild_id, actor_id, target_id, action, changes, options, reason, {} AS created_ms",
        ms("created_at")
    )
}

fn audit_from(r: &PgRow) -> RepoResult<AuditEntry> {
    let changes: Json<Vec<AuditChange>> = get(r, "changes")?;
    let options: Option<serde_json::Value> = get(r, "options")?;
    Ok(AuditEntry {
        id: u(get(r, "id")?),
        guild_id: GuildId::new(u(get(r, "guild_id")?)),
        actor_id: UserId::new(u(get(r, "actor_id")?)),
        target_id: get::<Option<i64>>(r, "target_id")?.map(u),
        action: AuditAction::from_code(get(r, "action")?).ok_or_else(|| bad("action"))?,
        changes: changes.0,
        options,
        reason: get(r, "reason")?,
        created_at_ms: get(r, "created_ms")?,
    })
}

#[async_trait]
impl AuditRepository for PgSettingsRepo {
    async fn append_audit(&self, e: &AuditEntry) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO audit_log_entries (id, guild_id, actor_id, target_id, action, changes, \
             options, reason, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, to_timestamp($9::double precision / 1000.0))",
        )
        .bind(i(e.id))
        .bind(i(e.guild_id.get()))
        .bind(i(e.actor_id.get()))
        .bind(e.target_id.map(i))
        .bind(e.action.code())
        .bind(Json(&e.changes))
        .bind(e.options.as_ref())
        .bind(e.reason.as_deref())
        .bind(e.created_at_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn list_audit(&self, q: &AuditQuery) -> RepoResult<Vec<AuditEntry>> {
        let sql = format!(
            "SELECT {} FROM audit_log_entries \
             WHERE guild_id = $1 \
               AND ($2::bigint IS NULL OR actor_id = $2) \
               AND ($3::smallint IS NULL OR action = $3) \
               AND ($4::bigint IS NULL OR id < $4) \
             ORDER BY id DESC LIMIT $5",
            audit_cols()
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(q.guild_id.get()))
            .bind(q.actor_id.map(|a| i(a.get())))
            .bind(q.action.map(AuditAction::code))
            .bind(q.before.map(i))
            .bind(q.clamped_limit() as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(audit_from)
            .collect()
    }

    async fn purge_audit_before(&self, cutoff_ms: i64) -> RepoResult<u64> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        // The delete trigger only allows removal inside the retention job.
        sqlx::query("SELECT set_config('sr.audit_purge', 'on', true)")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let r = sqlx::query(
            "DELETE FROM audit_log_entries \
             WHERE created_at < to_timestamp($1::double precision / 1000.0)",
        )
        .bind(cutoff_ms)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(r.rows_affected())
    }
}

// ---- emoji and stickers --------------------------------------------------------

fn emoji_cols() -> String {
    format!(
        "id, guild_id, name, animated, available, creator_id, media_key, \
         {} AS created_ms, \
         COALESCE((SELECT array_agg(role_id) FROM emoji_roles WHERE emoji_id = emojis.id), '{{}}') AS role_ids",
        ms("created_at")
    )
}

fn emoji_from(r: &PgRow) -> RepoResult<Emoji> {
    Ok(Emoji {
        id: EmojiId::new(u(get(r, "id")?)),
        guild_id: GuildId::new(u(get(r, "guild_id")?)),
        name: get(r, "name")?,
        animated: get(r, "animated")?,
        available: get(r, "available")?,
        role_ids: ids_out(get(r, "role_ids")?).map(RoleId::new).collect(),
        creator_id: UserId::new(u(get(r, "creator_id")?)),
        media_key: get(r, "media_key")?,
        created_at_ms: get(r, "created_ms")?,
    })
}

fn sticker_cols() -> String {
    format!(
        "id, guild_id, name, description, tags, format_type, available, creator_id, media_key, \
         {} AS created_ms",
        ms("created_at")
    )
}

fn sticker_from(r: &PgRow) -> RepoResult<Sticker> {
    Ok(Sticker {
        id: StickerId::new(u(get(r, "id")?)),
        guild_id: GuildId::new(u(get(r, "guild_id")?)),
        name: get(r, "name")?,
        description: get(r, "description")?,
        tags: get(r, "tags")?,
        format: StickerFormat::from_code(get(r, "format_type")?)
            .ok_or_else(|| bad("format_type"))?,
        available: get(r, "available")?,
        creator_id: UserId::new(u(get(r, "creator_id")?)),
        media_key: get(r, "media_key")?,
        created_at_ms: get(r, "created_ms")?,
    })
}

fn sound_cols() -> String {
    format!(
        "id, guild_id, name, volume, emoji_id, emoji_name, available, creator_id, media_key, \
         format_type, duration_ms, {} AS created_ms",
        ms("created_at")
    )
}

fn sound_from(r: &PgRow) -> RepoResult<Sound> {
    let emoji_id: Option<i64> = get(r, "emoji_id")?;
    let duration: i32 = get(r, "duration_ms")?;
    Ok(Sound {
        id: SoundId::new(u(get(r, "id")?)),
        guild_id: GuildId::new(u(get(r, "guild_id")?)),
        name: get(r, "name")?,
        volume: get(r, "volume")?,
        emoji_id: emoji_id.map(|e| EmojiId::new(u(e))),
        emoji_name: get(r, "emoji_name")?,
        available: get(r, "available")?,
        creator_id: UserId::new(u(get(r, "creator_id")?)),
        media_key: get(r, "media_key")?,
        format: SoundFormat::from_code(get(r, "format_type")?).ok_or_else(|| bad("format_type"))?,
        duration_ms: u32::try_from(duration).unwrap_or(0),
        created_at_ms: get(r, "created_ms")?,
    })
}

#[async_trait]
impl ExpressionRepository for PgSettingsRepo {
    async fn list_emoji(&self, guild: GuildId) -> RepoResult<Vec<Emoji>> {
        let sql = format!(
            "SELECT {} FROM emojis WHERE guild_id = $1 ORDER BY created_at, id",
            emoji_cols()
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(guild.get()))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(emoji_from)
            .collect()
    }

    async fn get_emoji(&self, guild: GuildId, id: EmojiId) -> RepoResult<Option<Emoji>> {
        let sql = format!(
            "SELECT {} FROM emojis WHERE guild_id = $1 AND id = $2",
            emoji_cols()
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(guild.get()))
            .bind(i(id.get()))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(emoji_from)
            .transpose()
    }

    async fn insert_emoji(&self, e: &Emoji) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "INSERT INTO emojis (id, guild_id, name, animated, available, creator_id, media_key, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8::double precision / 1000.0))",
        )
        .bind(i(e.id.get()))
        .bind(i(e.guild_id.get()))
        .bind(&e.name)
        .bind(e.animated)
        .bind(e.available)
        .bind(i(e.creator_id.get()))
        .bind(&e.media_key)
        .bind(e.created_at_ms)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        set_emoji_roles(&mut tx, e).await?;
        tx.commit().await.map_err(db)
    }

    async fn update_emoji(&self, e: &Emoji) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "UPDATE emojis SET name = $2, available = $3, updated_at = now() WHERE id = $1",
        )
        .bind(i(e.id.get()))
        .bind(&e.name)
        .bind(e.available)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query("DELETE FROM emoji_roles WHERE emoji_id = $1")
            .bind(i(e.id.get()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        set_emoji_roles(&mut tx, e).await?;
        tx.commit().await.map_err(db)
    }

    async fn delete_emoji(&self, guild: GuildId, id: EmojiId) -> RepoResult<bool> {
        let r = sqlx::query("DELETE FROM emojis WHERE guild_id = $1 AND id = $2")
            .bind(i(guild.get()))
            .bind(i(id.get()))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(r.rows_affected() > 0)
    }

    async fn list_stickers(&self, guild: GuildId) -> RepoResult<Vec<Sticker>> {
        let sql = format!(
            "SELECT {} FROM stickers WHERE guild_id = $1 ORDER BY created_at, id",
            sticker_cols()
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(guild.get()))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(sticker_from)
            .collect()
    }

    async fn get_sticker(&self, guild: GuildId, id: StickerId) -> RepoResult<Option<Sticker>> {
        let sql = format!(
            "SELECT {} FROM stickers WHERE guild_id = $1 AND id = $2",
            sticker_cols()
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(guild.get()))
            .bind(i(id.get()))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(sticker_from)
            .transpose()
    }

    async fn insert_sticker(&self, s: &Sticker) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO stickers (id, guild_id, name, description, tags, format_type, available, \
             creator_id, media_key, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, to_timestamp($10::double precision / 1000.0))",
        )
        .bind(i(s.id.get()))
        .bind(i(s.guild_id.get()))
        .bind(&s.name)
        .bind(s.description.as_deref())
        .bind(&s.tags)
        .bind(s.format.code())
        .bind(s.available)
        .bind(i(s.creator_id.get()))
        .bind(&s.media_key)
        .bind(s.created_at_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn update_sticker(&self, s: &Sticker) -> RepoResult<()> {
        sqlx::query(
            "UPDATE stickers SET name = $2, description = $3, tags = $4, available = $5, \
             updated_at = now() WHERE id = $1",
        )
        .bind(i(s.id.get()))
        .bind(&s.name)
        .bind(s.description.as_deref())
        .bind(&s.tags)
        .bind(s.available)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_sticker(&self, guild: GuildId, id: StickerId) -> RepoResult<bool> {
        let r = sqlx::query("DELETE FROM stickers WHERE guild_id = $1 AND id = $2")
            .bind(i(guild.get()))
            .bind(i(id.get()))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(r.rows_affected() > 0)
    }

    async fn list_sounds(&self, guild: GuildId) -> RepoResult<Vec<Sound>> {
        let sql = format!(
            "SELECT {} FROM soundboard_sounds WHERE guild_id = $1 ORDER BY created_at, id",
            sound_cols()
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(guild.get()))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(sound_from)
            .collect()
    }

    async fn get_sound(&self, guild: GuildId, id: SoundId) -> RepoResult<Option<Sound>> {
        let sql = format!(
            "SELECT {} FROM soundboard_sounds WHERE guild_id = $1 AND id = $2",
            sound_cols()
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(guild.get()))
            .bind(i(id.get()))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(sound_from)
            .transpose()
    }

    async fn insert_sound(&self, s: &Sound) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO soundboard_sounds (id, guild_id, name, volume, emoji_id, emoji_name, \
             available, creator_id, media_key, format_type, duration_ms, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, \
             to_timestamp($12::double precision / 1000.0))",
        )
        .bind(i(s.id.get()))
        .bind(i(s.guild_id.get()))
        .bind(&s.name)
        .bind(s.volume)
        .bind(s.emoji_id.map(|e| i(e.get())))
        .bind(s.emoji_name.as_deref())
        .bind(s.available)
        .bind(i(s.creator_id.get()))
        .bind(&s.media_key)
        .bind(s.format.code())
        .bind(i32::try_from(s.duration_ms).unwrap_or(i32::MAX))
        .bind(s.created_at_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn update_sound(&self, s: &Sound) -> RepoResult<()> {
        sqlx::query(
            "UPDATE soundboard_sounds SET name = $2, volume = $3, emoji_id = $4, emoji_name = $5, \
             available = $6, updated_at = now() WHERE id = $1",
        )
        .bind(i(s.id.get()))
        .bind(&s.name)
        .bind(s.volume)
        .bind(s.emoji_id.map(|e| i(e.get())))
        .bind(s.emoji_name.as_deref())
        .bind(s.available)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_sound(&self, guild: GuildId, id: SoundId) -> RepoResult<bool> {
        let r = sqlx::query("DELETE FROM soundboard_sounds WHERE guild_id = $1 AND id = $2")
            .bind(i(guild.get()))
            .bind(i(id.get()))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(r.rows_affected() > 0)
    }
}

async fn set_emoji_roles(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    e: &Emoji,
) -> RepoResult<()> {
    if e.role_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO emoji_roles (emoji_id, role_id) \
         SELECT $1, unnest($2::bigint[]) ON CONFLICT DO NOTHING",
    )
    .bind(i(e.id.get()))
    .bind(ids_in(e.role_ids.iter().map(|r| r.get())))
    .execute(&mut **tx)
    .await
    .map_err(db)?;
    Ok(())
}

// ---- AutoMod -------------------------------------------------------------------

fn event_str(e: EventType) -> &'static str {
    match e {
        EventType::MessageSend => "message_send",
        EventType::MemberUpdate => "member_update",
    }
}

fn event_parse(s: &str) -> RepoResult<EventType> {
    match s {
        "message_send" => Ok(EventType::MessageSend),
        "member_update" => Ok(EventType::MemberUpdate),
        _ => Err(bad("event_type")),
    }
}

fn rule_from(r: &PgRow) -> RepoResult<AutoModRule> {
    let trigger: Json<Trigger> = get(r, "trigger")?;
    let actions: Json<Vec<Action>> = get(r, "actions")?;
    Ok(AutoModRule {
        id: u(get(r, "id")?),
        guild_id: GuildId::new(u(get(r, "guild_id")?)),
        name: get(r, "name")?,
        creator_id: UserId::new(u(get(r, "creator_id")?)),
        event_type: event_parse(&get::<String>(r, "event_type")?)?,
        trigger: trigger.0,
        actions: actions.0,
        enabled: get(r, "enabled")?,
        exempt_role_ids: ids_out(get(r, "exempt_role_ids")?)
            .map(RoleId::new)
            .collect(),
        exempt_channel_ids: ids_out(get(r, "exempt_channel_ids")?)
            .map(ChannelId::new)
            .collect(),
    })
}

const RULE_COLS: &str = "id, guild_id, name, creator_id, event_type, trigger, actions, enabled, \
    exempt_role_ids, exempt_channel_ids";

#[async_trait]
impl AutoModRepository for PgSettingsRepo {
    async fn list_rules(&self, guild: GuildId) -> RepoResult<Vec<AutoModRule>> {
        let sql = format!("SELECT {RULE_COLS} FROM automod_rules WHERE guild_id = $1 ORDER BY id");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(guild.get()))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(rule_from)
            .collect()
    }

    async fn get_rule(&self, guild: GuildId, id: u64) -> RepoResult<Option<AutoModRule>> {
        let sql = format!("SELECT {RULE_COLS} FROM automod_rules WHERE guild_id = $1 AND id = $2");
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(guild.get()))
            .bind(i(id))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(rule_from)
            .transpose()
    }

    async fn put_rule(&self, r: &AutoModRule) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO automod_rules (id, guild_id, name, creator_id, event_type, trigger_kind, \
             trigger, actions, enabled, exempt_role_ids, exempt_channel_ids) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
             ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, event_type = EXCLUDED.event_type, \
               trigger_kind = EXCLUDED.trigger_kind, trigger = EXCLUDED.trigger, \
               actions = EXCLUDED.actions, enabled = EXCLUDED.enabled, \
               exempt_role_ids = EXCLUDED.exempt_role_ids, \
               exempt_channel_ids = EXCLUDED.exempt_channel_ids, updated_at = now()",
        )
        .bind(i(r.id))
        .bind(i(r.guild_id.get()))
        .bind(&r.name)
        .bind(i(r.creator_id.get()))
        .bind(event_str(r.event_type))
        .bind(format!("{:?}", r.trigger.kind()))
        .bind(Json(&r.trigger))
        .bind(Json(&r.actions))
        .bind(r.enabled)
        .bind(ids_in(r.exempt_role_ids.iter().map(|x| x.get())))
        .bind(ids_in(r.exempt_channel_ids.iter().map(|x| x.get())))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_rule(&self, guild: GuildId, id: u64) -> RepoResult<bool> {
        let r = sqlx::query("DELETE FROM automod_rules WHERE guild_id = $1 AND id = $2")
            .bind(i(guild.get()))
            .bind(i(id))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(r.rows_affected() > 0)
    }
}

// ---- webhooks ------------------------------------------------------------------

fn webhook_cols() -> String {
    format!(
        "id, guild_id, channel_id, kind, creator_id, name, avatar_key, token_hash, \
         application_id, source_channel_id, {} AS created_ms",
        ms("created_at")
    )
}

fn webhook_from(r: &PgRow) -> RepoResult<Webhook> {
    Ok(Webhook {
        id: WebhookId::new(u(get(r, "id")?)),
        guild_id: GuildId::new(u(get(r, "guild_id")?)),
        channel_id: ChannelId::new(u(get(r, "channel_id")?)),
        kind: WebhookKind::from_code(get(r, "kind")?).ok_or_else(|| bad("kind"))?,
        creator_id: get::<Option<i64>>(r, "creator_id")?.map(|v| UserId::new(u(v))),
        name: get(r, "name")?,
        avatar_key: get(r, "avatar_key")?,
        token_hash: get(r, "token_hash")?,
        application_id: get::<Option<i64>>(r, "application_id")?.map(|v| ApplicationId::new(u(v))),
        source_channel_id: get::<Option<i64>>(r, "source_channel_id")?
            .map(|v| ChannelId::new(u(v))),
        created_at_ms: get(r, "created_ms")?,
    })
}

#[async_trait]
impl WebhookRepository for PgSettingsRepo {
    async fn insert_webhook(&self, w: &Webhook) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO webhooks (id, guild_id, channel_id, kind, creator_id, name, avatar_key, \
             token_hash, application_id, source_channel_id, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, to_timestamp($11::double precision / 1000.0))",
        )
        .bind(i(w.id.get()))
        .bind(i(w.guild_id.get()))
        .bind(i(w.channel_id.get()))
        .bind(w.kind.code())
        .bind(w.creator_id.map(|x| i(x.get())))
        .bind(&w.name)
        .bind(w.avatar_key.as_deref())
        .bind(w.token_hash.as_deref())
        .bind(w.application_id.map(|x| i(x.get())))
        .bind(w.source_channel_id.map(|x| i(x.get())))
        .bind(w.created_at_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn get_webhook(&self, id: WebhookId) -> RepoResult<Option<Webhook>> {
        let sql = format!("SELECT {} FROM webhooks WHERE id = $1", webhook_cols());
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(id.get()))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(webhook_from)
            .transpose()
    }

    async fn list_guild_webhooks(&self, guild: GuildId) -> RepoResult<Vec<Webhook>> {
        let sql = format!(
            "SELECT {} FROM webhooks WHERE guild_id = $1 ORDER BY id",
            webhook_cols()
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(guild.get()))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(webhook_from)
            .collect()
    }

    async fn list_channel_followers(&self, source: ChannelId) -> RepoResult<Vec<Webhook>> {
        let sql = format!(
            "SELECT {} FROM webhooks WHERE kind = 2 AND source_channel_id = $1 ORDER BY id",
            webhook_cols()
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(source.get()))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?
            .iter()
            .map(webhook_from)
            .collect()
    }

    async fn count_channel_webhooks(&self, channel: ChannelId) -> RepoResult<usize> {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM webhooks WHERE channel_id = $1")
            .bind(i(channel.get()))
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        Ok(usize::try_from(n).unwrap_or(0))
    }

    async fn update_webhook(&self, w: &Webhook) -> RepoResult<()> {
        sqlx::query(
            "UPDATE webhooks SET channel_id = $2, name = $3, avatar_key = $4, token_hash = $5, \
             updated_at = now() WHERE id = $1",
        )
        .bind(i(w.id.get()))
        .bind(i(w.channel_id.get()))
        .bind(&w.name)
        .bind(w.avatar_key.as_deref())
        .bind(w.token_hash.as_deref())
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_webhook(&self, id: WebhookId) -> RepoResult<bool> {
        let r = sqlx::query("DELETE FROM webhooks WHERE id = $1")
            .bind(i(id.get()))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(r.rows_affected() > 0)
    }
}

// ---- templates -----------------------------------------------------------------

fn template_cols() -> String {
    format!(
        "code, guild_id, creator_id, name, description, usage_count, snapshot, \
         {} AS created_ms, {} AS updated_ms",
        ms("created_at"),
        ms("updated_at")
    )
}

fn template_from(r: &PgRow) -> RepoResult<GuildTemplate> {
    let snapshot: Json<TemplateSnapshot> = get(r, "snapshot")?;
    Ok(GuildTemplate {
        code: get(r, "code")?,
        guild_id: GuildId::new(u(get(r, "guild_id")?)),
        creator_id: UserId::new(u(get(r, "creator_id")?)),
        name: get(r, "name")?,
        description: get(r, "description")?,
        usage_count: u32::try_from(get::<i32>(r, "usage_count")?)
            .map_err(|_| bad("usage_count"))?,
        snapshot: snapshot.0,
        created_at_ms: get(r, "created_ms")?,
        updated_at_ms: get(r, "updated_ms")?,
    })
}

#[async_trait]
impl TemplateRepository for PgSettingsRepo {
    async fn get_template(&self, code: &str) -> RepoResult<Option<GuildTemplate>> {
        let sql = format!(
            "SELECT {} FROM guild_templates WHERE code = $1",
            template_cols()
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(code)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(template_from)
            .transpose()
    }

    async fn get_guild_template(&self, guild: GuildId) -> RepoResult<Option<GuildTemplate>> {
        let sql = format!(
            "SELECT {} FROM guild_templates WHERE guild_id = $1",
            template_cols()
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(i(guild.get()))
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .as_ref()
            .map(template_from)
            .transpose()
    }

    async fn put_template(&self, t: &GuildTemplate) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        // One template per guild: replace whatever exists (the code may change).
        sqlx::query("DELETE FROM guild_templates WHERE guild_id = $1")
            .bind(i(t.guild_id.get()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query(
            "INSERT INTO guild_templates (code, guild_id, creator_id, name, description, \
             usage_count, snapshot, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, \
                     to_timestamp($8::double precision / 1000.0), \
                     to_timestamp($9::double precision / 1000.0))",
        )
        .bind(&t.code)
        .bind(i(t.guild_id.get()))
        .bind(i(t.creator_id.get()))
        .bind(&t.name)
        .bind(t.description.as_deref())
        .bind(t.usage_count as i32)
        .bind(Json(&t.snapshot))
        .bind(t.created_at_ms)
        .bind(t.updated_at_ms)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn delete_template(&self, guild: GuildId) -> RepoResult<bool> {
        let r = sqlx::query("DELETE FROM guild_templates WHERE guild_id = $1")
            .bind(i(guild.get()))
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(r.rows_affected() > 0)
    }

    async fn bump_template_usage(&self, code: &str) -> RepoResult<()> {
        sqlx::query("UPDATE guild_templates SET usage_count = usage_count + 1 WHERE code = $1")
            .bind(code)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }
}

// ---- community -----------------------------------------------------------------

fn opt_channel(r: &PgRow, col: &str) -> RepoResult<Option<ChannelId>> {
    Ok(get::<Option<i64>>(r, col)?.map(|v| ChannelId::new(u(v))))
}

#[async_trait]
impl CommunityRepository for PgSettingsRepo {
    async fn get_community(&self, guild: GuildId) -> RepoResult<CommunitySettings> {
        // Rules and updates channels are owned by `guilds` (open question 138).
        let guild_row = sqlx::query(
            "SELECT rules_channel_id, public_updates_channel_id FROM guilds WHERE id = $1",
        )
        .bind(i(guild.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        let (rules_channel_id, updates_channel_id) = match &guild_row {
            Some(g) => (
                opt_channel(g, "rules_channel_id")?,
                opt_channel(g, "public_updates_channel_id")?,
            ),
            None => (None, None),
        };
        let row = sqlx::query(
            "SELECT community_enabled, safety_alerts_channel_id, \
             description, preferred_locale, onboarding_enabled, welcome_screen_enabled, guide_enabled, \
             invites_paused, raid_alert_joins_per_minute, age_restricted, \
             screening_enabled, screening_description, screening_rules, dm_spam_filter, \
             explicit_media_filter_dms, hide_suspicious_links, warn_new_account_days \
             FROM guild_community WHERE guild_id = $1",
        )
        .bind(i(guild.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        let Some(r) = row else {
            return Ok(CommunitySettings {
                rules_channel_id,
                updates_channel_id,
                ..CommunitySettings::new(guild)
            });
        };
        Ok(CommunitySettings {
            guild_id: guild,
            community_enabled: get(&r, "community_enabled")?,
            rules_channel_id,
            updates_channel_id,
            safety_alerts_channel_id: opt_channel(&r, "safety_alerts_channel_id")?,
            description: get(&r, "description")?,
            preferred_locale: get(&r, "preferred_locale")?,
            onboarding_enabled: get(&r, "onboarding_enabled")?,
            welcome_screen_enabled: get(&r, "welcome_screen_enabled")?,
            guide_enabled: get(&r, "guide_enabled")?,
            invites_paused: get(&r, "invites_paused")?,
            raid_alert_joins_per_minute: u32::try_from(get::<i32>(
                &r,
                "raid_alert_joins_per_minute",
            )?)
            .map_err(|_| bad("raid_alert_joins_per_minute"))?,
            dm_spam_filter: get(&r, "dm_spam_filter")?,
            explicit_media_filter_dms: get(&r, "explicit_media_filter_dms")?,
            hide_suspicious_links: get(&r, "hide_suspicious_links")?,
            warn_new_account_days: u32::try_from(get::<i32>(&r, "warn_new_account_days")?)
                .map_err(|_| bad("warn_new_account_days"))?,
            age_restricted: get(&r, "age_restricted")?,
            screening_enabled: get(&r, "screening_enabled")?,
            screening_description: get(&r, "screening_description")?,
            screening_rules: get::<Json<Vec<String>>>(&r, "screening_rules")?.0,
        })
    }

    async fn put_community(&self, s: &CommunitySettings) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "UPDATE guilds SET rules_channel_id = $2, public_updates_channel_id = $3 WHERE id = $1",
        )
        .bind(i(s.guild_id.get()))
        .bind(s.rules_channel_id.map(|c| i(c.get())))
        .bind(s.updates_channel_id.map(|c| i(c.get())))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query(
            "INSERT INTO guild_community (guild_id, community_enabled, \
             safety_alerts_channel_id, description, preferred_locale, \
             onboarding_enabled, welcome_screen_enabled, guide_enabled, invites_paused, \
             raid_alert_joins_per_minute, age_restricted, screening_enabled, \
             screening_description, screening_rules, dm_spam_filter, \
             explicit_media_filter_dms, hide_suspicious_links, warn_new_account_days) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18) \
             ON CONFLICT (guild_id) DO UPDATE SET \
               community_enabled = EXCLUDED.community_enabled, \
               safety_alerts_channel_id = EXCLUDED.safety_alerts_channel_id, \
               description = EXCLUDED.description, preferred_locale = EXCLUDED.preferred_locale, \
               onboarding_enabled = EXCLUDED.onboarding_enabled, \
               welcome_screen_enabled = EXCLUDED.welcome_screen_enabled, \
               guide_enabled = EXCLUDED.guide_enabled, invites_paused = EXCLUDED.invites_paused, \
               raid_alert_joins_per_minute = EXCLUDED.raid_alert_joins_per_minute, \
               age_restricted = EXCLUDED.age_restricted, \
               screening_enabled = EXCLUDED.screening_enabled, \
               screening_description = EXCLUDED.screening_description, \
               screening_rules = EXCLUDED.screening_rules, \
               dm_spam_filter = EXCLUDED.dm_spam_filter, \
               explicit_media_filter_dms = EXCLUDED.explicit_media_filter_dms, \
               hide_suspicious_links = EXCLUDED.hide_suspicious_links, \
               warn_new_account_days = EXCLUDED.warn_new_account_days, updated_at = now()",
        )
        .bind(i(s.guild_id.get()))
        .bind(s.community_enabled)
        .bind(s.safety_alerts_channel_id.map(|c| i(c.get())))
        .bind(s.description.as_deref())
        .bind(&s.preferred_locale)
        .bind(s.onboarding_enabled)
        .bind(s.welcome_screen_enabled)
        .bind(s.guide_enabled)
        .bind(s.invites_paused)
        .bind(s.raid_alert_joins_per_minute as i32)
        .bind(s.age_restricted)
        .bind(s.screening_enabled)
        .bind(s.screening_description.as_deref())
        .bind(Json(&s.screening_rules))
        .bind(s.dm_spam_filter)
        .bind(s.explicit_media_filter_dms)
        .bind(s.hide_suspicious_links)
        .bind(s.warn_new_account_days as i32)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn get_onboarding(&self, guild: GuildId) -> RepoResult<Onboarding> {
        let row = sqlx::query(
            "SELECT enabled, default_channel_ids, prompts FROM guild_onboarding WHERE guild_id = $1",
        )
        .bind(i(guild.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        let Some(r) = row else {
            return Ok(Onboarding {
                guild_id: guild,
                ..Onboarding::default()
            });
        };
        let prompts: Json<Vec<OnboardingPrompt>> = get(&r, "prompts")?;
        Ok(Onboarding {
            guild_id: guild,
            enabled: get(&r, "enabled")?,
            default_channel_ids: ids_out(get(&r, "default_channel_ids")?)
                .map(ChannelId::new)
                .collect(),
            prompts: prompts.0,
        })
    }

    async fn put_onboarding(&self, o: &Onboarding) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO guild_onboarding (guild_id, enabled, default_channel_ids, prompts) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (guild_id) DO UPDATE SET enabled = EXCLUDED.enabled, \
               default_channel_ids = EXCLUDED.default_channel_ids, prompts = EXCLUDED.prompts, \
               updated_at = now()",
        )
        .bind(i(o.guild_id.get()))
        .bind(o.enabled)
        .bind(ids_in(o.default_channel_ids.iter().map(|c| c.get())))
        .bind(Json(&o.prompts))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn get_welcome_screen(&self, guild: GuildId) -> RepoResult<WelcomeScreen> {
        let row = sqlx::query(
            "SELECT enabled, description, channels FROM guild_welcome_screens WHERE guild_id = $1",
        )
        .bind(i(guild.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        let Some(r) = row else {
            return Ok(WelcomeScreen {
                guild_id: guild,
                ..WelcomeScreen::default()
            });
        };
        let channels: Json<Vec<WelcomeChannel>> = get(&r, "channels")?;
        Ok(WelcomeScreen {
            guild_id: guild,
            enabled: get(&r, "enabled")?,
            description: get(&r, "description")?,
            channels: channels.0,
        })
    }

    async fn put_welcome_screen(&self, w: &WelcomeScreen) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO guild_welcome_screens (guild_id, enabled, description, channels) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (guild_id) DO UPDATE SET enabled = EXCLUDED.enabled, \
               description = EXCLUDED.description, channels = EXCLUDED.channels, updated_at = now()",
        )
        .bind(i(w.guild_id.get()))
        .bind(w.enabled)
        .bind(w.description.as_deref())
        .bind(Json(&w.channels))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }
}

#[async_trait]
impl TagRepository for PgSettingsRepo {
    async fn get_guild_tag(&self, guild: GuildId) -> RepoResult<Option<GuildTag>> {
        let row = sqlx::query(
            "SELECT tag, badge, color, members_can_display FROM guild_tags WHERE guild_id = $1",
        )
        .bind(i(guild.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        let Some(r) = row else { return Ok(None) };
        let badge: i16 = get(&r, "badge")?;
        let color: i32 = get(&r, "color")?;
        Ok(Some(GuildTag {
            guild_id: guild,
            tag: get(&r, "tag")?,
            badge: u8::try_from(badge).unwrap_or(0),
            color: u32::try_from(color).unwrap_or(0),
            members_can_display: get(&r, "members_can_display")?,
        }))
    }

    async fn put_guild_tag(&self, t: &GuildTag) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO guild_tags (guild_id, tag, badge, color, members_can_display) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (guild_id) DO UPDATE SET tag = EXCLUDED.tag, badge = EXCLUDED.badge, \
               color = EXCLUDED.color, members_can_display = EXCLUDED.members_can_display, \
               updated_at = now()",
        )
        .bind(i(t.guild_id.get()))
        .bind(&t.tag)
        .bind(i16::from(t.badge))
        .bind(i32::try_from(t.color).unwrap_or(0))
        .bind(t.members_can_display)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_guild_tag(&self, guild: GuildId) -> RepoResult<bool> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("DELETE FROM user_primary_guilds WHERE guild_id = $1")
            .bind(i(guild.get()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let n = sqlx::query("DELETE FROM guild_tags WHERE guild_id = $1")
            .bind(i(guild.get()))
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();
        tx.commit().await.map_err(db)?;
        Ok(n > 0)
    }

    async fn get_primary_guild(&self, user: UserId) -> RepoResult<Option<GuildId>> {
        let g: Option<i64> =
            sqlx::query_scalar("SELECT guild_id FROM user_primary_guilds WHERE user_id = $1")
                .bind(i(user.get()))
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?;
        Ok(g.map(|g| GuildId::new(u(g))))
    }

    async fn set_primary_guild(&self, user: UserId, guild: Option<GuildId>) -> RepoResult<()> {
        match guild {
            Some(g) => {
                sqlx::query(
                    "INSERT INTO user_primary_guilds (user_id, guild_id) VALUES ($1, $2) \
                     ON CONFLICT (user_id) DO UPDATE SET guild_id = EXCLUDED.guild_id, set_at = now()",
                )
                .bind(i(user.get()))
                .bind(i(g.get()))
                .execute(&self.pool)
                .await
                .map_err(db)?;
            }
            None => {
                sqlx::query("DELETE FROM user_primary_guilds WHERE user_id = $1")
                    .bind(i(user.get()))
                    .execute(&self.pool)
                    .await
                    .map_err(db)?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl InsightsRepository for PgSettingsRepo {
    async fn insights(&self, guild: GuildId, days: u32, now_ms: i64) -> RepoResult<Insights> {
        // The window is the `days` completed UTC days before today.
        const WINDOW: &str = "day >= (to_timestamp($2::double precision / 1000.0) AT TIME ZONE 'UTC')::date - $3::int \
             AND day < (to_timestamp($2::double precision / 1000.0) AT TIME ZONE 'UTC')::date";
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT to_char(day, 'YYYY-MM-DD') AS d, members, joins, messages, voice_minutes \
             FROM guild_insights_daily WHERE guild_id = $1 AND {WINDOW} ORDER BY day"
        )))
        .bind(i(guild.get()))
        .bind(now_ms)
        .bind(i32::try_from(days).unwrap_or(30))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut out = Insights::default();
        for r in &rows {
            let n = |col: &str| -> RepoResult<i64> { Ok(i64::from(get::<i32>(r, col)?)) };
            out.days.push(InsightsDay {
                day: get(r, "d")?,
                members: n("members")?,
                joins: n("joins")?,
                messages: n("messages")?,
                voice_minutes: n("voice_minutes")?,
            });
        }
        let top = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT channel_id, sum(messages)::bigint AS n FROM guild_insights_channel_daily \
             WHERE guild_id = $1 AND {WINDOW} GROUP BY channel_id ORDER BY n DESC, channel_id \
             LIMIT {TOP_CHANNELS}"
        )))
        .bind(i(guild.get()))
        .bind(now_ms)
        .bind(i32::try_from(days).unwrap_or(30))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        for r in &top {
            out.top_channels.push(TopChannel {
                channel_id: ChannelId::new(u(get(r, "channel_id")?)),
                messages: get(r, "n")?,
            });
        }
        Ok(out)
    }
}

/// How many days of insights are kept.
pub const INSIGHTS_KEEP_DAYS: i32 = 90;

/// Compute the counters of the UTC day that starts at `day_start_ms` for every live
/// guild (LNG-SSET-068), replacing any earlier numbers for that day. Returns how many
/// guilds were written. Also drops rows older than [`INSIGHTS_KEEP_DAYS`].
pub async fn compute_insights_day(pool: &PgPool, day_start_ms: i64) -> RepoResult<u64> {
    let mut tx = pool.begin().await.map_err(db)?;
    // Message ids embed their creation time, so the id range prunes partitions.
    const DAY: &str = "(to_timestamp($1::double precision / 1000.0) AT TIME ZONE 'UTC')::date";
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DELETE FROM guild_insights_channel_daily WHERE day = {DAY}"
    )))
    .bind(day_start_ms)
    .execute(&mut *tx)
    .await
    .map_err(db)?;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO guild_insights_channel_daily (guild_id, day, channel_id, messages) \
         SELECT c.guild_id, {DAY}, m.channel_id, count(*) \
         FROM messages m JOIN channels c ON c.id = m.channel_id \
         JOIN guilds g ON g.id = c.guild_id AND g.deleted_at IS NULL \
         WHERE m.id >= sr_snowflake_floor(to_timestamp($1::double precision / 1000.0)) \
           AND m.id < sr_snowflake_floor(to_timestamp(($1::double precision + 86400000) / 1000.0)) \
         GROUP BY c.guild_id, m.channel_id"
    )))
    .bind(day_start_ms)
    .execute(&mut *tx)
    .await
    .map_err(db)?;
    let n = sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO guild_insights_daily (guild_id, day, members, joins, messages) \
         SELECT g.id, {DAY}, \
           (SELECT count(*) FROM guild_members m WHERE m.guild_id = g.id \
              AND m.joined_at < to_timestamp(($1::double precision + 86400000) / 1000.0)), \
           (SELECT count(*) FROM guild_members m WHERE m.guild_id = g.id \
              AND m.joined_at >= to_timestamp($1::double precision / 1000.0) \
              AND m.joined_at < to_timestamp(($1::double precision + 86400000) / 1000.0)), \
           COALESCE((SELECT sum(d.messages) FROM guild_insights_channel_daily d \
                     WHERE d.guild_id = g.id AND d.day = {DAY}), 0) \
         FROM guilds g WHERE g.deleted_at IS NULL \
         ON CONFLICT (guild_id, day) DO UPDATE SET members = EXCLUDED.members, \
           joins = EXCLUDED.joins, messages = EXCLUDED.messages"
    )))
    .bind(day_start_ms)
    .execute(&mut *tx)
    .await
    .map_err(db)?
    .rows_affected();
    for table in ["guild_insights_daily", "guild_insights_channel_daily"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {table} WHERE day < {DAY} - {INSIGHTS_KEEP_DAYS}"
        )))
        .bind(day_start_ms)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    }
    tx.commit().await.map_err(db)?;
    Ok(n)
}

#[async_trait]
impl GuideRepository for PgSettingsRepo {
    async fn get_guide(&self, guild: GuildId) -> RepoResult<Guide> {
        let row = sqlx::query(
            "SELECT welcome_message, todos, resources FROM guild_guides WHERE guild_id = $1",
        )
        .bind(i(guild.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        let Some(r) = row else {
            return Ok(Guide {
                guild_id: guild,
                ..Guide::default()
            });
        };
        let todos: Json<Vec<GuideTodo>> = get(&r, "todos")?;
        let resources: Json<Vec<GuideResource>> = get(&r, "resources")?;
        Ok(Guide {
            guild_id: guild,
            welcome_message: get(&r, "welcome_message")?,
            todos: todos.0,
            resources: resources.0,
        })
    }

    async fn put_guide(&self, g: &Guide) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO guild_guides (guild_id, welcome_message, todos, resources) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (guild_id) DO UPDATE SET welcome_message = EXCLUDED.welcome_message, \
               todos = EXCLUDED.todos, resources = EXCLUDED.resources, updated_at = now()",
        )
        .bind(i(g.guild_id.get()))
        .bind(g.welcome_message.as_deref())
        .bind(Json(&g.todos))
        .bind(Json(&g.resources))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }
}

fn application_from(r: &PgRow) -> RepoResult<Application> {
    let answers: Json<Vec<Answer>> = get(r, "answers")?;
    let status: String = get(r, "status")?;
    let reviewed_by: Option<i64> = get(r, "reviewed_by")?;
    Ok(Application {
        id: u(get(r, "id")?),
        guild_id: GuildId::new(u(get(r, "guild_id")?)),
        user_id: UserId::new(u(get(r, "user_id")?)),
        answers: answers.0,
        status: ApplicationStatus::parse(&status)
            .ok_or_else(|| RepoError(format!("bad application status {status}")))?,
        created_at_ms: get(r, "created_ms")?,
        reviewed_by: reviewed_by.map(|v| UserId::new(u(v))),
        reviewed_at_ms: get(r, "reviewed_ms")?,
        rejection_reason: get(r, "rejection_reason")?,
    })
}

const APPLICATION_COLS: &str = "id, guild_id, user_id, answers, status, reviewed_by, \
     rejection_reason, (extract(epoch FROM created_at) * 1000)::bigint AS created_ms, \
     (extract(epoch FROM reviewed_at) * 1000)::bigint AS reviewed_ms";

#[async_trait]
impl ApplicationRepository for PgSettingsRepo {
    async fn get_application_form(&self, guild: GuildId) -> RepoResult<ApplicationForm> {
        let row = sqlx::query(
            "SELECT enabled, description, questions FROM guild_application_forms WHERE guild_id = $1",
        )
        .bind(i(guild.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        let Some(r) = row else {
            return Ok(ApplicationForm {
                guild_id: guild,
                ..ApplicationForm::default()
            });
        };
        let questions: Json<Vec<Question>> = get(&r, "questions")?;
        Ok(ApplicationForm {
            guild_id: guild,
            enabled: get(&r, "enabled")?,
            description: get(&r, "description")?,
            questions: questions.0,
        })
    }

    async fn put_application_form(&self, f: &ApplicationForm) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO guild_application_forms (guild_id, enabled, description, questions) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (guild_id) DO UPDATE SET enabled = EXCLUDED.enabled, \
               description = EXCLUDED.description, questions = EXCLUDED.questions, \
               updated_at = now()",
        )
        .bind(i(f.guild_id.get()))
        .bind(f.enabled)
        .bind(f.description.as_deref())
        .bind(Json(&f.questions))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn insert_application(&self, a: &Application) -> RepoResult<bool> {
        let n = sqlx::query(
            "INSERT INTO guild_applications (id, guild_id, user_id, answers, status, created_at) \
             VALUES ($1, $2, $3, $4, 'pending', to_timestamp($5::double precision / 1000.0)) \
             ON CONFLICT DO NOTHING",
        )
        .bind(i(a.id))
        .bind(i(a.guild_id.get()))
        .bind(i(a.user_id.get()))
        .bind(Json(&a.answers))
        .bind(a.created_at_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?
        .rows_affected();
        Ok(n > 0)
    }

    async fn get_application(&self, guild: GuildId, id: u64) -> RepoResult<Option<Application>> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {APPLICATION_COLS} FROM guild_applications WHERE guild_id = $1 AND id = $2"
        )))
        .bind(i(guild.get()))
        .bind(i(id))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(application_from).transpose()
    }

    async fn latest_application(
        &self,
        guild: GuildId,
        user: UserId,
    ) -> RepoResult<Option<Application>> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {APPLICATION_COLS} FROM guild_applications \
             WHERE guild_id = $1 AND user_id = $2 ORDER BY id DESC LIMIT 1"
        )))
        .bind(i(guild.get()))
        .bind(i(user.get()))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(application_from).transpose()
    }

    async fn list_applications(
        &self,
        guild: GuildId,
        status: Option<ApplicationStatus>,
        before: Option<u64>,
        limit: usize,
    ) -> RepoResult<Vec<Application>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {APPLICATION_COLS} FROM guild_applications \
             WHERE guild_id = $1 AND ($2::text IS NULL OR status = $2) \
               AND ($3::bigint IS NULL OR id < $3) ORDER BY id DESC LIMIT $4"
        )))
        .bind(i(guild.get()))
        .bind(status.map(ApplicationStatus::as_str))
        .bind(before.map(i))
        .bind(i64::try_from(limit).unwrap_or(100))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(application_from).collect()
    }

    async fn decide_application(
        &self,
        guild: GuildId,
        id: u64,
        approve: bool,
        reviewer: UserId,
        now_ms: i64,
        reason: Option<&str>,
    ) -> RepoResult<Option<Application>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE guild_applications SET status = $3, reviewed_by = $4, \
               reviewed_at = to_timestamp($5::double precision / 1000.0), rejection_reason = $6 \
             WHERE guild_id = $1 AND id = $2 AND status = 'pending' \
             RETURNING {APPLICATION_COLS}"
        )))
        .bind(i(guild.get()))
        .bind(i(id))
        .bind(if approve { "approved" } else { "rejected" })
        .bind(i(reviewer.get()))
        .bind(now_ms)
        .bind(reason.filter(|_| !approve))
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let Some(row) = row else { return Ok(None) };
        let app = application_from(&row)?;
        if approve {
            // The applicant may have joined another way meanwhile; the join is idempotent.
            sqlx::query(
                "INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(i(guild.get()))
            .bind(i(app.user_id.get()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        Ok(Some(app))
    }
}

// ---- scheduled events (migration 0800) ---------------------------------------------

fn event_cols() -> String {
    format!(
        "id, guild_id, channel_id, creator_id, name, description, status, entity_type, location, {} AS start_ms, {} AS end_ms, {} AS created_ms",
        ms("scheduled_start"),
        ms("scheduled_end"),
        ms("created_at")
    )
}

fn event_from(r: &PgRow) -> RepoResult<ScheduledEvent> {
    Ok(ScheduledEvent {
        id: u(get(r, "id")?),
        guild_id: GuildId::new(u(get(r, "guild_id")?)),
        channel_id: get::<Option<i64>>(r, "channel_id")?.map(|c| ChannelId::new(u(c))),
        creator_id: UserId::new(u(get(r, "creator_id")?)),
        name: get(r, "name")?,
        description: get(r, "description")?,
        start_ms: get(r, "start_ms")?,
        end_ms: get(r, "end_ms")?,
        status: u8::try_from(get::<i16>(r, "status")?)
            .ok()
            .and_then(EventStatus::from_code)
            .ok_or_else(|| bad("status"))?,
        entity_type: u8::try_from(get::<i16>(r, "entity_type")?)
            .ok()
            .and_then(EntityType::from_code)
            .ok_or_else(|| bad("entity_type"))?,
        location: get(r, "location")?,
        created_at_ms: get(r, "created_ms")?,
    })
}

#[async_trait]
impl ScheduledEventRepository for PgSettingsRepo {
    async fn insert_event(&self, e: &ScheduledEvent) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO guild_scheduled_events (id, guild_id, channel_id, creator_id, name, description, scheduled_start, scheduled_end, status, entity_type, location, created_at) VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7::bigint / 1000.0), to_timestamp($8::bigint / 1000.0), $9, $10, $11, to_timestamp($12::bigint / 1000.0))",
        )
        .bind(i(e.id))
        .bind(i(e.guild_id.get()))
        .bind(e.channel_id.map(|c| i(c.get())))
        .bind(i(e.creator_id.get()))
        .bind(&e.name)
        .bind(e.description.as_deref())
        .bind(e.start_ms)
        .bind(e.end_ms)
        .bind(i16::from(e.status.code()))
        .bind(i16::from(e.entity_type.code()))
        .bind(e.location.as_deref())
        .bind(e.created_at_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn get_event(&self, guild: GuildId, id: u64) -> RepoResult<Option<ScheduledEvent>> {
        let row = sqlx::query(&format!(
            "SELECT {} FROM guild_scheduled_events WHERE guild_id = $1 AND id = $2",
            event_cols()
        ))
        .bind(i(guild.get()))
        .bind(i(id))
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(event_from).transpose()
    }

    async fn list_open_events(&self, guild: GuildId) -> RepoResult<Vec<ScheduledEvent>> {
        let rows = sqlx::query(&format!(
            "SELECT {} FROM guild_scheduled_events WHERE guild_id = $1 AND status IN (1, 2) ORDER BY scheduled_start, id",
            event_cols()
        ))
        .bind(i(guild.get()))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(event_from).collect()
    }

    async fn count_open_events(&self, guild: GuildId) -> RepoResult<usize> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM guild_scheduled_events WHERE guild_id = $1 AND status IN (1, 2)",
        )
        .bind(i(guild.get()))
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        Ok(usize::try_from(n).unwrap_or(0))
    }

    async fn update_event(&self, e: &ScheduledEvent) -> RepoResult<()> {
        sqlx::query(
            "UPDATE guild_scheduled_events SET channel_id = $3, name = $4, description = $5, scheduled_start = to_timestamp($6::bigint / 1000.0), scheduled_end = to_timestamp($7::bigint / 1000.0), status = $8, entity_type = $9, location = $10, updated_at = now() WHERE guild_id = $1 AND id = $2",
        )
        .bind(i(e.guild_id.get()))
        .bind(i(e.id))
        .bind(e.channel_id.map(|c| i(c.get())))
        .bind(&e.name)
        .bind(e.description.as_deref())
        .bind(e.start_ms)
        .bind(e.end_ms)
        .bind(i16::from(e.status.code()))
        .bind(i16::from(e.entity_type.code()))
        .bind(e.location.as_deref())
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_event(&self, guild: GuildId, id: u64) -> RepoResult<bool> {
        let n = sqlx::query("DELETE FROM guild_scheduled_events WHERE guild_id = $1 AND id = $2")
            .bind(i(guild.get()))
            .bind(i(id))
            .execute(&self.pool)
            .await
            .map_err(db)?
            .rows_affected();
        Ok(n > 0)
    }

    async fn add_interest(&self, event: u64, user: UserId, now_ms: i64) -> RepoResult<bool> {
        let n = sqlx::query(
            "INSERT INTO guild_scheduled_event_users (event_id, user_id, created_at) VALUES ($1, $2, to_timestamp($3::bigint / 1000.0)) ON CONFLICT DO NOTHING",
        )
        .bind(i(event))
        .bind(i(user.get()))
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(db)?
        .rows_affected();
        Ok(n > 0)
    }

    async fn remove_interest(&self, event: u64, user: UserId) -> RepoResult<bool> {
        let n = sqlx::query(
            "DELETE FROM guild_scheduled_event_users WHERE event_id = $1 AND user_id = $2",
        )
        .bind(i(event))
        .bind(i(user.get()))
        .execute(&self.pool)
        .await
        .map_err(db)?
        .rows_affected();
        Ok(n > 0)
    }

    async fn interest_counts(&self, events: &[u64]) -> RepoResult<HashMap<u64, u32>> {
        let rows = sqlx::query(
            "SELECT event_id, count(*) AS n FROM guild_scheduled_event_users WHERE event_id = ANY($1) GROUP BY event_id",
        )
        .bind(ids_in(events.iter().copied()))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|r| {
                Ok((
                    u(get(r, "event_id")?),
                    u32::try_from(get::<i64>(r, "n")?).unwrap_or(u32::MAX),
                ))
            })
            .collect()
    }

    async fn interested_in(&self, user: UserId, events: &[u64]) -> RepoResult<HashSet<u64>> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT event_id FROM guild_scheduled_event_users WHERE user_id = $1 AND event_id = ANY($2)",
        )
        .bind(i(user.get()))
        .bind(ids_in(events.iter().copied()))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(ids_out(ids).collect())
    }

    async fn list_interested(
        &self,
        event: u64,
        before: Option<UserId>,
        after: Option<UserId>,
        limit: usize,
    ) -> RepoResult<Vec<UserId>> {
        let limit = i64::try_from(limit).unwrap_or(100);
        // "before" alone pages from the end: take the highest ids below it, then flip back.
        let backwards = before.is_some() && after.is_none();
        let lower = after.map_or(0, |a| i(a.get()));
        let upper = before.map_or(i64::MAX, |b| i(b.get()));
        let sql = if backwards {
            "SELECT user_id FROM guild_scheduled_event_users WHERE event_id = $1 AND user_id > $2 AND user_id < $3 ORDER BY user_id DESC LIMIT $4"
        } else {
            "SELECT user_id FROM guild_scheduled_event_users WHERE event_id = $1 AND user_id > $2 AND user_id < $3 ORDER BY user_id LIMIT $4"
        };
        let mut ids: Vec<i64> = sqlx::query_scalar(sql)
            .bind(i(event))
            .bind(lower)
            .bind(upper)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        if backwards {
            ids.reverse();
        }
        Ok(ids_out(ids).map(UserId::new).collect())
    }

    async fn due_events(&self, now_ms: i64) -> RepoResult<Vec<ScheduledEvent>> {
        let rows = sqlx::query(&format!(
            "SELECT {} FROM guild_scheduled_events WHERE (status = 1 AND scheduled_start <= to_timestamp($1::bigint / 1000.0)) OR (status = 2 AND scheduled_end IS NOT NULL AND scheduled_end <= to_timestamp($1::bigint / 1000.0)) OR (status IN (1, 2) AND entity_type IN (1, 2) AND channel_id IS NULL) ORDER BY scheduled_start, id LIMIT 500",
            event_cols()
        ))
        .bind(now_ms)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(event_from).collect()
    }
}
