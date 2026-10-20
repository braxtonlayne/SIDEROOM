//! Data export (LNG-USET-020, doc 09 section 4): "Request my data" builds a Discord-style
//! package (JSON files in a zip) in a background job and offers it for 7 days.
//!
//! This module holds the request bookkeeping and the package builder; the job runner and the
//! download route live in `sr-server` and `sr-api`.
//!
//! The package contains: `README.txt`, `account.json`, `settings.json`, `relationships.json`,
//! `servers.json`, `connections.json`, `invites.json`, `invoices.json`, `activity.json`
//! (ratings, reviews, watch and read history, lists), `security.json` (sessions and sign-in
//! history, never password hashes or tokens), `attachments.json` (an index of uploads, not the
//! files) and `messages/<channel id>.json` (the account's own undeleted messages).

use std::io::{Seek, Write};

use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use zip::write::{SimpleFileOptions, ZipWriter};

use crate::{
    DbError, DbResult,
    jobs::{self, NewJob},
};

/// Job kind of the package builder.
pub const JOB_KIND: &str = "user.data_export";
/// Hours between two requests of one account (a failed build does not count).
pub const COOLDOWN_HOURS: i64 = 24;
/// Days a finished package stays available.
pub const KEEP_DAYS: i64 = 7;
/// Messages written per query.
const MESSAGE_PAGE: i64 = 2_000;

/// One export request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRecord {
    pub id: i64,
    pub user_id: i64,
    /// `pending`, `building`, `ready`, `failed` or `expired`.
    pub state: String,
    pub requested_ms: i64,
    pub ready_ms: Option<i64>,
    pub expires_ms: Option<i64>,
    pub storage_key: Option<String>,
    pub size_bytes: Option<i64>,
}

/// Why a request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestRefusal {
    /// A package is already being built.
    InProgress(ExportRecord),
    /// The last request was less than [`COOLDOWN_HOURS`] ago.
    TooSoon { retry_after_secs: i64 },
}

const COLS: &str = "id, user_id, state, \
    (EXTRACT(EPOCH FROM requested_at) * 1000)::BIGINT AS requested_ms, \
    (EXTRACT(EPOCH FROM ready_at) * 1000)::BIGINT AS ready_ms, \
    (EXTRACT(EPOCH FROM expires_at) * 1000)::BIGINT AS expires_ms, storage_key, size_bytes";

fn record(r: &sqlx::postgres::PgRow) -> DbResult<ExportRecord> {
    Ok(ExportRecord {
        id: r.try_get("id")?,
        user_id: r.try_get("user_id")?,
        state: r.try_get("state")?,
        requested_ms: r.try_get("requested_ms")?,
        ready_ms: r.try_get("ready_ms")?,
        expires_ms: r.try_get("expires_ms")?,
        storage_key: r.try_get("storage_key")?,
        size_bytes: r.try_get("size_bytes")?,
    })
}

fn query(sql: String) -> sqlx::query::Query<'static, sqlx::Postgres, sqlx::postgres::PgArguments> {
    sqlx::query(sqlx::AssertSqlSafe(sql))
}

/// The newest request of `user`, if any.
pub async fn latest(pool: &PgPool, user: i64) -> DbResult<Option<ExportRecord>> {
    let row = query(format!(
        "SELECT {COLS} FROM data_exports WHERE user_id = $1 ORDER BY requested_at DESC, id DESC LIMIT 1"
    ))
    .bind(user)
    .fetch_optional(pool)
    .await?;
    row.as_ref().map(record).transpose()
}

/// One request of `user`.
pub async fn get(pool: &PgPool, user: i64, id: i64) -> DbResult<Option<ExportRecord>> {
    let row = query(format!(
        "SELECT {COLS} FROM data_exports WHERE user_id = $1 AND id = $2"
    ))
    .bind(user)
    .bind(id)
    .fetch_optional(pool)
    .await?;
    row.as_ref().map(record).transpose()
}

