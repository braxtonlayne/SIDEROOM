//! Work the message path triggers after a message is stored, owned by the social
//! domain: reopening closed DMs (open question 114) and mention badges
//! (open question 117). `ChatService` calls the hook; failures are logged, never
//! returned, because the message is already persisted and published.

use crate::domain::Message;
use crate::social::repo::SocialRepository;
use crate::social::service::SocialService;
use async_trait::async_trait;
use sr_core::UserId;

#[async_trait]
pub trait MessageHooks: Send + Sync {
    /// `notify` is every user the message pings: explicit mentions plus the
    /// `@everyone`/`@here` and role fan-out, all already VIEW_CHANNEL-checked.
    async fn message_sent(&self, message: &Message, notify: &[UserId]);
}

#[async_trait]
impl<R: SocialRepository + 'static> MessageHooks for SocialService<R> {
    async fn message_sent(&self, m: &Message, notify: &[UserId]) {
        if m.guild_id.is_none()
            && let Err(e) = self.reopen_for_message(m.channel_id).await
        {
            tracing::warn!(error = %e, channel = %m.channel_id, "reopening DM after send failed");
        }
        // Recipients were filtered for VIEW_CHANNEL before the message was stored.
        if let Err(e) = self
            .record_eligible_mentions(m.author_id, notify, m.channel_id, |_| true)
            .await
        {
            tracing::warn!(error = %e, channel = %m.channel_id, "recording mentions failed");
        }
    }
}
