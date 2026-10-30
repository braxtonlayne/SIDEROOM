//! UTL-CRD-001: the `cards.remind` singleton (job id 1670). Every 30 seconds it
//!
//! 1. marks cards whose start has passed as started and rewrites their message (a cinema card with a title gains
//!    its "Start watch party" button; `START_WATCH_PARTY` is checked by the watch party API when it is used),
//! 2. closes cards six hours after their start (buttons gone), and
//! 3. sends the reminders: a DM from the system user to each claimant who asked for one, about an hour and about
//!    ten minutes before the start. A DM is a notification and a push like any other (LNG-NTF-003); there is no
//!    separate delivery path.
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

/// Register the handler. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, jobs_state: Arc<CardJobs>) -> JobRunner {
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
    Ok(())
}
