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

    /// Server-set flag bits for the message before it is stored (PAR-FU-002: a DM the recipient's spam filter
    /// flags). Also asked again for the edited text when a content edit changes the message, which sets or clears
    /// the bit. Defaults to none.
    async fn extra_flags(&self, _message: &Message) -> u32 {
        0
    }
}

#[async_trait]
impl<R: SocialRepository + 'static> MessageHooks for SocialService<R> {
    async fn extra_flags(&self, m: &Message) -> u32 {
        if m.guild_id.is_some() {
            return 0;
        }
        match self
            .friend_dm_spam_flag(m.channel_id, m.author_id, &m.content)
            .await
        {
            Ok(true) => crate::domain::message_flags::LIKELY_SPAM,
            Ok(false) => 0,
            Err(e) => {
                tracing::warn!(error = %e, channel = %m.channel_id, "friend DM spam flagging failed");
                0
            }
        }
    }

    async fn message_sent(&self, m: &Message, notify: &[UserId]) {
        if m.guild_id.is_none()
            && let Err(e) = self.reopen_for_message(m.channel_id).await
        {
            tracing::warn!(error = %e, channel = %m.channel_id, "reopening DM after send failed");
        }
        // RDY-PAR-004: the recipient's DM spam filter reads a stranger's message.
        if m.guild_id.is_none()
            && let Err(e) = self
                .screen_dm_message(m.channel_id, m.author_id, &m.content)
                .await
        {
            tracing::warn!(error = %e, channel = %m.channel_id, "DM spam screening failed");
        }
        // Recipients were filtered for VIEW_CHANNEL before the message was stored. A block either
        // way, or an ignore of the author, keeps the badge off (SEC-AUD-011); fail closed.
        let muted = match self.mention_muted(m.author_id, notify).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, channel = %m.channel_id, "mention block check failed");
                return;
            }
        };
        // D-726, D-733: a recipient reached only by `@everyone` or a role whose scope suppresses that kind gets no
        // badge; `@everyone` reaches all of them, a role mention only the holders of one of the roles.
        // One lookup, and only for a message that has such a mention; a failure fails closed like the check above.
        let mut muted = muted;
        if let Some(guild) = m.guild_id
            && (m.mention_everyone || !m.mention_role_ids.is_empty())
        {
            let indirect: Vec<UserId> = notify
                .iter()
                .copied()
                .filter(|u| !m.mention_user_ids.contains(u))
                .collect();
            match self
                .mention_suppressed(guild, &indirect, m.mention_everyone, &m.mention_role_ids)
                .await
            {
                Ok(v) => muted.extend(v),
                Err(e) => {
                    tracing::warn!(error = %e, channel = %m.channel_id, "mention suppress check failed");
                    return;
                }
            }
        }
        if let Err(e) = self
            .record_eligible_mentions(m.author_id, notify, m.channel_id, |u| !muted.contains(&u))
            .await
        {
            tracing::warn!(error = %e, channel = %m.channel_id, "recording mentions failed");
        }
    }
}
