//! Postgres implementation of the `sr-chat` settings repository traits
//! (migrations 0030-0036). Queries are runtime-checked; timestamps cross the
//! boundary as Unix milliseconds, like `chat_repo`. Tests are `#[ignore]` and need
//! `SR_TEST_DATABASE_URL` (see `tests/settings_repo.rs`).

use async_trait::async_trait;
use sqlx::postgres::PgRow;
use sqlx::types::Json;
use sqlx::{PgPool, Row};

use sr_chat::error::RepoError;
use sr_chat::repo::RepoResult;
use sr_chat::settings::audit::{AuditAction, AuditChange, AuditEntry, AuditQuery};
use sr_chat::settings::automod::{Action, AutoModRule, EventType, Trigger};
use sr_chat::settings::community::{
    CommunitySettings, Onboarding, OnboardingPrompt, WelcomeChannel, WelcomeScreen,
};
use sr_chat::settings::expressions::{Emoji, Sticker, StickerFormat};
use sr_chat::settings::invites::GuildInvite;
use sr_chat::settings::repo::{
    AuditRepository, AutoModRepository, CommunityRepository, ExpressionRepository,
    InviteRepository, TemplateRepository, WebhookRepository,
};
use sr_chat::settings::templates::{GuildTemplate, TemplateSnapshot};
use sr_chat::settings::webhooks::{Webhook, WebhookKind};
use sr_core::{ApplicationId, ChannelId, EmojiId, GuildId, RoleId, StickerId, UserId, WebhookId};

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
            "UPDATE webhooks SET channel_id = $2, name = $3, avatar_key = $4, updated_at = now() \
             WHERE id = $1",
        )
        .bind(i(w.id.get()))
        .bind(i(w.channel_id.get()))
        .bind(&w.name)
        .bind(w.avatar_key.as_deref())
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
             invites_paused, raid_alert_joins_per_minute, age_restricted \
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
            age_restricted: get(&r, "age_restricted")?,
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
             raid_alert_joins_per_minute, age_restricted) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
             ON CONFLICT (guild_id) DO UPDATE SET \
               community_enabled = EXCLUDED.community_enabled, \
               safety_alerts_channel_id = EXCLUDED.safety_alerts_channel_id, \
               description = EXCLUDED.description, preferred_locale = EXCLUDED.preferred_locale, \
               onboarding_enabled = EXCLUDED.onboarding_enabled, \
               welcome_screen_enabled = EXCLUDED.welcome_screen_enabled, \
               guide_enabled = EXCLUDED.guide_enabled, invites_paused = EXCLUDED.invites_paused, \
               raid_alert_joins_per_minute = EXCLUDED.raid_alert_joins_per_minute, \
               age_restricted = EXCLUDED.age_restricted, updated_at = now()",
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
