//! UTL-CRD-001: the `cards.remind` singleton (job id 1670). Every 30 seconds it
//!
//! 1. marks cards whose start has passed as started and rewrites their message (a cinema card with a title gains
//!    its "Start watch party" button; `START_WATCH_PARTY` is checked by the watch party API when it is used),
//! 2. closes cards six hours after their start (buttons gone), and
//! 3. sends the reminders: a DM from the system user to each claimant who asked for one, about an hour and about
//!    ten minutes before the start. A DM is a notification and a push like any other (LNG-NTF-003); there is no
//!    separate delivery path.
//!
//! A second singleton, `cards.purge_claimants` (job id 1671, FU-8), deletes the claimant list of a card
//! `invites.card_claimants_retention_days` (default 30, 0 = never) after the card finished or was canceled: who said they
//! would come is personal data with no use once the event is over. The card stays and so does how many went
//! (`invite_cards.going_final`, written in the same statement that deletes the rows).
//!
//! A reminder is sent only if the member's DM settings allow it ([`sr_chat::cards::reminders::dm_decision`]) and
//! the member can still see the channel. The mark is taken before the send (so two nodes never both send) and
//! given back when the delivery fails for a reason worth retrying.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::json;
use sr_api::routes::messages::MessagesDeps;
use sr_api::routes::tools::cards::{CardDeps, refresh_card};
use sr_chat::cards::ics::card_path;
use sr_chat::cards::reminders::{Verdict, Window, due_window, reminder_text};
use sr_core::{ChannelId, UserId};
use sr_db::jobs;
use sr_db::runner::JobRunner;
use sr_db::{DbResult, PgPool};
use sr_userfeat::{Card as Dm, Poster};

/// Reserved singleton id of the job (unique among job ids; 1660 is `tool_rooms.sweep`).
pub const JOB_ID: i64 = 1670;
/// Kind of the job.
pub const KIND: &str = "cards.remind";
/// Reserved singleton id of the claimant purge (unique among job ids).
pub const PURGE_JOB_ID: i64 = 1671;
/// Kind of the claimant purge.
pub const PURGE_KIND: &str = "cards.purge_claimants";
/// `invites.card_claimants_retention_days` when staff set nothing.
pub const DEFAULT_RETENTION_DAYS: i64 = 30;
const PURGE_INTERVAL: Duration = Duration::from_secs(6 * 3600);
const PURGE_BATCH: i64 = 500;
const INTERVAL: Duration = Duration::from_secs(30);
const BATCH: i64 = 100;
const CANDIDATES: i64 = 500;
const DM_COLOR: u32 = 0x00C9_A24D;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// What the job needs.
pub struct CardJobs {
    pub deps: Arc<CardDeps>,
    pub messages: Arc<MessagesDeps>,
    /// The system user's DM (the same path Live TV reminders use).
    pub poster: Arc<dyn Poster>,
}

/// What one pass did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    pub started: usize,
    pub closed: usize,
    pub sent: usize,
    /// Not sent because of the member's DM settings, a lost channel view or a block.
    pub skipped: usize,
    /// Delivery failed; the mark was given back for the next pass.
    pub failed: usize,
}

impl CardJobs {
    /// One pass at `now_ms`.
    pub async fn run_once(&self, now: i64) -> Report {
        let mut report = Report::default();
        let store = &self.deps.store;

        match store.take_started(now, BATCH).await {
            Ok(cards) => {
                report.started = cards.len();
                for c in &cards {
                    refresh_card(&self.messages, &self.deps, c).await;
                }
            }
            Err(e) => tracing::warn!(error = %e.0, "marking started invite cards failed"),
        }
        match store.take_closed(now, BATCH).await {
            Ok(cards) => {
                report.closed = cards.len();
                for c in &cards {
                    refresh_card(&self.messages, &self.deps, c).await;
                }
            }
            Err(e) => tracing::warn!(error = %e.0, "closing invite cards failed"),
        }

        let due = match store.reminder_candidates(now, CANDIDATES).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e.0, "reading invite card reminders failed");
                return report;
            }
        };
        for (card, claim) in due {
            let window = due_window(
                now,
                card.starts_ms,
                claim.claimed_ms,
                claim.reminded_hour,
                claim.reminded_ten,
            );
            let Some(window) = window else {
                // A claim made after the last window can never owe a reminder: settle it so it stops
                // being a candidate (nothing is sent).
                if claim.claimed_ms > card.starts_ms - Window::TenMinutes.lead_ms()
                    && let Err(e) = store
                        .mark_reminded(card.id, claim.user_id, Window::TenMinutes, now)
                        .await
                {
                    tracing::warn!(error = %e.0, "settling an invite card claim failed");
                }
                continue;
            };
            match store
                .mark_reminded(card.id, claim.user_id, window, now)
                .await
            {
                Ok(true) => {}
                // Another node took this window.
                Ok(false) => continue,
                Err(e) => {
                    tracing::warn!(error = %e.0, "marking an invite card reminder failed");
                    continue;
                }
            }
            match self.deliver(&card, claim.user_id, window).await {
                Delivery::Sent => report.sent += 1,
                Delivery::Skipped => report.skipped += 1,
                Delivery::Retry => {
                    report.failed += 1;
                    if let Err(e) = store.unmark_reminded(card.id, claim.user_id, window).await {
                        tracing::warn!(error = %e.0, "returning an invite card reminder mark failed");
                    }
                }
            }
        }
        report
    }

    async fn deliver(&self, card: &sr_chat::cards::Card, user: u64, window: Window) -> Delivery {
        // The member must still be able to see the channel the card is in.
        let member = UserId::new(user);
        match self
            .messages
            .access(member, ChannelId::new(card.channel_id))
            .await
        {
            Ok(acc) if acc.require_view().is_ok() => {}
            Ok(_) => return Delivery::Skipped,
            Err(_) => return Delivery::Skipped,
        }
        match self.deps.gate.check(user, card.guild_id).await {
            Verdict::Send => {}
            Verdict::Skip(why) => {
                tracing::debug!(
                    ?why,
                    card = card.id,
                    "invite card reminder skipped by DM settings"
                );
                return Delivery::Skipped;
            }
            Verdict::Unknown(e) => {
                tracing::warn!(error = %e, "reading DM settings for a reminder failed");
                return Delivery::Retry;
            }
        }
        let (title, description) = reminder_text(card, window);
        let dm = Dm {
            title: Some(title),
            description: Some(description),
            url: Some(card_path(card)),
            color: DM_COLOR,
            ..Dm::default()
        };
        let Ok(user) = i64::try_from(user) else {
            return Delivery::Skipped;
        };
        match self.poster.dm(user, dm).await {
            Ok(_) => Delivery::Sent,
            Err(e) => {
                tracing::warn!(error = %e, "sending an invite card reminder failed");
                Delivery::Retry
            }
        }
    }
}