/// Record a request and queue the build job in one transaction.
pub async fn request(
    pool: &PgPool,
    id: i64,
    job_id: i64,
    user: i64,
) -> DbResult<Result<ExportRecord, RequestRefusal>> {
    let mut tx = pool.begin().await?;
    // Serialise one account's requests.
    sqlx::query("SELECT id FROM accounts WHERE id = $1 FOR UPDATE")
        .bind(user)
        .fetch_optional(&mut *tx)
        .await?;
    let open = query(format!(
        "SELECT {COLS} FROM data_exports WHERE user_id = $1 AND state IN ('pending', 'building')"
    ))
    .bind(user)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(r) = open {
        return Ok(Err(RequestRefusal::InProgress(record(&r)?)));
    }
    let wait: Option<f64> = sqlx::query_scalar(
        "SELECT EXTRACT(EPOCH FROM (requested_at + make_interval(hours => $2) - now()))::float8 \
         FROM data_exports WHERE user_id = $1 AND state <> 'failed' \
         ORDER BY requested_at DESC LIMIT 1",
    )
    .bind(user)
    .bind(i32::try_from(COOLDOWN_HOURS).unwrap_or(24))
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(w) = wait.filter(|w| *w > 0.0) {
        return Ok(Err(RequestRefusal::TooSoon {
            retry_after_secs: w.ceil() as i64,
        }));
    }
    queue_in(&mut tx, id, job_id, user).await?;
    let row = query(format!("SELECT {COLS} FROM data_exports WHERE id = $1"))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Ok(record(&row)?))
}

/// Insert the request row and its build job inside `tx` (shared by the self-service route and the staff
/// data-request queue, which skips the 24-hour cooldown but not the one-open-request rule).
pub(crate) async fn queue_in(
    tx: &mut sqlx::PgConnection,
    id: i64,
    job_id: i64,
    user: i64,
) -> DbResult<()> {
    sqlx::query("INSERT INTO data_exports (id, user_id) VALUES ($1, $2)")
        .bind(id)
        .bind(user)
        .execute(&mut *tx)
        .await?;
    jobs::enqueue(
        &mut *tx,
        &NewJob::new(
            job_id,
            JOB_KIND,
            json!({ "export_id": id.to_string(), "user_id": user.to_string() }),
        ),
    )
    .await?;
    Ok(())
}

