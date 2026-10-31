//! In-memory [`RichStore`] for tests.

use super::messages::MemoryBackend;
use crate::message::Embed;
use crate::poll_rules;
use crate::repo::StoreResult;
use crate::rich_repo::{
    PollTally, RetractResult, RichStore, StickerPack, StickerRecord, VoteResult,
};
use async_trait::async_trait;
use sr_core::{ChannelId, MessageId, UserId};
use std::collections::HashMap;

#[async_trait]
impl RichStore for MemoryBackend {
    async fn set_webhook_author(
        &self,
        message: MessageId,
        author: &crate::rich_repo::WebhookAuthor,
    ) -> StoreResult<()> {
        self.data().webhook_authors.insert(message, author.clone());
        Ok(())
    }

    async fn webhook_authors(
        &self,
        messages: &[MessageId],
    ) -> StoreResult<HashMap<MessageId, crate::rich_repo::WebhookAuthor>> {
        let d = self.data();
        Ok(messages
            .iter()
            .filter_map(|m| d.webhook_authors.get(m).map(|a| (*m, a.clone())))
            .collect())
    }

    async fn bridged_members(
        &self,
        _guild: sr_core::GuildId,
    ) -> StoreResult<Vec<crate::rich_repo::BridgedMember>> {
        Ok(self.data().bridged_members.clone())
    }

    async fn set_bridge_author(
        &self,
        message: MessageId,
        author: &crate::rich_repo::BridgeAuthor,
    ) -> StoreResult<()> {
        self.data().bridge_authors.insert(message, author.clone());
        Ok(())
    }

    async fn bridge_authors(
        &self,
        messages: &[MessageId],
    ) -> StoreResult<HashMap<MessageId, crate::rich_repo::BridgeAuthor>> {
        let d = self.data();
        Ok(messages
            .iter()
            .filter_map(|m| d.bridge_authors.get(m).map(|a| (*m, a.clone())))
            .collect())
    }

    async fn app_emoji_owners(&self, emoji: &[u64]) -> StoreResult<HashMap<u64, i64>> {
        let d = self.data();
        Ok(emoji
            .iter()
            .filter_map(|e| d.app_emojis.get(e).map(|a| (*e, *a)))
            .collect())
    }

    async fn guild_emojis(
        &self,
        emoji: &[u64],
    ) -> StoreResult<HashMap<u64, crate::rich_repo::GuildEmojiRef>> {
        let d = self.data();
        Ok(emoji
            .iter()
            .filter_map(|e| d.guild_emojis.get(e).map(|g| (*e, g.clone())))
            .collect())
    }

    async fn bot_applications(&self, users: &[UserId]) -> StoreResult<HashMap<UserId, i64>> {
        let d = self.data();
        Ok(users
            .iter()
            .filter_map(|u| d.bot_apps.get(u).map(|a| (*u, *a)))
            .collect())
    }

    async fn set_embeds(&self, message: MessageId, embeds: &[Embed]) -> StoreResult<()> {
        if let Some(m) = self.data().messages.iter_mut().find(|m| m.id == message) {
            m.embeds = embeds.to_vec();
        }
        Ok(())
    }

    async fn poll_tallies(
        &self,
        messages: &[MessageId],
        viewer: Option<UserId>,
    ) -> StoreResult<HashMap<MessageId, PollTally>> {
        let d = self.data();
        let mut out: HashMap<MessageId, PollTally> = HashMap::new();
        for (m, a, u) in &d.poll_votes {
            if messages.contains(m) {
                let t = out.entry(*m).or_default();
                *t.counts.entry(*a).or_default() += 1;
                if viewer == Some(*u) {
                    t.mine.insert(*a);
                }
            }
        }
        for m in messages {
            if let Some(poll) = d
                .messages
                .iter()
                .find(|x| x.id == *m)
                .and_then(|x| x.poll.as_ref())
            {
                for (a, n) in &poll.anonymous_counts {
                    *out.entry(*m).or_default().counts.entry(*a).or_default() += *n;
                }
            }
        }
        Ok(out)
    }