enum Delivery {
    Sent,
    Skipped,
    Retry,
}

/// The retention in days: `invites.card_claimants_retention_days`, 0 meaning never. A stored value that is not a
/// number in range falls back to the default.
async fn retention_days(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let v: Option<i64> = sqlx::query_scalar(
        "SELECT floor((value #>> '{}')::numeric)::bigint FROM platform_settings          WHERE key = 'invites.card_claimants_retention_days' AND jsonb_typeof(value) = 'number'",
    )
    .fetch_optional(pool)
    .await?;
    Ok(v.filter(|d| (0..=3_650).contains(d))
        .unwrap_or(DEFAULT_RETENTION_DAYS))
}

/// One purge pass: for cards that finished or were canceled more than the retention ago, remember how many held a
/// seat and delete the claim rows, in one statement. Returns the number of claim rows deleted. With a retention of 0
/// nothing happens. A pass handles at most [`PURGE_BATCH`] cards; the next one takes the rest.
pub async fn purge_claimants(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let days = retention_days(pool).await?;
    if days == 0 {
        return Ok(0);
    }
    let done = sqlx::query(
        "WITH due AS (             SELECT c.id, (SELECT count(*) FROM invite_card_claims k WHERE k.card_id = c.id) AS n             FROM invite_cards c             WHERE c.claimants_purged_at IS NULL               AND LEAST(c.closed_at, c.canceled_at) < now() - make_interval(days => $1::int)             ORDER BY c.id LIMIT $2 FOR UPDATE OF c          ), upd AS (             UPDATE invite_cards c SET claimants_purged_at = now(), going_final = due.n::int             FROM due WHERE c.id = due.id RETURNING c.id          )          DELETE FROM invite_card_claims k USING upd WHERE k.card_id = upd.id",
    )
    .bind(i32::try_from(days).unwrap_or(30))
    .bind(PURGE_BATCH)
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// Register the handler. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, jobs_state: Arc<CardJobs>) -> JobRunner {
    let purge_pool = pool.clone();
    let runner = runner.register(PURGE_KIND, 1, move |job| {
        let pool = purge_pool.clone();
        Box::pin(async move {
            match purge_claimants(&pool).await {
                Ok(0) => {}
                Ok(n) => tracing::info!(claims = n, "invite card claimant lists purged"),
                // The next pass retries; a failure must not stop the singleton.
                Err(e) => tracing::warn!(error = %e, "cards.purge_claimants failed"),
            }
            jobs::reschedule_running(&pool, job.id, PURGE_INTERVAL).await?;
            Ok(())
        })
    });
    let pool = pool.clone();
    runner.register(KIND, 1, move |job| {
        let state = jobs_state.clone();
        let pool = pool.clone();
        Box::pin(async move {
            let report = state.run_once(now_ms()).await;
            if report != Report::default() {
                tracing::info!(?report, "invite cards pass");
            }
            jobs::reschedule_running(&pool, job.id, INTERVAL).await?;
            Ok(())
        })
    })
}

/// Create or re-arm the singleton. Safe on every boot.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(pool, JOB_ID, KIND, &json!({}), Duration::from_secs(20)).await?;
    jobs::schedule_singleton(
        pool,
        PURGE_JOB_ID,
        PURGE_KIND,
        &json!({}),
        Duration::from_secs(90),
    )
    .await?;
    Ok(())
}