/// Claim a pending request for building. `None` when it is gone or already taken.
pub async fn begin(pool: &PgPool, id: i64) -> DbResult<Option<i64>> {
    Ok(sqlx::query_scalar(
        "UPDATE data_exports SET state = 'building' WHERE id = $1 \
         AND state IN ('pending', 'building') RETURNING user_id",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

/// The package is stored: offer it for [`KEEP_DAYS`] days.
pub async fn finish(pool: &PgPool, id: i64, key: &str, size: i64) -> DbResult<()> {
    sqlx::query(
        "UPDATE data_exports SET state = 'ready', storage_key = $2, size_bytes = $3, \
         ready_at = now(), expires_at = now() + make_interval(days => $4), error = NULL \
         WHERE id = $1",
    )
    .bind(id)
    .bind(key)
    .bind(size)
    .bind(i32::try_from(KEEP_DAYS).unwrap_or(7))
    .execute(pool)
    .await?;
    // A staff data-request export (M-25 step c) completes with its package.
    sqlx::query(
        "UPDATE data_requests SET state = 'completed', completed_at = now(), outcome = $2 WHERE export_id = $1 AND state = 'running'",
    )
    .bind(id)
    .bind(sr_admin::data_requests::EXPORT_DONE)
    .execute(pool)
    .await?;
    Ok(())
}

/// The build gave up.
pub async fn fail(pool: &PgPool, id: i64, error: &str) -> DbResult<()> {
    sqlx::query("UPDATE data_exports SET state = 'failed', error = $2 WHERE id = $1")
        .bind(id)
        .bind(error.chars().take(500).collect::<String>())
        .execute(pool)
        .await?;
    // The queue entry goes back to verified so staff can run the export again.
    sqlx::query(
        "UPDATE data_requests SET state = 'verified', export_id = NULL WHERE export_id = $1 AND state = 'running'",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Ready packages whose link has lapsed: `(id, storage key)`.
pub async fn lapsed(pool: &PgPool) -> DbResult<Vec<(i64, String)>> {
    Ok(sqlx::query_as(
        "SELECT id, storage_key FROM data_exports \
         WHERE state = 'ready' AND expires_at <= now() AND storage_key IS NOT NULL LIMIT 200",
    )
    .fetch_all(pool)
    .await?)
}

/// The package object is deleted: the request is expired.
pub async fn mark_expired(pool: &PgPool, id: i64) -> DbResult<()> {
    sqlx::query("UPDATE data_exports SET state = 'expired', storage_key = NULL WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ---- the package ----------------------------------------------------------------------

/// One small section: a JSON array of the rows `sql` selects (ids already cast to text).
async fn rows(pool: &PgPool, sql: &str, user: i64) -> DbResult<Value> {
    let wrapped = format!("SELECT COALESCE(jsonb_agg(t), '[]'::jsonb) FROM ({sql}) t");
    Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(wrapped))
        .bind(user)
        .fetch_one(pool)
        .await?)
}

fn zip_err(e: impl std::fmt::Display) -> DbError {
    DbError::Sqlx(sqlx::Error::Protocol(format!("zip: {e}")))
}

fn put<W: Write + Seek>(zip: &mut ZipWriter<W>, name: &str, value: &Value) -> DbResult<()> {
    zip.start_file(name, SimpleFileOptions::default())
        .map_err(zip_err)?;
    let bytes = serde_json::to_vec_pretty(value).map_err(zip_err)?;
    zip.write_all(&bytes).map_err(zip_err)?;
    Ok(())
}

const README: &str = "SIDE ROOM data package\n\
\n\
This archive holds the data tied to your account, as JSON files.\n\
\n\
  account.json        your profile and account details\n\
  settings.json       your settings, privacy and notification choices, chosen status\n\
  relationships.json  friends, requests, blocks, ignores and your private notes\n\
  servers.json        servers you belong to or own, with your nickname there\n\
  connections.json    linked accounts\n\
  invites.json        invites you issued, and your invite credits ledger\n\
  invoices.json       your donations and purchases\n\
  activity.json       ratings, reviews, lists, watch and read history, your profile page and packages\n\
  security.json       signed-in sessions and recent sign-in attempts\n\
  attachments.json    an index of files you uploaded (the files themselves are not included)\n\
  messages/           the messages you wrote that you have not deleted, one file per channel\n\
                      (a message sent through a webhook you made has \"webhook\": its id and name)\n\
\n\
Passwords, tokens and keys are never part of this package.\n\
Messages you deleted are hidden from everyone and are not listed.\n\
\n\
What stays after you delete your account\n\
Some records are kept after erasure so that bans, invoices and moderation decisions keep\n\
their history: your Person record (the link between your accounts), the invite graph,\n\
your invoices and the staff-side moderation history. They are kept to stop banned people\n\
from returning, to meet accounting duties and to keep staff decisions accountable.\n\
Your profile and your sign-in data are removed.\n";

/// Write the package for `user` into `out` (a zip). Returns the number of messages written.
pub async fn build_package<W: Write + Seek>(
    pool: &PgPool,
    user: i64,
    out: W,
    now_ms: i64,
) -> DbResult<u64> {
    let mut zip = ZipWriter::new(out);
    zip.start_file("README.txt", SimpleFileOptions::default())
        .map_err(zip_err)?;
    zip.write_all(README.as_bytes()).map_err(zip_err)?;

    let account = rows(
        pool,
        "SELECT id::text AS id, username::text AS username, display_name, email::text AS email, \
         email_verified_at, state::text AS state, locale, timezone, flags::text AS flags, bio, \
         pronouns, accent_color, avatar_key, banner_key, avatar_decoration, profile_effect, \
         theme_primary, theme_accent, age_confirmed, is_bot, created_at, last_seen_at \
         FROM accounts WHERE id = $1",
        user,
    )
    .await?;
    put(
        &mut zip,
        "account.json",
        &account.get(0).cloned().unwrap_or(Value::Null),
    )?;

    let settings = json!({
        "user_settings": rows(pool, "SELECT settings, version, default_notification_level, updated_at FROM user_settings WHERE user_id = $1", user).await?,
        "privacy": rows(pool, "SELECT fr_everyone, fr_friends_of_friends, fr_server_members, dm_server_members, dm_message_requests FROM user_privacy WHERE user_id = $1", user).await?,
        "notification_settings": rows(pool, "SELECT guild_id::text AS guild_id, level, suppress_everyone, suppress_roles, suppress_highlights, mute_scheduled_events, mobile_push, muted, mute_end_at FROM notification_settings WHERE user_id = $1", user).await?,
        "presence": rows(pool, "SELECT status, status_expires_at, custom_text, custom_emoji_name, custom_expires_at FROM user_presence_prefs WHERE user_id = $1", user).await?,
        "primary_server": rows(pool, "SELECT guild_id::text AS guild_id, set_at FROM user_primary_guilds WHERE user_id = $1", user).await?,
    });
    put(&mut zip, "settings.json", &settings)?;

    let relationships = json!({
        "relationships": rows(pool, "SELECT target_id::text AS user_id, type, nickname, created_at FROM relationships WHERE user_id = $1", user).await?,
        "ignores": rows(pool, "SELECT target_id::text AS user_id, created_at FROM user_ignores WHERE user_id = $1", user).await?,
        "notes": rows(pool, "SELECT target_id::text AS user_id, note, updated_at FROM user_notes WHERE user_id = $1", user).await?,
    });
    put(&mut zip, "relationships.json", &relationships)?;

    let servers = json!({
        "memberships": rows(pool, "SELECT m.guild_id::text AS guild_id, g.name, m.nick, m.bio, m.joined_at FROM guild_members m JOIN guilds g ON g.id = m.guild_id WHERE m.user_id = $1 ORDER BY m.guild_id", user).await?,
        "owned": rows(pool, "SELECT id::text AS guild_id, name, created_at FROM guilds WHERE owner_id = $1 ORDER BY id", user).await?,
    });
    put(&mut zip, "servers.json", &servers)?;

    put(
        &mut zip,
        "connections.json",
        &rows(pool, "SELECT provider, external_id, display_name AS name, verified, visible, show_activity, url, stats, created_at FROM user_connections WHERE user_id = $1 ORDER BY created_at", user).await?,
    )?;

    let invites = json!({
        "issued": rows(pool, "SELECT code::text AS code, state::text AS state, note, redeemed_by_account_id::text AS redeemed_by, redeemed_at, revoked_at, created_at FROM platform_invites WHERE issuer_account_id = $1 ORDER BY id", user).await?,
        "credits_ledger": rows(pool, "SELECT delta, reason::text AS reason, created_at FROM invite_credits_ledger WHERE account_id = $1 ORDER BY id", user).await?,
        "server_invites": rows(pool, "SELECT guild_id::text AS guild_id, code, uses, created_at FROM guild_invites WHERE inviter_id = $1 ORDER BY created_at", user).await?,
    });
    put(&mut zip, "invites.json", &invites)?;

    put(
        &mut zip,
        "invoices.json",
        &rows(pool, "SELECT id::text AS id, purpose, usd_cents, sats, state, created_ms FROM payment_invoices WHERE user_id = $1 ORDER BY created_ms", user).await?,
    )?;

    let activity = json!({
        "cinema_ratings": rows(pool, "SELECT title_id::text AS title_id, episode_id::text AS episode_id, score, updated_at FROM cinema_ratings WHERE user_id = $1", user).await?,
        "cinema_reviews": rows(pool, "SELECT id::text AS id, title_id::text AS title_id, body, spoiler, created_at FROM cinema_reviews WHERE user_id = $1", user).await?,
        "cinema_progress": rows(pool, "SELECT title_id::text AS title_id, episode_id::text AS episode_id, position_secs, duration_secs, percent, completed, updated_at FROM cinema_progress WHERE user_id = $1", user).await?,
        "cinema_lists": rows(pool, "SELECT id::text AS id, kind, name, description, visibility, created_at FROM cinema_lists WHERE owner_id = $1", user).await?,
        "reading_progress": rows(pool, "SELECT series_id::text AS series_id, chapter_id::text AS chapter_id, page_index, percent, completed, updated_at FROM reading_progress WHERE user_id = $1", user).await?,
        "reading_bookmarks": rows(pool, "SELECT chapter_id::text AS chapter_id, page_index, note, created_at FROM reading_bookmarks WHERE user_id = $1", user).await?,
        // EXT-PRF / EXT-PKG: the profile page document, guestbook entries written and received, packages
        // authored and themes turned on.
        "profile_page": rows(pool, "SELECT profile_doc FROM accounts WHERE id = $1", user).await?,
        "profile_media": rows(pool, "SELECT id::text AS id, kind, object_key, content_type, width, height, bytes, created_at FROM profile_media WHERE account_id = $1 ORDER BY id", user).await?,
        "guestbook_written": rows(pool, "SELECT id::text AS id, profile_id::text AS profile_id, body, created_at FROM profile_guestbook WHERE author_id = $1 ORDER BY id", user).await?,
        "guestbook_received": rows(pool, "SELECT id::text AS id, author_id::text AS author_id, body, created_at FROM profile_guestbook WHERE profile_id = $1 ORDER BY id", user).await?,
        "packages": rows(pool, "SELECT p.id::text AS id, p.kind, p.slug, p.name, v.version, v.review_state, v.created_at FROM packages p JOIN package_versions v ON v.package_id = p.id WHERE p.author_id = $1 ORDER BY p.id, v.id", user).await?,
        "enabled_packages": rows(pool, "SELECT package_id::text AS package_id, version_id::text AS version_id, section, enabled_at FROM package_enablements WHERE scope = 'user' AND scope_id = $1", user).await?,
    });
    put(&mut zip, "activity.json", &activity)?;

    let security = json!({
        "sessions": rows(pool, "SELECT id::text AS id, device_label, user_agent, ip::text AS ip, last_seen_at, expires_at, revoked_at, created_at FROM sessions WHERE account_id = $1 ORDER BY created_at DESC LIMIT 500", user).await?,
        "sign_in_history": rows(pool, "SELECT ip::text AS ip, success, created_at FROM login_events WHERE account_id = $1 ORDER BY created_at DESC LIMIT 500", user).await?,
        "authenticators": json!({
            "totp_enabled": rows(pool, "SELECT confirmed_at FROM mfa_totp WHERE account_id = $1", user).await?,
            "passkeys": rows(pool, "SELECT name, created_at, last_used_at FROM passkey_credentials WHERE account_id = $1", user).await?,
        }),
        "authorized_apps": rows(pool, "SELECT application_id::text AS application_id, scopes, created_at, revoked_at FROM oauth2_grants WHERE user_id = $1", user).await?,
    });
    put(&mut zip, "security.json", &security)?;

    put(
        &mut zip,
        "attachments.json",
        &rows(pool, "SELECT id::text AS id, message_id::text AS message_id, filename, content_type, size_bytes, width, height, description, is_large, expires_at, state::text AS state, created_at FROM attachments WHERE uploader_id = $1 ORDER BY id", user).await?,
    )?;

    // Messages: one file per channel, paged so a long history never sits in memory twice.
    let channels: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT channel_id FROM messages WHERE author_id = $1 AND deleted_at IS NULL ORDER BY channel_id",
    )
    .bind(user)
    .fetch_all(pool)
    .await?;
    let mut total = 0_u64;
    for channel in channels {
        zip.start_file(
            format!("messages/{channel}.json"),
            SimpleFileOptions::default(),
        )
        .map_err(zip_err)?;
        zip.write_all(b"[").map_err(zip_err)?;
        let mut after = 0_i64;
        let mut first = true;
        loop {
            let page: Vec<(i64, Value)> = sqlx::query_as(
                "SELECT id, jsonb_build_object('id', id::text, 'channel_id', channel_id::text, \
                   'guild_id', guild_id::text, 'type', type, 'content', content, \
                   'timestamp', created_at, 'edited_timestamp', edited_at, 'pinned', pinned, \
                   'reference', reference, 'embeds', embeds, 'flags', flags, \
                   'webhook', (SELECT jsonb_build_object('id', w.webhook_id::text, 'name', w.username) \
                               FROM message_webhook_authors w WHERE w.message_id = messages.id)) \
                 FROM messages WHERE author_id = $1 AND channel_id = $2 AND deleted_at IS NULL \
                   AND id > $3 ORDER BY id LIMIT $4",
            )
            .bind(user)
            .bind(channel)
            .bind(after)
            .bind(MESSAGE_PAGE)
            .fetch_all(pool)
            .await?;
            let Some(last) = page.last().map(|p| p.0) else {
                break;
            };
            for (_, m) in &page {
                if !first {
                    zip.write_all(b",\n").map_err(zip_err)?;
                }
                first = false;
                zip.write_all(&serde_json::to_vec(m).map_err(zip_err)?)
                    .map_err(zip_err)?;
                total += 1;
            }
            after = last;
        }
        zip.write_all(b"]").map_err(zip_err)?;
    }
    put(
        &mut zip,
        "package.json",
        &json!({ "user_id": user.to_string(), "generated_at_ms": now_ms, "messages": total, "format": 1 }),
    )?;
    zip.finish().map_err(zip_err)?;
    Ok(total)
}
