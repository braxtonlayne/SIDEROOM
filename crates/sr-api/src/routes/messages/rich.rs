//! Collaborators of the rich message features (polls, stickers, link previews),
//! registered with [`super::MessagesDeps::with_rich`]. Every piece is optional: a
//! deployment without a [`RichStore`] rejects polls and stickers with a clear
//! error, one without a scheduler or unfurler simply never unfurls links.

use super::deps::MessagesDeps;
use super::error::{MsgResult, invalid};
use async_trait::async_trait;
use serde_json::{Value, json};
use sr_chat::rich_repo::RichStore;
use sr_chat::{Message, message_flags};
use sr_core::GuildId;
use sr_unfurl::Unfurler;
use std::sync::Arc;
use std::time::Duration;

/// Job kind: fetch link previews for one message.
pub const JOB_UNFURL: &str = "messages.unfurl";
/// Job kind: finalise one poll when it expires.
pub const JOB_POLL_FINALIZE: &str = "messages.poll_finalize";
/// Job kind of the recurring sweep that finalises polls whose job was lost.
pub const JOB_POLL_SWEEP: &str = "messages.poll_sweep";
/// Reserved singleton id of the poll sweep (below any Snowflake).
pub const POLL_SWEEP_JOB_ID: i64 = 120;
/// Platform feature id that toggles link previews (default on).
pub const UNFURL_FEATURE_ID: &str = "unfurl";
/// Links previewed per message.
pub const MAX_PREVIEW_LINKS: usize = 5;

/// Enqueues background jobs (implemented over the durable queue by `sr-server`).
#[async_trait]
pub trait JobScheduler: Send + Sync {
    async fn enqueue(&self, kind: &str, payload: Value, delay: Duration) -> Result<(), String>;
}

/// Whether link previews are switched on for a guild (the `unfurl` feature).
#[async_trait]
pub trait UnfurlGate: Send + Sync {
    async fn enabled(&self, guild: GuildId) -> bool;
}

#[derive(Clone, Default)]
pub struct RichDeps {
    pub store: Option<Arc<dyn RichStore>>,
    pub scheduler: Option<Arc<dyn JobScheduler>>,
    pub unfurler: Option<Arc<Unfurler>>,
    pub gate: Option<Arc<dyn UnfurlGate>>,
}

impl MessagesDeps {
    /// Install the rich-message collaborators.
    pub fn with_rich(mut self, rich: RichDeps) -> Self {
        self.rich = rich;
        self
    }

    /// The rich store, or a 50035 telling the caller the feature is unavailable.
    pub fn rich_store(&self, field: &str) -> MsgResult<&Arc<dyn RichStore>> {
        self.rich.store.as_ref().ok_or_else(|| {
            invalid(
                field,
                "NOT_SUPPORTED",
                "This feature is not available on this server.",
            )
        })
    }

    async fn enqueue(&self, kind: &str, payload: Value, delay: Duration) {
        if let Some(s) = &self.rich.scheduler
            && let Err(e) = s.enqueue(kind, payload, delay).await
        {
            tracing::warn!(error = %e, kind, "enqueueing a rich-message job failed");
        }
    }

    /// Queue a link-preview pass for `msg` after it was created or edited. Does
    /// nothing in DMs (previews would make this server fetch for the recipient's
    /// conversation; there is no per-user opt-in yet) or without an unfurler.
    pub async fn schedule_unfurl(&self, msg: &Message) {
        if self.rich.unfurler.is_none() || msg.guild_id.is_none() {
            return;
        }
        let has_links = !sr_markdown::extract_preview_links(&sr_markdown::parse(&msg.content))
            .is_empty();
        let had_previews = msg.embeds.iter().any(sr_chat::message::Embed::is_unfurled);
        if !has_links && !had_previews {
            return;
        }
        self.enqueue(
            JOB_UNFURL,
            json!({"channel_id": msg.channel_id.get(), "message_id": msg.id.get()}),
            Duration::ZERO,
        )
        .await;
    }

    /// Queue the finalisation of `msg`'s poll for when it expires.
    pub async fn schedule_poll_finalize(&self, msg: &Message) {
        let Some(poll) = &msg.poll else { return };
        let Some(expiry) = poll.expiry_ms else { return };
        let delay = u64::try_from((expiry - self.now_ms()).max(0)).unwrap_or(0);
        self.enqueue(
            JOB_POLL_FINALIZE,
            json!({"channel_id": msg.channel_id.get(), "message_id": msg.id.get()}),
            Duration::from_millis(delay),
        )
        .await;
    }

    /// Whether message flags ask for link previews to be hidden.
    pub fn embeds_suppressed(flags: u32) -> bool {
        flags & message_flags::SUPPRESS_EMBEDS != 0
    }
}