    async fn cast_vote(
        &self,
        message: MessageId,
        answer: u32,
        user: UserId,
        now_ms: i64,
    ) -> StoreResult<VoteResult> {
        let mut d = self.data();
        let Some(poll) = d
            .messages
            .iter()
            .find(|m| m.id == message)
            .and_then(|m| m.poll.clone())
        else {
            return Ok(VoteResult::NoPoll);
        };
        if !poll_rules::is_open(&poll, now_ms) {
            return Ok(VoteResult::Closed);
        }
        if !poll_rules::has_answer(&poll, answer) {
            return Ok(VoteResult::UnknownAnswer);
        }
        if d.poll_votes.contains(&(message, answer, user)) {
            return Ok(VoteResult::AlreadyVoted);
        }
        let mut replaced = Vec::new();
        if !poll.allow_multiselect {
            replaced = d
                .poll_votes
                .iter()
                .filter(|(m, _, u)| *m == message && *u == user)
                .map(|(_, a, _)| *a)
                .collect();
            for a in &replaced {
                d.poll_votes.remove(&(message, *a, user));
            }
        }
        d.poll_votes.insert((message, answer, user));
        Ok(VoteResult::Added { replaced })
    }

    async fn retract_vote(
        &self,
        message: MessageId,
        answer: u32,
        user: UserId,
        now_ms: i64,
    ) -> StoreResult<RetractResult> {
        let mut d = self.data();
        let Some(poll) = d
            .messages
            .iter()
            .find(|m| m.id == message)
            .and_then(|m| m.poll.clone())
        else {
            return Ok(RetractResult::NoPoll);
        };
        if !poll_rules::is_open(&poll, now_ms) {
            return Ok(RetractResult::Closed);
        }
        Ok(if d.poll_votes.remove(&(message, answer, user)) {
            RetractResult::Removed
        } else {
            RetractResult::NotVoted
        })
    }

    async fn poll_voters(
        &self,
        message: MessageId,
        answer: u32,
        after: Option<UserId>,
        limit: usize,
    ) -> StoreResult<Vec<UserId>> {
        Ok(self
            .data()
            .poll_votes
            .iter()
            .filter(|(m, a, u)| *m == message && *a == answer && after.is_none_or(|x| *u > x))
            .map(|(_, _, u)| *u)
            .take(limit)
            .collect())
    }

    async fn expire_poll_now(&self, message: MessageId, now_ms: i64) -> StoreResult<bool> {
        let mut d = self.data();
        let Some(poll) = d
            .messages
            .iter_mut()
            .find(|m| m.id == message)
            .and_then(|m| m.poll.as_mut())
        else {
            return Ok(false);
        };
        if poll.finalized || poll.expiry_ms.is_some_and(|e| e <= now_ms) {
            return Ok(false);
        }
        poll.expiry_ms = Some(now_ms);
        Ok(true)
    }

    async fn finalize_poll(&self, message: MessageId, now_ms: i64) -> StoreResult<bool> {
        let mut d = self.data();
        let Some(poll) = d
            .messages
            .iter_mut()
            .find(|m| m.id == message)
            .and_then(|m| m.poll.as_mut())
        else {
            return Ok(false);
        };
        if !poll_rules::needs_finalizing(poll, now_ms) {
            return Ok(false);
        }
        poll.finalized = true;
        Ok(true)
    }

    async fn due_polls(
        &self,
        now_ms: i64,
        limit: usize,
    ) -> StoreResult<Vec<(ChannelId, MessageId)>> {
        Ok(self
            .data()
            .messages
            .iter()
            .filter(|m| {
                m.is_visible()
                    && m.poll
                        .as_ref()
                        .is_some_and(|p| poll_rules::needs_finalizing(p, now_ms))
            })
            .map(|m| (m.channel_id, m.id))
            .take(limit)
            .collect())
    }

    async fn stickers(&self, ids: &[u64]) -> StoreResult<Vec<StickerRecord>> {
        let d = self.data();
        Ok(ids
            .iter()
            .filter_map(|id| d.sticker_rows.get(id).cloned())
            .collect())
    }

    async fn sticker_packs(&self) -> StoreResult<Vec<StickerPack>> {
        Ok(self.data().sticker_packs.clone())
    }

    async fn pack_sticker_media(&self, id: u64) -> StoreResult<Option<String>> {
        Ok(self
            .data()
            .sticker_packs
            .iter()
            .flat_map(|p| p.stickers.iter())
            .find(|s| s.id == id)
            .map(|s| s.media_key.clone()))
    }
}
