//! The takedown register (step 2; spec 5; D-293, D-212). Staff enter a notice, identify the content, and either
//! reject it as incomplete or uphold it: the items are held through the hold API ([`crate::holds`]), made
//! unavailable, blocked on the media path, and their uploaders are told. Upheld takedowns are the strikes of the
//! repeat-infringer policy; reaching `moderation.strike_review_threshold` opens a review case and never suspends
//! anyone. A counter-notice recorded by staff schedules the restore for ten business days later, which the
//! housekeeping job applies unless staff noted a court filing. Every state change writes the access log in the
//! same transaction, and notices are kept forever.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use sqlx::{PgConnection, Row};
use sr_admin::{AccessKind, AuditEntry};
use sr_core::PlatformPermissions as P;

use crate::error::{ModError, ModResult, conflict, invalid};
use crate::holds::{HoldRequest, HoldScope, place_hold_in, release_hold_in};
use crate::model::{
    CountedNotice, ItemKind, ItemState, ItemStatus, NewItem, NewNotice, NoticeDetail, NoticeEvent,
    NoticeState, NoticeSummary, RejectOutcome, StrikeCount, TakedownItem, TakedownKind,
    TakedownPolicy, UpholdOutcome, Who,
};
use crate::rules;
use crate::service::{IdSource, Moderation, limits_in, log_staff, push_event, require_staff};
use crate::takedown_rules::{
    RESTORE_BUSINESS_DAYS, TRIAGE_BUSINESS_DAYS, Weekend, add_business_days, can_move,
    incomplete_reply, missing_parts, restore_due, review_due, strikes_and_contested,
};

const MAX_FIELD: usize = 4000;
const MAX_ORIGINAL: usize = 40_000;
const MAX_LOCATIONS: usize = 100;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0))
}

fn notice_ref(id: i64) -> String {
    format!("TK-{id}")
}

const NOTICE_SELECT: &str = "SELECT n.id, n.kind, n.state, n.notifier_name, n.notifier_contact, n.legal_basis, \
    n.locations, n.original_text, n.entered_by, n.case_id, n.rejection_reason, n.counter_notice_text, \
    n.restore_reason, n.court_filing_noted, n.counter_notice_by, n.removed_by, \
    (extract(epoch FROM n.received_at) * 1000)::bigint AS received_ms, \
    (extract(epoch FROM n.triage_due_at) * 1000)::bigint AS triage_due_ms, \
    (extract(epoch FROM n.removed_at) * 1000)::bigint AS removed_ms, \
    (extract(epoch FROM n.counter_noticed_at) * 1000)::bigint AS counter_ms, \
    (extract(epoch FROM n.restore_after) * 1000)::bigint AS restore_after_ms, \
    (extract(epoch FROM n.restored_at) * 1000)::bigint AS restored_ms, \
    (SELECT count(*) FROM takedown_items i WHERE i.notice_id = n.id) AS items \
    FROM takedown_notices n";

/// A notice row as loaded.
struct NoticeRow {
    summary: NoticeSummary,
    notifier_contact: String,
    legal_basis: String,
    locations: Vec<String>,
    original_text: String,
    entered_by: i64,
    rejection_reason: String,
    counter_notice_text: String,
    counter_ms: Option<i64>,
    restored_ms: Option<i64>,
    restore_reason: String,
    counter_notice_by: Option<i64>,
    removed_by: Option<i64>,
}

fn notice_from(r: &sqlx::postgres::PgRow, now: i64) -> ModResult<NoticeRow> {
    let kind: String = r.try_get("kind")?;
    let state: String = r.try_get("state")?;
    let state =
        NoticeState::parse(&state).ok_or_else(|| ModError::Storage("notice state".into()))?;
    let triage_due: i64 = r.try_get("triage_due_ms")?;
    let summary = NoticeSummary {
        id: r.try_get("id")?,
        kind: TakedownKind::parse(&kind).ok_or_else(|| ModError::Storage("notice kind".into()))?,
        state,
        notifier_name: r.try_get("notifier_name")?,
        received_at_ms: r.try_get("received_ms")?,
        triage_due_at_ms: triage_due,
        triage_overdue: matches!(state, NoticeState::Received | NoticeState::Triage)
            && now > triage_due,
        case_id: r.try_get("case_id")?,
        items: r.try_get("items")?,
        removed_at_ms: r.try_get("removed_ms")?,
        restore_after_ms: r.try_get("restore_after_ms")?,
        court_filing_noted: r.try_get("court_filing_noted")?,
    };
    Ok(NoticeRow {
        summary,
        notifier_contact: r.try_get("notifier_contact")?,
        legal_basis: r.try_get("legal_basis")?,
        locations: r.try_get("locations")?,
        original_text: r.try_get("original_text")?,
        entered_by: r.try_get("entered_by")?,
        rejection_reason: r.try_get("rejection_reason")?,
        counter_notice_text: r.try_get("counter_notice_text")?,
        counter_ms: r.try_get("counter_ms")?,
        restored_ms: r.try_get("restored_ms")?,
        restore_reason: r.try_get("restore_reason")?,
        counter_notice_by: r.try_get("counter_notice_by")?,
        removed_by: r.try_get("removed_by")?,
    })
}

/// Load a notice and lock it for the rest of the transaction.
async fn lock_notice(conn: &mut PgConnection, id: i64, now: i64) -> ModResult<NoticeRow> {
    let sql = format!("{NOTICE_SELECT} WHERE n.id = $1 FOR UPDATE OF n");
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(ModError::NotFound("takedown notice"))?;
    notice_from(&row, now)
}

fn item_from(r: &sqlx::postgres::PgRow) -> ModResult<TakedownItem> {
    let kind: String = r.try_get("kind")?;
    let state: String = r.try_get("state")?;
    Ok(TakedownItem {
        id: r.try_get("id")?,
        kind: ItemKind::parse(&kind).ok_or_else(|| ModError::Storage("item kind".into()))?,
        target_id: r.try_get("target_id")?,
        label: r.try_get("label")?,
        subject_account_id: r.try_get("subject_account_id")?,
        state: ItemState::parse(&state).ok_or_else(|| ModError::Storage("item state".into()))?,
        hold_id: r.try_get("hold_id")?,
        disabled_at_ms: r.try_get("disabled_ms")?,
        restored_at_ms: r.try_get("restored_ms")?,
    })
}

const ITEM_SELECT: &str = "SELECT id, kind, target_id, label, subject_account_id, state, hold_id, \
    (extract(epoch FROM disabled_at) * 1000)::bigint AS disabled_ms, \
    (extract(epoch FROM restored_at) * 1000)::bigint AS restored_ms \
    FROM takedown_items";

async fn items_of(conn: &mut PgConnection, notice_id: i64) -> ModResult<Vec<TakedownItem>> {
    let sql = format!("{ITEM_SELECT} WHERE notice_id = $1 ORDER BY id");
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(notice_id)
        .fetch_all(&mut *conn)
        .await?;
    rows.iter().map(item_from).collect()
}

async fn events_of(conn: &mut PgConnection, notice_id: i64) -> ModResult<Vec<NoticeEvent>> {
    let rows = sqlx::query(
        "SELECT id, actor_id, kind, detail, (extract(epoch FROM created_at) * 1000)::bigint AS at_ms \
         FROM takedown_events WHERE notice_id = $1 ORDER BY id",
    )
    .bind(notice_id)
    .fetch_all(&mut *conn)
    .await?;
    rows.iter()
        .map(|r| {
            Ok(NoticeEvent {
                id: r.try_get("id")?,
                actor_id: r.try_get("actor_id")?,
                kind: r.try_get("kind")?,
                detail: r.try_get("detail")?,
                created_at_ms: r.try_get("at_ms")?,
            })
        })
        .collect::<Result<_, sqlx::Error>>()
        .map_err(Into::into)
}

async fn push_notice_event(
    conn: &mut PgConnection,
    id: i64,
    notice_id: i64,
    actor: Option<i64>,
    kind: &str,
    detail: Value,
) -> ModResult<()> {
    sqlx::query(
        "INSERT INTO takedown_events (id, notice_id, actor_id, kind, detail) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(notice_id)
    .bind(actor)
    .bind(kind)
    .bind(detail)
    .execute(&mut *conn)
    .await?;
    sqlx::query("UPDATE takedown_notices SET updated_at = now() WHERE id = $1")
        .bind(notice_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Change a notice's state, refusing a move the lifecycle does not allow.
async fn move_state(
    conn: &mut PgConnection,
    id: i64,
    from: NoticeState,
    to: NoticeState,
) -> ModResult<()> {
    if !can_move(from, to) {
        return Err(conflict(format!(
            "A notice that is {} cannot become {}",
            from.as_str(),
            to.as_str()
        )));
    }
    let n = sqlx::query(
        "UPDATE takedown_notices SET state = $2, updated_at = now() WHERE id = $1 AND state = $3",
    )
    .bind(id)
    .bind(to.as_str())
    .bind(from.as_str())
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(conflict("The notice changed; reload it"));
    }
    Ok(())
}

/// Close (or move) the case a notice opened, unless somebody already closed it.
async fn case_to(
    conn: &mut PgConnection,
    ids: &IdSource,
    case_id: Option<i64>,
    actor: Option<i64>,
    to: &str,
    outcome: &str,
) -> ModResult<()> {
    let Some(case_id) = case_id else {
        return Ok(());
    };
    let closing = to == "closed";
    let n = sqlx::query(
        "UPDATE moderation_cases SET state = $2, updated_at = now(), \
         closed_at = CASE WHEN $2 = 'closed' THEN now() ELSE closed_at END, \
         outcome = CASE WHEN $3 <> '' THEN $3 ELSE outcome END, \
         first_action_at = COALESCE(first_action_at, now()) \
         WHERE id = $1 AND state <> 'closed'",
    )
    .bind(case_id)
    .bind(to)
    .bind(outcome)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if n > 0 {
        push_event(
            conn,
            ids(),
            case_id,
            actor,
            "state_changed",
            json!({ "to": to, "closing": closing, "outcome": outcome, "by": "takedown" }),
        )
        .await?;
    }
    Ok(())
}

/// What the register needs to know about a piece of content before it can be named in a notice.
struct Resolved {
    label: String,
    account: Option<i64>,
    person: Option<i64>,
}

async fn person_of(conn: &mut PgConnection, account: Option<i64>) -> ModResult<Option<i64>> {
    let Some(a) = account else { return Ok(None) };
    Ok(
        sqlx::query_scalar("SELECT person_id FROM accounts WHERE id = $1")
            .bind(a)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

async fn resolve_item(conn: &mut PgConnection, kind: ItemKind, id: i64) -> ModResult<Resolved> {
    let (label, account): (String, Option<i64>) = match kind {
        ItemKind::Message => {
            let r = sqlx::query("SELECT author_id FROM messages WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?
                .ok_or(ModError::NotFound("message"))?;
            (format!("Message {id}"), Some(r.try_get("author_id")?))
        }
        ItemKind::Attachment => {
            let r = sqlx::query("SELECT filename, uploader_id FROM attachments WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?
                .ok_or(ModError::NotFound("attachment"))?;
            let name: String = r.try_get("filename")?;
            (
                format!("Attachment {name}"),
                Some(r.try_get("uploader_id")?),
            )
        }
        ItemKind::LibrarySeries => {
            let r = sqlx::query("SELECT title, created_by FROM reading_series WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?
                .ok_or(ModError::NotFound("library series"))?;
            let title: String = r.try_get("title")?;
            (format!("Series {title}"), r.try_get("created_by")?)
        }
        ItemKind::LibraryChapter => {
            let r = sqlx::query(
                "SELECT s.title, c.number::text AS number, c.created_by \
                 FROM reading_chapters c JOIN reading_series s ON s.id = c.series_id WHERE c.id = $1",
            )
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or(ModError::NotFound("library chapter"))?;
            let title: String = r.try_get("title")?;
            let number: String = r.try_get("number")?;
            (
                format!("{title}, chapter {number}"),
                r.try_get("created_by")?,
            )
        }
        ItemKind::CinemaTitle => {
            let r = sqlx::query("SELECT title, created_by FROM video_titles WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?
                .ok_or(ModError::NotFound("cinema title"))?;
            let title: String = r.try_get("title")?;
            (format!("Title {title}"), r.try_get("created_by")?)
        }
        ItemKind::CinemaEpisode => {
            let r = sqlx::query(
                "SELECT t.title, e.number, t.created_by FROM video_episodes e \
                 JOIN video_titles t ON t.id = e.title_id WHERE e.id = $1",
            )
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or(ModError::NotFound("cinema episode"))?;
            let title: String = r.try_get("title")?;
            let number: i32 = r.try_get("number")?;
            (
                format!("{title}, episode {number}"),
                r.try_get("created_by")?,
            )
        }
    };
    // An episode has no uploader of its own: it belongs to its title, whose creator is the subject.
    let person = person_of(conn, account).await?;
    Ok(Resolved {
        label: label.chars().take(300).collect(),
        account,
        person,
    })
}

fn scope_of(kind: ItemKind, id: i64) -> HoldScope {
    match kind {
        ItemKind::Message => HoldScope::Message(id),
        ItemKind::Attachment => HoldScope::Attachment(id),
        ItemKind::LibrarySeries => HoldScope::LibrarySeries(id),
        ItemKind::LibraryChapter => HoldScope::LibraryChapter(id),
        ItemKind::CinemaTitle => HoldScope::CinemaTitle(id),
        ItemKind::CinemaEpisode => HoldScope::CinemaEpisode(id),
    }
}

/// An item as the code works on it.
struct ItemRow {
    id: i64,
    kind: ItemKind,
    target_id: i64,
    state: ItemState,
    hold_id: Option<i64>,
    prior: Value,
}

async fn item_rows(conn: &mut PgConnection, notice_id: i64) -> ModResult<Vec<ItemRow>> {
    let rows = sqlx::query(
        "SELECT id, kind, target_id, state, hold_id, prior FROM takedown_items WHERE notice_id = $1 ORDER BY id",
    )
    .bind(notice_id)
    .fetch_all(&mut *conn)
    .await?;
    rows.iter()
        .map(|r| {
            let kind: String = r.try_get("kind")?;
            let state: String = r.try_get("state")?;
            Ok(ItemRow {
                id: r.try_get("id")?,
                kind: ItemKind::parse(&kind)
                    .ok_or_else(|| ModError::Storage("item kind".into()))?,
                target_id: r.try_get("target_id")?,
                state: ItemState::parse(&state)
                    .ok_or_else(|| ModError::Storage("item state".into()))?,
                hold_id: r.try_get("hold_id")?,
                prior: r.try_get("prior")?,
            })
        })
        .collect()
}

/// Freeze an identified item through the hold API.
async fn hold_item(
    conn: &mut PgConnection,
    ids: &IdSource,
    notice: &NoticeRow,
    item: &ItemRow,
    actor: Option<i64>,
    reason: &str,
) -> ModResult<Option<i64>> {
    let case_id = notice
        .summary
        .case_id
        .ok_or_else(|| ModError::Storage("notice without a case".into()))?;
    let out = place_hold_in(
        conn,
        ids,
        &HoldRequest {
            case_id,
            scope: scope_of(item.kind, item.target_id),
            reason: reason.to_owned(),
            actor,
        },
    )
    .await?;
    sqlx::query("UPDATE takedown_items SET state = 'held', hold_id = $2 WHERE id = $1")
        .bind(item.id)
        .bind(out.hold_id)
        .execute(&mut *conn)
        .await?;
    Ok(out.hold_id)
}

async fn block(
    conn: &mut PgConnection,
    ids: &IdSource,
    notice_id: i64,
    kind: &str,
    value: &str,
) -> ModResult<()> {
    sqlx::query(
        "INSERT INTO blocked_objects (id, kind, value, notice_id) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
    )
    .bind(ids())
    .bind(kind)
    .bind(value)
    .bind(notice_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn block_asset(
    conn: &mut PgConnection,
    ids: &IdSource,
    notice_id: i64,
    asset: i64,
) -> ModResult<()> {
    block(conn, ids, notice_id, "hls_asset", &asset.to_string()).await?;
    block(conn, ids, notice_id, "prefix", &format!("video/{asset}/")).await?;
    block(
        conn,
        ids,
        notice_id,
        "prefix",
        &format!("originals/{asset}/"),
    )
    .await?;
    // Every transcode output of the asset, every attempt (D-335 item 2): the Patron download links and the
    // stored renditions live under `transcode/{job}/`.
    let jobs: Vec<i64> = sqlx::query_scalar("SELECT id FROM transcode_jobs WHERE asset_id = $1")
        .bind(asset)
        .fetch_all(&mut *conn)
        .await?;
    for job in jobs {
        block(conn, ids, notice_id, "prefix", &format!("transcode/{job}/")).await?;
    }
    Ok(())
}

async fn refresh_series(conn: &mut PgConnection, series: i64) -> ModResult<()> {
    sqlx::query(
        "UPDATE reading_series s SET \
           chapter_count = (SELECT count(*) FROM reading_chapters c WHERE c.series_id = s.id AND c.published), \
           last_chapter_at = (SELECT max(c.published_at) FROM reading_chapters c WHERE c.series_id = s.id AND c.published) \
         WHERE s.id = $1",
    )
    .bind(series)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn refresh_title(conn: &mut PgConnection, title: i64) -> ModResult<()> {
    sqlx::query(
        "UPDATE video_titles t SET \
           season_count = (SELECT count(DISTINCT e.season_id) FROM video_episodes e WHERE e.title_id = t.id AND e.published), \
           episode_count = (SELECT count(*) FROM video_episodes e WHERE e.title_id = t.id AND e.published), \
           last_content_at = greatest(t.published_at, \
                           (SELECT max(e.published_at) FROM video_episodes e WHERE e.title_id = t.id AND e.published)) \
         WHERE t.id = $1",
    )
    .bind(title)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Make the content of an item unavailable and block its stored objects. Returns what it looked like before, so
/// a restore puts back exactly that.
async fn disable_item(
    conn: &mut PgConnection,
    ids: &IdSource,
    notice_id: i64,
    item: &ItemRow,
) -> ModResult<Value> {
    let id = item.target_id;
    let prior = match item.kind {
        ItemKind::Message => {
            let channel: Option<i64> = sqlx::query_scalar(
                "UPDATE messages SET visibility = 'staff_removed', deleted_at = now(), pinned = FALSE, \
                 purge_after = NULL WHERE id = $1 AND visibility = 'visible' RETURNING channel_id",
            )
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
            if let Some(ch) = channel {
                sqlx::query(
                    "UPDATE thread_metadata SET message_count = GREATEST(message_count - 1, 0) WHERE channel_id = $1",
                )
                .bind(ch)
                .execute(&mut *conn)
                .await?;
            }
            let keys: Vec<String> =
                sqlx::query_scalar("SELECT storage_key FROM attachments WHERE message_id = $1")
                    .bind(id)
                    .fetch_all(&mut *conn)
                    .await?;
            for k in keys {
                block(conn, ids, notice_id, "key", &k).await?;
            }
            json!({ "was_visible": channel.is_some() })
        }
        ItemKind::Attachment => {
            let key: String =
                sqlx::query_scalar("SELECT storage_key FROM attachments WHERE id = $1")
                    .bind(id)
                    .fetch_optional(&mut *conn)
                    .await?
                    .ok_or(ModError::NotFound("attachment"))?;
            block(conn, ids, notice_id, "key", &key).await?;
            json!({})
        }
        ItemKind::LibrarySeries => {
            let was: Option<bool> =
                sqlx::query_scalar("SELECT published FROM reading_series WHERE id = $1")
                    .bind(id)
                    .fetch_optional(&mut *conn)
                    .await?;
            sqlx::query(
                "UPDATE reading_series SET published = FALSE, updated_at = now() WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *conn)
            .await?;
            block(conn, ids, notice_id, "prefix", &format!("library/{id}/")).await?;
            let cover: Option<String> =
                sqlx::query_scalar("SELECT cover_key FROM reading_series WHERE id = $1")
                    .bind(id)
                    .fetch_optional(&mut *conn)
                    .await?
                    .flatten();
            if let Some(c) = cover {
                block(conn, ids, notice_id, "key", &c).await?;
            }
            json!({ "published": was.unwrap_or(false) })
        }
        ItemKind::LibraryChapter => {
            let row = sqlx::query(
                "SELECT series_id, published, source_key FROM reading_chapters WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or(ModError::NotFound("library chapter"))?;
            let series: i64 = row.try_get("series_id")?;
            let was: bool = row.try_get("published")?;
            let source: Option<String> = row.try_get("source_key")?;
            sqlx::query(
                "UPDATE reading_chapters SET published = FALSE, updated_at = now() WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *conn)
            .await?;
            refresh_series(conn, series).await?;
            block(
                conn,
                ids,
                notice_id,
                "prefix",
                &format!("library/{series}/{id}/"),
            )
            .await?;
            if let Some(s) = source {
                block(conn, ids, notice_id, "key", &s).await?;
            }
            json!({ "published": was })
        }
        ItemKind::CinemaTitle => {
            let was: Option<bool> =
                sqlx::query_scalar("SELECT published FROM video_titles WHERE id = $1")
                    .bind(id)
                    .fetch_optional(&mut *conn)
                    .await?;
            sqlx::query(
                "UPDATE video_titles SET published = FALSE, updated_at = now() WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *conn)
            .await?;
            refresh_title(conn, id).await?;
            block(conn, ids, notice_id, "prefix", &format!("cinema/{id}/")).await?;
            let assets: Vec<i64> = sqlx::query_scalar(
                "SELECT asset_id FROM video_titles WHERE id = $1 AND asset_id IS NOT NULL \
                 UNION SELECT asset_id FROM video_episodes WHERE title_id = $1 AND asset_id IS NOT NULL \
                 UNION SELECT asset_id FROM video_title_variants WHERE title_id = $1 AND asset_id IS NOT NULL",
            )
            .bind(id)
            .fetch_all(&mut *conn)
            .await?;
            for a in assets {
                block_asset(conn, ids, notice_id, a).await?;
            }
            json!({ "published": was.unwrap_or(false) })
        }
        ItemKind::CinemaEpisode => {
            let row = sqlx::query(
                "SELECT title_id, published, asset_id FROM video_episodes WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or(ModError::NotFound("cinema episode"))?;
            let title: i64 = row.try_get("title_id")?;
            let was: bool = row.try_get("published")?;
            let asset: Option<i64> = row.try_get("asset_id")?;
            sqlx::query(
                "UPDATE video_episodes SET published = FALSE, updated_at = now() WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *conn)
            .await?;
            refresh_title(conn, title).await?;
            if let Some(a) = asset {
                block_asset(conn, ids, notice_id, a).await?;
            }
            json!({ "published": was })
        }
    };
    sqlx::query(
        "UPDATE takedown_items SET state = 'disabled', prior = $2, disabled_at = now() WHERE id = $1",
    )
    .bind(item.id)
    .bind(&prior)
    .execute(&mut *conn)
    .await?;
    Ok(prior)
}

/// Put an item back the way it was and lift its holds. The blocks of the whole notice are lifted by the caller.
async fn enable_item(
    conn: &mut PgConnection,
    ids: &IdSource,
    item: &ItemRow,
    actor: Option<i64>,
    reason: &str,
) -> ModResult<()> {
    let id = item.target_id;
    let was_published = item
        .prior
        .get("published")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match item.kind {
        ItemKind::Message => {
            if item
                .prior
                .get("was_visible")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                let channel: Option<i64> = sqlx::query_scalar(
                    "UPDATE messages SET visibility = 'visible', deleted_at = NULL, purge_after = NULL \
                     WHERE id = $1 AND visibility = 'staff_removed' RETURNING channel_id",
                )
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?;
                if let Some(ch) = channel {
                    sqlx::query(
                        "UPDATE thread_metadata SET message_count = message_count + 1 WHERE channel_id = $1",
                    )
                    .bind(ch)
                    .execute(&mut *conn)
                    .await?;
                }
            }
        }
        ItemKind::Attachment => {}
        ItemKind::LibrarySeries if was_published => {
            sqlx::query(
                "UPDATE reading_series SET published = TRUE, updated_at = now() WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *conn)
            .await?;
        }
        ItemKind::LibraryChapter if was_published => {
            let series: Option<i64> = sqlx::query_scalar(
                "UPDATE reading_chapters SET published = TRUE, updated_at = now() WHERE id = $1 RETURNING series_id",
            )
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
            if let Some(s) = series {
                refresh_series(conn, s).await?;
            }
        }
        ItemKind::CinemaTitle if was_published => {
            sqlx::query(
                "UPDATE video_titles SET published = TRUE, updated_at = now() WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *conn)
            .await?;
            refresh_title(conn, id).await?;
        }
        ItemKind::CinemaEpisode if was_published => {
            let title: Option<i64> = sqlx::query_scalar(
                "UPDATE video_episodes SET published = TRUE, updated_at = now() WHERE id = $1 RETURNING title_id",
            )
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
            if let Some(t) = title {
                refresh_title(conn, t).await?;
            }
        }
        _ => {}
    }
    if let Some(h) = item.hold_id {
        let limits = limits_in(&mut *conn).await?;
        match release_hold_in(conn, ids, h, actor, reason, &limits).await {
            Ok(_) | Err(ModError::Conflict(_)) => {}
            Err(e) => return Err(e),
        }
    }
    sqlx::query("UPDATE takedown_items SET state = 'restored', restored_at = now() WHERE id = $1")
        .bind(item.id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Lift the media blocks of a notice.
async fn unblock(conn: &mut PgConnection, notice_id: i64) -> ModResult<()> {
    sqlx::query("DELETE FROM blocked_objects WHERE notice_id = $1")
        .bind(notice_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The accounts that uploaded or wrote the content of the items, once each.
fn subjects(items: &[TakedownItem]) -> Vec<i64> {
    let mut v: Vec<i64> = items.iter().filter_map(|i| i.subject_account_id).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Strike state for the Persons a notice names: the count inside the window, and the review case that exists.
async fn strike_counts(
    conn: &mut PgConnection,
    notice_id: i64,
    policy: &TakedownPolicy,
    now: i64,
) -> ModResult<Vec<StrikeCount>> {
    let persons: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT subject_person_id FROM takedown_items \
         WHERE notice_id = $1 AND subject_person_id IS NOT NULL ORDER BY 1",
    )
    .bind(notice_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut out = Vec::new();
    for p in persons {
        let rows = sqlx::query(
            "SELECT n.id, n.kind, n.state, (extract(epoch FROM n.removed_at) * 1000)::bigint AS at_ms \
             FROM takedown_notices n \
             WHERE n.state IN ('removed', 'counter_noticed') AND n.removed_at IS NOT NULL \
               AND EXISTS (SELECT 1 FROM takedown_items i WHERE i.notice_id = n.id AND i.subject_person_id = $1) \
             ORDER BY n.removed_at DESC",
        )
        .bind(p)
        .fetch_all(&mut *conn)
        .await?;
        let start = crate::takedown_rules::months_before(now, policy.strike_window_months);
        let mut counted = Vec::new();
        let mut entries = Vec::new();
        for r in &rows {
            let (kind, state): (String, String) = (r.try_get("kind")?, r.try_get("state")?);
            let at: i64 = r.try_get("at_ms")?;
            let contested = state == "counter_noticed";
            entries.push((at, contested));
            if at >= start && at <= now {
                counted.push(CountedNotice {
                    id: r.try_get("id")?,
                    kind: TakedownKind::parse(&kind)
                        .ok_or_else(|| ModError::Storage("notice kind".into()))?,
                    state: NoticeState::parse(&state)
                        .ok_or_else(|| ModError::Storage("notice state".into()))?,
                    removed_at_ms: at,
                    contested,
                });
            }
        }
        let (strikes, contested) =
            strikes_and_contested(&entries, now, policy.strike_window_months);
        let review: Option<i64> = sqlx::query_scalar(
            "SELECT r.case_id FROM takedown_strike_reviews r WHERE r.person_id = $1 ORDER BY r.id DESC LIMIT 1",
        )
        .bind(p)
        .fetch_optional(&mut *conn)
        .await?;
        out.push(StrikeCount {
            person_id: p,
            strikes: i64::try_from(strikes).unwrap_or(0),
            contested: i64::try_from(contested).unwrap_or(0),
            counted,
            window_months: i64::from(policy.strike_window_months),
            threshold: i64::from(policy.strike_review_threshold),
            review_case_id: review,
        });
    }
    Ok(out)
}

/// After a notice is upheld: for each Person it names, open a review case when the count inside the window
/// reaches the threshold and no review of theirs is still open. Never suspends (D-293).
async fn open_reviews(
    conn: &mut PgConnection,
    ids: &IdSource,
    notice: &NoticeRow,
    actor: i64,
    policy: &TakedownPolicy,
    now: i64,
) -> ModResult<Option<i64>> {
    let counts = strike_counts(conn, notice.summary.id, policy, now).await?;
    let mut opened = None;
    for c in counts {
        if !review_due(
            usize::try_from(c.strikes).unwrap_or(0),
            policy.strike_review_threshold,
        ) {
            continue;
        }
        let open_review: Option<i64> = sqlx::query_scalar(
            "SELECT r.case_id FROM takedown_strike_reviews r \
             JOIN moderation_cases c ON c.id = r.case_id \
             WHERE r.person_id = $1 AND c.state <> 'closed' LIMIT 1",
        )
        .bind(c.person_id)
        .fetch_optional(&mut *conn)
        .await?;
        if open_review.is_some() {
            continue;
        }
        let account: Option<i64> = sqlx::query_scalar(
            "SELECT subject_account_id FROM takedown_items \
             WHERE notice_id = $1 AND subject_person_id = $2 AND subject_account_id IS NOT NULL LIMIT 1",
        )
        .bind(notice.summary.id)
        .bind(c.person_id)
        .fetch_optional(&mut *conn)
        .await?;
        let case_id = ids();
        let summary = format!(
            "Repeat takedowns: {} upheld in {} months",
            c.strikes, c.window_months
        );
        sqlx::query(
            "INSERT INTO moderation_cases (id, category, severity, routing, summary, target_kind, target_id, \
             subject_person_id, subject_account_id, opened_by_kind, opened_by_id) \
             VALUES ($1, 'illegal_content', 'standard', 'platform', $2, 'person', $3, $3, $4, 'staff', $5)",
        )
        .bind(case_id)
        .bind(summary)
        .bind(c.person_id)
        .bind(account)
        .bind(actor)
        .execute(&mut *conn)
        .await?;
        push_event(
            conn,
            ids(),
            case_id,
            None,
            "strike_review_opened",
            json!({ "strikes": c.strikes, "contested": c.contested, "window_months": c.window_months,
                    "threshold": c.threshold, "notice_id": notice.summary.id.to_string(),
                    "counted": c.counted.iter().map(|n| json!({
                        "reference": notice_ref(n.id), "kind": n.kind.as_str(), "state": n.state.as_str(),
                        "removed_at_ms": n.removed_at_ms, "contested": n.contested })).collect::<Vec<_>>() }),
        )
        .await?;
        let mut note = format!(
            "Opened at {} of {} upheld takedowns in {} months ({} contested by a counter-notice). Counted:\n",
            c.strikes, c.threshold, c.window_months, c.contested
        );
        for n in &c.counted {
            note.push_str(&format!(
                "- {} ({}, {}, removed {}){}\n",
                notice_ref(n.id),
                kind_word(n.kind),
                n.state.as_str(),
                crate::takedown_rules::iso_utc(n.removed_at_ms),
                if n.contested {
                    " - counter-notice pending"
                } else {
                    ""
                }
            ));
        }
        sqlx::query(
            "INSERT INTO moderation_case_notes (id, case_id, author_id, body) VALUES ($1, $2, $3, $4)",
        )
        .bind(ids())
        .bind(case_id)
        .bind(actor)
        .bind(note)
        .execute(&mut *conn)
        .await?;
        sqlx::query(
            "INSERT INTO takedown_strike_reviews (id, person_id, case_id, strikes) VALUES ($1, $2, $3, $4)",
        )
        .bind(ids())
        .bind(c.person_id)
        .bind(case_id)
        .bind(i32::try_from(c.strikes).unwrap_or(i32::MAX))
        .execute(&mut *conn)
        .await?;
        opened = Some(case_id);
    }
    Ok(opened)
}

fn kind_word(k: TakedownKind) -> &'static str {
    match k {
        TakedownKind::Copyright => "copyright",
        TakedownKind::IllegalContent => "illegal-content",
    }
}

fn clip(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}...")
    } else {
        t
    }
}

fn removal_dm(
    kind: TakedownKind,
    labels: &[String],
    basis: &str,
    reference: &str,
    policy: &TakedownPolicy,
) -> String {
    let how = match &policy.operator_contact {
        Some(c) => format!(
            "If you believe this was a mistake, send a counter-notice to {c} and quote {reference}. Staff record it, and the content returns after ten business days unless the sender goes to court."
        ),
        None => format!(
            "If you believe this was a mistake, tell the operator and quote {reference}. Staff record a counter-notice, and the content returns after ten business days unless the sender goes to court."
        ),
    };
    format!(
        "Content of yours was removed after a {} notice ({reference}).\n\nRemoved: {}\nWhat the notice says: {}\n\nWe do not decide whether a notice is right in law; we record it and act on it. {how}",
        kind_word(kind),
        labels.join("; "),
        clip(basis, 300),
    )
}

fn restore_dm(labels: &[String], reference: &str) -> String {
    format!(
        "Content of yours is available again ({reference}): {}.",
        labels.join("; ")
    )
}

/// The case a new notice opens: the platform's, `illegal_content` (the copyright category of the spec is the
/// notice kind; both are handled by platform staff, D-209).
async fn open_notice_case(
    conn: &mut PgConnection,
    ids: &IdSource,
    actor: i64,
    notice_id: i64,
    kind: TakedownKind,
    notifier: &str,
) -> ModResult<i64> {
    let case_id = ids();
    let summary = clip(
        &format!(
            "{} takedown notice from {notifier}",
            capital(kind_word(kind))
        ),
        200,
    );
    sqlx::query(
        "INSERT INTO moderation_cases (id, category, severity, routing, summary, target_kind, \
         opened_by_kind, opened_by_id) VALUES ($1, 'illegal_content', 'serious', 'platform', $2, 'other', 'staff', $3)",
    )
    .bind(case_id)
    .bind(summary)
    .bind(actor)
    .execute(&mut *conn)
    .await?;
    push_event(
        conn,
        ids(),
        case_id,
        Some(actor),
        "case_opened",
        json!({ "by": "staff", "category": "illegal_content", "takedown": notice_ref(notice_id) }),
    )
    .await?;
    Ok(case_id)
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn clean_list(v: &[String]) -> ModResult<Vec<String>> {
    if v.len() > MAX_LOCATIONS {
        return Err(invalid(format!("at most {MAX_LOCATIONS} locations")));
    }
    let mut out = Vec::new();
    for s in v {
        let t = s.trim();
        if t.is_empty() {
            continue;
        }
        if t.chars().count() > 1000 {
            return Err(invalid("a location must be 1000 characters or fewer"));
        }
        out.push(t.to_owned());
    }
    Ok(out)
}

impl Moderation {
    fn tk_staff(who: &Who<'_>) -> ModResult<()> {
        require_staff(who, P::MANAGE_TAKEDOWNS, "MANAGE_TAKEDOWNS")
    }

    async fn weekend(&self) -> Weekend {
        Weekend::for_country(self.policy().await.operator_country.as_deref())
    }

    async fn detail_in(
        &self,
        conn: &mut PgConnection,
        id: i64,
        now: i64,
    ) -> ModResult<NoticeDetail> {
        let sql = format!("{NOTICE_SELECT} WHERE n.id = $1");
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or(ModError::NotFound("takedown notice"))?;
        let n = notice_from(&row, now)?;
        let items = items_of(conn, id).await?;
        let events = events_of(conn, id).await?;
        let policy = self.policy().await;
        let strikes = strike_counts(conn, id, &policy, now).await?;
        Ok(NoticeDetail {
            summary: n.summary,
            notifier_contact: n.notifier_contact,
            legal_basis: n.legal_basis,
            locations: n.locations,
            original_text: n.original_text,
            entered_by: n.entered_by,
            rejection_reason: n.rejection_reason,
            counter_notice_text: n.counter_notice_text,
            counter_noticed_at_ms: n.counter_ms,
            restored_at_ms: n.restored_ms,
            restore_reason: n.restore_reason,
            items,
            events,
            strikes,
        })
    }

    async fn reload_blocklist(&self) {
        if let Some(b) = &self.blocklist {
            b.refresh().await;
        }
    }

    // ---- intake ---------------------------------------------------------------------------------------------

    /// Enter a notice (spec 5, step 1): staff only. Opens its case and sets the one-business-day triage target.
    pub async fn create_notice(&self, who: &Who<'_>, n: &NewNotice) -> ModResult<NoticeDetail> {
        Self::tk_staff(who)?;
        let name = rules::required_text("notifier_name", &n.notifier_name, 200)?;
        let contact = rules::required_text("notifier_contact", &n.notifier_contact, 300)?;
        let basis = rules::required_text("legal_basis", &n.legal_basis, MAX_FIELD)?;
        let original = rules::required_text("original_text", &n.original_text, MAX_ORIGINAL)?;
        let locations = clean_list(&n.locations)?;
        let now = now_ms();
        let received = n.received_at_ms.unwrap_or(now);
        if received > now + 86_400_000 {
            return Err(invalid("received_at_ms is in the future"));
        }
        let due = add_business_days(received, TRIAGE_BUSINESS_DAYS, self.weekend().await);
        let id = self.next_id();
        let mut tx = self.pool.begin().await?;
        let case_id =
            open_notice_case(&mut tx, &self.ids, who.user_id(), id, n.kind, &name).await?;
        sqlx::query(
            "INSERT INTO takedown_notices (id, kind, notifier_name, notifier_contact, legal_basis, locations, \
             original_text, received_at, entered_by, triage_due_at, case_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, to_timestamp($8::double precision / 1000.0), $9, \
                     to_timestamp($10::double precision / 1000.0), $11)",
        )
        .bind(id)
        .bind(n.kind.as_str())
        .bind(&name)
        .bind(&contact)
        .bind(&basis)
        .bind(&locations)
        .bind(&original)
        .bind(received)
        .bind(who.user_id())
        .bind(due)
        .bind(case_id)
        .execute(&mut *tx)
        .await?;
        push_notice_event(
            &mut tx,
            self.next_id(),
            id,
            Some(who.user_id()),
            "received",
            json!({ "kind": n.kind.as_str(), "locations": locations.len() }),
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.takedown.create",
            ("takedown", id),
            "",
            json!({ "kind": n.kind.as_str(), "case_id": case_id.to_string() }),
        )
        .await?;
        let d = self.detail_in(&mut tx, id, now).await?;
        tx.commit().await?;
        Ok(d)
    }

    /// The register, newest first, optionally one state.
    pub async fn list_notices(
        &self,
        who: &Who<'_>,
        state: Option<NoticeState>,
        limit: i64,
    ) -> ModResult<Vec<NoticeSummary>> {
        Self::tk_staff(who)?;
        let now = now_ms();
        let sql = format!(
            "{NOTICE_SELECT} WHERE ($1::text IS NULL OR n.state = $1) ORDER BY n.received_at DESC, n.id DESC LIMIT $2"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(state.map(NoticeState::as_str))
            .bind(limit.clamp(1, 500))
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(|r| notice_from(r, now).map(|n| n.summary))
            .collect()
    }

    pub async fn get_notice(&self, who: &Who<'_>, id: i64) -> ModResult<NoticeDetail> {
        Self::tk_staff(who)?;
        let mut conn = self.pool.acquire().await?;
        self.detail_in(&mut conn, id, now_ms()).await
    }

    // ---- triage ---------------------------------------------------------------------------------------------

    /// Name a piece of content the notice covers (spec 5, step 2). The first one moves the notice to triage.
    pub async fn add_item(
        &self,
        who: &Who<'_>,
        notice_id: i64,
        item: &NewItem,
    ) -> ModResult<NoticeDetail> {
        Self::tk_staff(who)?;
        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        let n = lock_notice(&mut tx, notice_id, now).await?;
        if !matches!(n.summary.state, NoticeState::Received | NoticeState::Triage) {
            return Err(conflict(
                "Items can only be added while the notice is being triaged",
            ));
        }
        let r = resolve_item(&mut tx, item.kind, item.id).await?;
        let item_id = self.next_id();
        let inserted = sqlx::query(
            "INSERT INTO takedown_items (id, notice_id, kind, target_id, label, subject_account_id, subject_person_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (notice_id, kind, target_id) DO NOTHING",
        )
        .bind(item_id)
        .bind(notice_id)
        .bind(item.kind.as_str())
        .bind(item.id)
        .bind(&r.label)
        .bind(r.account)
        .bind(r.person)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if inserted == 0 {
            return Err(conflict("That content is already on the notice"));
        }
        if n.summary.state == NoticeState::Received {
            move_state(
                &mut tx,
                notice_id,
                NoticeState::Received,
                NoticeState::Triage,
            )
            .await?;
        }
        if let Some(case_id) = n.summary.case_id {
            let (tk, tid) = match item.kind {
                ItemKind::Message => ("message", Some(item.id)),
                ItemKind::Attachment => ("other", Some(item.id)),
                ItemKind::LibrarySeries | ItemKind::LibraryChapter => ("library", Some(item.id)),
                ItemKind::CinemaTitle | ItemKind::CinemaEpisode => ("cinema", Some(item.id)),
            };
            sqlx::query(
                "UPDATE moderation_cases SET \
                   target_kind = CASE WHEN target_id IS NULL THEN $2 ELSE target_kind END, \
                   target_id = COALESCE(target_id, $3), \
                   subject_person_id = COALESCE(subject_person_id, $4), \
                   subject_account_id = COALESCE(subject_account_id, $5), \
                   state = CASE WHEN state = 'open' THEN 'triage' ELSE state END \
                 WHERE id = $1",
            )
            .bind(case_id)
            .bind(tk)
            .bind(tid)
            .bind(r.person)
            .bind(r.account)
            .execute(&mut *tx)
            .await?;
        }
        push_notice_event(
            &mut tx,
            self.next_id(),
            notice_id,
            Some(who.user_id()),
            "item_added",
            json!({ "kind": item.kind.as_str(), "target_id": item.id.to_string(), "label": r.label }),
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.takedown.item_add",
            ("takedown", notice_id),
            "",
            json!({ "kind": item.kind.as_str(), "target_id": item.id.to_string() }),
        )
        .await?;
        let d = self.detail_in(&mut tx, notice_id, now).await?;
        tx.commit().await?;
        Ok(d)
    }

    /// Take back an item named by mistake (only before it is held).
    pub async fn remove_item(
        &self,
        who: &Who<'_>,
        notice_id: i64,
        item_id: i64,
    ) -> ModResult<NoticeDetail> {
        Self::tk_staff(who)?;
        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        let n = lock_notice(&mut tx, notice_id, now).await?;
        if !matches!(n.summary.state, NoticeState::Received | NoticeState::Triage) {
            return Err(conflict(
                "Items can only be removed while the notice is being triaged",
            ));
        }
        let state: Option<String> = sqlx::query_scalar(
            "SELECT state FROM takedown_items WHERE id = $1 AND notice_id = $2 FOR UPDATE",
        )
        .bind(item_id)
        .bind(notice_id)
        .fetch_optional(&mut *tx)
        .await?;
        match state.as_deref() {
            None => return Err(ModError::NotFound("takedown item")),
            Some("identified") => {}
            Some(_) => return Err(conflict("A held item cannot be taken off the notice")),
        }
        // The only delete the register allows on items: one that never reached a hold (the trigger agrees).
        sqlx::query("DELETE FROM takedown_items WHERE id = $1 AND state = 'identified'")
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
        push_notice_event(
            &mut tx,
            self.next_id(),
            notice_id,
            Some(who.user_id()),
            "item_removed",
            json!({ "item_id": item_id.to_string() }),
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.takedown.item_remove",
            ("takedown", notice_id),
            "",
            json!({ "item_id": item_id.to_string() }),
        )
        .await?;
        let d = self.detail_in(&mut tx, notice_id, now).await?;
        tx.commit().await?;
        Ok(d)
    }
    // ---- hold, reject, uphold -------------------------------------------------------------------------------

    /// Freeze the identified items for evidence without making anything unavailable yet (spec 5, step 3: "hold,
    /// then disable"). Optional: upholding does this itself.
    pub async fn hold_notice(
        &self,
        who: &Who<'_>,
        id: i64,
        reason: Option<&str>,
    ) -> ModResult<NoticeDetail> {
        Self::tk_staff(who)?;
        let reason = rules::reason(reason)?;
        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        let n = lock_notice(&mut tx, id, now).await?;
        if n.summary.state != NoticeState::Triage {
            return Err(conflict("Only a notice in triage can be held"));
        }
        let mut held = 0_i64;
        for it in item_rows(&mut tx, id).await? {
            if it.state == ItemState::Identified {
                hold_item(&mut tx, &self.ids, &n, &it, Some(who.user_id()), &reason).await?;
                held += 1;
            }
        }
        if held == 0 {
            return Err(conflict("Nothing left to hold"));
        }
        push_notice_event(
            &mut tx,
            self.next_id(),
            id,
            Some(who.user_id()),
            "held",
            json!({ "items": held, "reason": reason }),
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.takedown.hold",
            ("takedown", id),
            &reason,
            json!({ "items": held }),
        )
        .await?;
        let d = self.detail_in(&mut tx, id, now).await?;
        tx.commit().await?;
        Ok(d)
    }

    /// Reject an incomplete notice (spec 5, step 2): closes it and its case, lifts any hold, and returns the
    /// templated reply for staff to send to the notifier. `reason` is what staff found missing, in their words;
    /// the standard gaps are listed too.
    pub async fn reject_notice(
        &self,
        who: &Who<'_>,
        id: i64,
        reason: Option<&str>,
    ) -> ModResult<RejectOutcome> {
        Self::tk_staff(who)?;
        let reason = rules::reason(reason)?;
        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        let n = lock_notice(&mut tx, id, now).await?;
        let rows = item_rows(&mut tx, id).await?;
        let mut missing: Vec<&str> = missing_parts(
            &n.summary.notifier_name,
            &n.notifier_contact,
            &n.legal_basis,
            &n.locations,
            rows.len(),
        );
        missing.push(&reason);
        let reply = incomplete_reply(n.summary.kind, &missing, &notice_ref(id));
        move_state(&mut tx, id, n.summary.state, NoticeState::Rejected).await?;
        sqlx::query(
            "UPDATE takedown_notices SET rejected_at = now(), rejection_reason = $2 WHERE id = $1",
        )
        .bind(id)
        .bind(&reason)
        .execute(&mut *tx)
        .await?;
        // Anything held for evidence while the notice was looked at goes back.
        for it in rows {
            if it.state == ItemState::Held {
                enable_item(&mut tx, &self.ids, &it, Some(who.user_id()), &reason).await?;
            }
        }
        case_to(
            &mut tx,
            &self.ids,
            n.summary.case_id,
            Some(who.user_id()),
            "closed",
            "Rejected as incomplete",
        )
        .await?;
        push_notice_event(
            &mut tx,
            self.next_id(),
            id,
            Some(who.user_id()),
            "rejected",
            json!({ "reason": reason }),
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.takedown.reject",
            ("takedown", id),
            &reason,
            json!({}),
        )
        .await?;
        tx.commit().await?;
        Ok(RejectOutcome { reply })
    }

    /// Uphold a notice: hold what is not held yet, make every item unavailable, block the stored objects, tell
    /// the uploaders, count the strike and, if a Person reached the threshold, open a review case (D-293).
    pub async fn uphold_notice(
        &self,
        who: &Who<'_>,
        id: i64,
        reason: Option<&str>,
    ) -> ModResult<UpholdOutcome> {
        Self::tk_staff(who)?;
        let reason = rules::reason(reason)?;
        let policy = self.policy().await;
        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        let n = lock_notice(&mut tx, id, now).await?;
        if n.summary.state != NoticeState::Triage {
            return Err(conflict(
                "Name the content first: only a notice in triage can be upheld",
            ));
        }
        let rows = item_rows(&mut tx, id).await?;
        if rows.is_empty() {
            return Err(conflict("The notice names no content yet"));
        }
        let mut disabled = 0_i64;
        for mut it in rows {
            if it.state == ItemState::Identified {
                it.hold_id =
                    hold_item(&mut tx, &self.ids, &n, &it, Some(who.user_id()), &reason).await?;
            }
            disable_item(&mut tx, &self.ids, id, &it).await?;
            disabled += 1;
        }
        move_state(&mut tx, id, NoticeState::Triage, NoticeState::Removed).await?;
        sqlx::query(
            "UPDATE takedown_notices SET removed_at = to_timestamp($2::double precision / 1000.0), \
             removed_by = $3 WHERE id = $1",
        )
        .bind(id)
        .bind(now)
        .bind(who.user_id())
        .execute(&mut *tx)
        .await?;
        case_to(
            &mut tx,
            &self.ids,
            n.summary.case_id,
            Some(who.user_id()),
            "actioned",
            "Takedown upheld",
        )
        .await?;
        let review = open_reviews(&mut tx, &self.ids, &n, who.user_id(), &policy, now).await?;
        push_notice_event(
            &mut tx,
            self.next_id(),
            id,
            Some(who.user_id()),
            "upheld",
            json!({ "items": disabled, "reason": reason,
                    "review_case_id": review.map(|c| c.to_string()) }),
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.takedown.uphold",
            ("takedown", id),
            &reason,
            json!({ "items": disabled, "review_case_id": review.map(|c| c.to_string()) }),
        )
        .await?;
        let items = items_of(&mut tx, id).await?;
        tx.commit().await?;
        self.announce_removed_messages(id).await;
        self.reload_blocklist().await;
        let notified = self
            .notify_subjects(&items, |labels| {
                removal_dm(
                    n.summary.kind,
                    labels,
                    &n.legal_basis,
                    &notice_ref(id),
                    &policy,
                )
            })
            .await;
        Ok(UpholdOutcome {
            items_disabled: disabled,
            notified,
            review_case_id: review,
        })
    }

    /// Tell live clients about every message the notice removed from view (once per message, after the commit).
    async fn announce_removed_messages(&self, notice_id: i64) {
        let Some(sink) = &self.removals else { return };
        let rows = sqlx::query(
            "SELECT m.id, m.channel_id, m.guild_id FROM takedown_items i JOIN messages m ON m.id = i.target_id \n             WHERE i.notice_id = $1 AND i.kind = 'message' AND (i.prior->>'was_visible')::boolean",
        )
        .bind(notice_id)
        .fetch_all(&self.pool)
        .await;
        let Ok(rows) = rows else { return };
        for r in rows {
            if let (Ok(id), Ok(channel), Ok(guild)) = (
                r.try_get::<i64, _>("id"),
                r.try_get::<i64, _>("channel_id"),
                r.try_get::<Option<i64>, _>("guild_id"),
            ) {
                sink.message_removed(guild, channel, id).await;
            }
        }
    }

    /// DM each uploader or author once with the labels of their own items. Returns how many were delivered.
    async fn notify_subjects(
        &self,
        items: &[TakedownItem],
        text: impl Fn(&[String]) -> String,
    ) -> i64 {
        let mut delivered = 0;
        for account in subjects(items) {
            let labels: Vec<String> = items
                .iter()
                .filter(|i| i.subject_account_id == Some(account))
                .map(|i| i.label.clone())
                .collect();
            if self.dm_subject(account, &text(&labels)).await {
                delivered += 1;
            }
        }
        delivered
    }

    // ---- counter-notice, court filing, restore ------------------------------------------------------------------

    /// Record a counter-notice (spec 5, step 5; staff record it, there is no public form). The restore is
    /// scheduled for ten business days later, counted in the operator country's weekdays with no holiday
    /// calendar (D-293).
    pub async fn record_counter_notice(
        &self,
        who: &Who<'_>,
        id: i64,
        text: &str,
    ) -> ModResult<NoticeDetail> {
        Self::tk_staff(who)?;
        let text = rules::required_text("counter-notice", text, 20_000)?;
        let now = now_ms();
        let restore_after = add_business_days(now, RESTORE_BUSINESS_DAYS, self.weekend().await);
        let mut tx = self.pool.begin().await?;
        let n = lock_notice(&mut tx, id, now).await?;
        sqlx::query(
            "UPDATE takedown_notices SET counter_noticed_at = to_timestamp($2::double precision / 1000.0), \
             counter_notice_by = $3, counter_notice_text = $4, \
             restore_after = to_timestamp($5::double precision / 1000.0) WHERE id = $1",
        )
        .bind(id)
        .bind(now)
        .bind(who.user_id())
        .bind(&text)
        .bind(restore_after)
        .execute(&mut *tx)
        .await?;
        move_state(&mut tx, id, n.summary.state, NoticeState::CounterNoticed).await?;
        push_notice_event(
            &mut tx,
            self.next_id(),
            id,
            Some(who.user_id()),
            "counter_notice",
            json!({ "restore_after_ms": restore_after }),
        )
        .await?;
        if let Some(c) = n.summary.case_id {
            push_event(
                &mut tx,
                self.next_id(),
                c,
                Some(who.user_id()),
                "counter_notice",
                json!({ "notice": notice_ref(id), "restore_after_ms": restore_after }),
            )
            .await?;
        }
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.takedown.counter_notice",
            ("takedown", id),
            "",
            json!({ "restore_after_ms": restore_after }),
        )
        .await?;
        let d = self.detail_in(&mut tx, id, now).await?;
        tx.commit().await?;
        Ok(d)
    }

    /// Note (or clear) that the notifier went to court: while it is noted, the housekeeping job never restores
    /// the content (D-293). Needs a reason.
    pub async fn set_court_filing(
        &self,
        who: &Who<'_>,
        id: i64,
        noted: bool,
        reason: Option<&str>,
    ) -> ModResult<NoticeDetail> {
        Self::tk_staff(who)?;
        let reason = rules::reason(reason)?;
        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        let n = lock_notice(&mut tx, id, now).await?;
        if !matches!(
            n.summary.state,
            NoticeState::Removed | NoticeState::CounterNoticed
        ) {
            return Err(conflict(
                "A court filing can only be noted on a notice that is in force",
            ));
        }
        sqlx::query(
            "UPDATE takedown_notices SET court_filing_noted = $2, \
             court_filing_at = CASE WHEN $2 THEN now() ELSE NULL END, \
             court_filing_by = CASE WHEN $2 THEN $3 ELSE NULL END, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(noted)
        .bind(who.user_id())
        .execute(&mut *tx)
        .await?;
        push_notice_event(
            &mut tx,
            self.next_id(),
            id,
            Some(who.user_id()),
            if noted {
                "court_filing_noted"
            } else {
                "court_filing_cleared"
            },
            json!({ "reason": reason }),
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            "moderation.takedown.court_filing",
            ("takedown", id),
            &reason,
            json!({ "noted": noted }),
        )
        .await?;
        let d = self.detail_in(&mut tx, id, now).await?;
        tx.commit().await?;
        Ok(d)
    }

    /// A restore or withdrawal can take a Person under the review threshold: a review that is still open then
    /// closes as `no_action` (D-333), with one timeline event.
    async fn close_reviews_if_under(
        &self,
        conn: &mut PgConnection,
        notice_id: i64,
        actor: Option<i64>,
    ) -> ModResult<()> {
        let policy = self.policy().await;
        for c in strike_counts(conn, notice_id, &policy, now_ms()).await? {
            let Some(case) = c.review_case_id else {
                continue;
            };
            if !review_due(
                usize::try_from(c.strikes).unwrap_or(0),
                policy.strike_review_threshold,
            ) {
                case_to(
                    conn,
                    &self.ids,
                    Some(case),
                    actor,
                    "closed",
                    "no_action: count fell under the threshold",
                )
                .await?;
            }
        }
        Ok(())
    }

    /// Put every item of a notice back, lift the blocks and holds, and end the notice in `to` (`Restored` or
    /// `Withdrawn`). Writes the notice event and closes the case; the caller logs the access row.
    /// `actor` is who the notice records (none for the job); `release_as` is the staff account the holds are
    /// released under.
    async fn restore_in(
        &self,
        conn: &mut PgConnection,
        n: &NoticeRow,
        to: NoticeState,
        actor: Option<i64>,
        release_as: i64,
        why: (&str, &str),
    ) -> ModResult<()> {
        let (reason, by) = why;
        let id = n.summary.id;
        move_state(conn, id, n.summary.state, to).await?;
        for it in item_rows(conn, id).await? {
            if matches!(it.state, ItemState::Held | ItemState::Disabled) {
                enable_item(conn, &self.ids, &it, Some(release_as), reason).await?;
            }
        }
        unblock(conn, id).await?;
        self.close_reviews_if_under(conn, id, actor).await?;
        sqlx::query(
            "UPDATE takedown_notices SET \
             restored_at = CASE WHEN $2 = 'restored' THEN now() ELSE restored_at END, \
             withdrawn_at = CASE WHEN $2 = 'withdrawn' THEN now() ELSE withdrawn_at END, \
             restored_by = $3, restore_reason = $4 WHERE id = $1",
        )
        .bind(id)
        .bind(to.as_str())
        .bind(actor)
        .bind(reason)
        .execute(&mut *conn)
        .await?;
        case_to(
            conn,
            &self.ids,
            n.summary.case_id,
            actor,
            "closed",
            if to == NoticeState::Withdrawn {
                "Notice withdrawn"
            } else {
                "Content restored"
            },
        )
        .await?;
        push_notice_event(
            conn,
            self.next_id(),
            id,
            actor,
            to.as_str(),
            json!({ "reason": reason, "by": by }),
        )
        .await?;
        Ok(())
    }

    /// Restore the content by hand, before or without the wait (spec 5, step 5). A logged action.
    pub async fn restore_notice(
        &self,
        who: &Who<'_>,
        id: i64,
        reason: Option<&str>,
    ) -> ModResult<NoticeDetail> {
        self.end_notice(who, id, reason, NoticeState::Restored)
            .await
    }

    /// The notifier withdrew the notice: the content comes back and no strike stands.
    pub async fn withdraw_notice(
        &self,
        who: &Who<'_>,
        id: i64,
        reason: Option<&str>,
    ) -> ModResult<NoticeDetail> {
        self.end_notice(who, id, reason, NoticeState::Withdrawn)
            .await
    }

    async fn end_notice(
        &self,
        who: &Who<'_>,
        id: i64,
        reason: Option<&str>,
        to: NoticeState,
    ) -> ModResult<NoticeDetail> {
        Self::tk_staff(who)?;
        let reason = rules::reason(reason)?;
        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        let n = lock_notice(&mut tx, id, now).await?;
        self.restore_in(
            &mut tx,
            &n,
            to,
            Some(who.user_id()),
            who.user_id(),
            (&reason, "staff"),
        )
        .await?;
        log_staff(
            &mut tx,
            who,
            AccessKind::AdminAction,
            if to == NoticeState::Withdrawn {
                "moderation.takedown.withdraw"
            } else {
                "moderation.takedown.restore"
            },
            ("takedown", id),
            &reason,
            json!({}),
        )
        .await?;
        let items = items_of(&mut tx, id).await?;
        let d = self.detail_in(&mut tx, id, now).await?;
        tx.commit().await?;
        self.reload_blocklist().await;
        let reference = notice_ref(id);
        self.notify_subjects(&items, |labels| restore_dm(labels, &reference))
            .await;
        Ok(d)
    }

    /// The housekeeping job's work (D-293): restore every notice whose counter-notice wait is over, unless staff
    /// noted a court filing. `now_ms` is a parameter so tests can move the clock. Each restore is its own
    /// transaction with an access-log row, attributed to the staff member who recorded the counter-notice and
    /// marked `housekeeping`. Returns the notices restored.
    pub async fn run_due_restores(&self, now_ms: i64) -> ModResult<Vec<i64>> {
        let due: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM takedown_notices WHERE state = 'counter_noticed' AND NOT court_filing_noted \
             AND restore_after <= to_timestamp($1::double precision / 1000.0) ORDER BY restore_after LIMIT 50",
        )
        .bind(now_ms)
        .fetch_all(&self.pool)
        .await?;
        let mut restored = Vec::new();
        for id in due {
            let mut tx = self.pool.begin().await?;
            let n = lock_notice(&mut tx, id, now_ms).await?;
            if !restore_due(
                n.summary.state,
                n.summary.restore_after_ms,
                n.summary.court_filing_noted,
                now_ms,
            ) {
                continue;
            }
            let staff = n.counter_notice_by.or(n.removed_by).unwrap_or(n.entered_by);
            self.restore_in(
                &mut tx,
                &n,
                NoticeState::Restored,
                None,
                staff,
                (
                    "The counter-notice wait is over and no court filing was noted",
                    "housekeeping",
                ),
            )
            .await?;
            sr_db::admin_repo::append_in(
                &mut tx,
                &AuditEntry {
                    actor_id: staff,
                    kind: AccessKind::AdminAction,
                    action: "moderation.takedown.auto_restore".to_owned(),
                    target_kind: Some("takedown".to_owned()),
                    target_id: Some(id),
                    reason: "counter-notice wait over".to_owned(),
                    ip: None,
                    detail: json!({ "by": "housekeeping" }).to_string(),
                },
            )
            .await?;
            let items = items_of(&mut tx, id).await?;
            tx.commit().await?;
            let reference = notice_ref(id);
            self.notify_subjects(&items, |labels| restore_dm(labels, &reference))
                .await;
            restored.push(id);
        }
        if !restored.is_empty() {
            self.reload_blocklist().await;
            tracing::info!(
                count = restored.len(),
                "restored content after counter-notices"
            );
        }
        Ok(restored)
    }

    // ---- export and item pages ----------------------------------------------------------------------------------

    /// Every notice with its items, for the register export (spec 5, step 7). The read is access-logged.
    pub async fn export_register(&self, who: &Who<'_>) -> ModResult<Vec<NoticeDetail>> {
        Self::tk_staff(who)?;
        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        let ids: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM takedown_notices ORDER BY received_at, id")
                .fetch_all(&mut *tx)
                .await?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            out.push(self.detail_in(&mut tx, id, now).await?);
        }
        log_staff(
            &mut tx,
            who,
            AccessKind::Read,
            "moderation.takedown.export",
            ("takedown", 0),
            "",
            json!({ "notices": out.len() }),
        )
        .await?;
        tx.commit().await?;
        Ok(out)
    }

    /// The notice that holds or took down this message, if one has not ended (D-337): a case restore
    /// refuses such a message, because only the register puts it back (its restore or withdrawal).
    pub async fn message_takedown(&self, message_id: i64) -> ModResult<Option<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT n.id FROM takedown_items i JOIN takedown_notices n ON n.id = i.notice_id              WHERE i.kind = 'message' AND i.target_id = $1 AND i.state IN ('held', 'disabled')                AND n.state IN ('triage', 'removed', 'counter_noticed')              ORDER BY n.id LIMIT 1",
        )
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// The refusal a case restore gives for a message a takedown holds (D-337, the N-32 shape).
    pub(crate) fn takedown_restore_refusal(notice: i64) -> String {
        format!(
            "The message was removed by takedown notice {}. Restore or withdraw the notice in the register.",
            notice_ref(notice)
        )
    }

    /// Whether a library or cinema item is held or taken down, for the reader and player pages
    /// (`MOD-TKD-011`). Any signed-in person may ask; the answer is only the state. A chapter answers for its
    /// series and an episode for its title.
    pub async fn item_status(&self, kind: ItemKind, id: i64) -> ModResult<ItemStatus> {
        let parent: Option<(&str, i64)> = match kind {
            ItemKind::LibraryChapter => {
                sqlx::query_scalar::<_, i64>("SELECT series_id FROM reading_chapters WHERE id = $1")
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await?
                    .map(|s| ("library_series", s))
            }
            ItemKind::CinemaEpisode => {
                sqlx::query_scalar::<_, i64>("SELECT title_id FROM video_episodes WHERE id = $1")
                    .bind(id)
                    .fetch_optional(&self.pool)
                    .await?
                    .map(|t| ("cinema_title", t))
            }
            _ => None,
        };
        let (pk, pid) = parent.unwrap_or((kind.as_str(), id));
        let states: Vec<String> = sqlx::query_scalar(
            "SELECT i.state FROM takedown_items i JOIN takedown_notices n ON n.id = i.notice_id \
             WHERE n.state IN ('triage', 'removed', 'counter_noticed') AND i.state IN ('held', 'disabled') \
               AND ((i.kind = $1 AND i.target_id = $2) OR (i.kind = $3 AND i.target_id = $4))",
        )
        .bind(kind.as_str())
        .bind(id)
        .bind(pk)
        .bind(pid)
        .fetch_all(&self.pool)
        .await?;
        let state = if states.iter().any(|s| s == "disabled") {
            "taken_down"
        } else if states.is_empty() {
            "none"
        } else {
            "held"
        };
        Ok(ItemStatus {
            state: state.to_owned(),
        })
    }
}

/// The register as CSV (header line first), one row per notice.
pub fn register_csv(notices: &[NoticeDetail]) -> String {
    use crate::takedown_rules::{csv_line, iso_utc};
    let opt = |ms: Option<i64>| ms.map(iso_utc).unwrap_or_default();
    let mut out = csv_line(
        &[
            "reference",
            "kind",
            "state",
            "notifier_name",
            "notifier_contact",
            "legal_basis",
            "locations",
            "received_at",
            "items",
            "removed_at",
            "counter_noticed_at",
            "restore_after",
            "court_filing_noted",
            "restored_at",
            "rejection_reason",
        ]
        .map(String::from),
    );
    for n in notices {
        out.push_str(&csv_line(&[
            notice_ref(n.summary.id),
            n.summary.kind.as_str().to_owned(),
            n.summary.state.as_str().to_owned(),
            n.summary.notifier_name.clone(),
            n.notifier_contact.clone(),
            n.legal_basis.clone(),
            n.locations.join(" | "),
            iso_utc(n.summary.received_at_ms),
            n.items
                .iter()
                .map(|i| i.label.as_str())
                .collect::<Vec<_>>()
                .join(" | "),
            opt(n.summary.removed_at_ms),
            opt(n.counter_noticed_at_ms),
            opt(n.summary.restore_after_ms),
            n.summary.court_filing_noted.to_string(),
            opt(n.restored_at_ms),
            n.rejection_reason.clone(),
        ]));
    }
    out
}
