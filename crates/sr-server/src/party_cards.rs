//! The live chat card of a watch party (WP-BE-008, WP-FE-010).
//!
//! When a party is bound to a channel, the system user posts one message of type 200 there: an embed with
//! the party's title, the episode and the headcount, linking to `/cinema/party/<id>`. The same message is
//! rewritten (`MESSAGE_UPDATE`) when the media, the host or the headcount changes and once more when the
//! party ends. Other clients see an ordinary embed; the web client draws a live card from the party state
//! it already receives and uses the embed as the snapshot after the end.
//!
//! * Which message belongs to which party is kept in `watch_party_cards` (migration 1001), so a restart or
//!   another node still finds it.
//! * Edits are spaced at least [`MIN_EDIT_GAP`] apart per party; a change that arrives inside the gap is
//!   queued and the newest one is written when the gap is over, so a crowd joining makes a few edits, not
//!   one per person. The end is never delayed.
//! * An age-restricted title is not named on the card.
//! * Failures never fail the party operation; they are logged.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use sqlx::PgPool;
use sr_api::routes::donations::ChatSlot;
use sr_api::routes::messages::MessagesDeps;
use sr_api::routes::messages::system::{post_system_card_of, rewrite_system_card};
use sr_chat::MessageType;
use sr_chat::message::{Embed, EmbedAuthor};
use sr_core::{ChannelId, MessageId};
use sr_watchparty::{Announcement, AnnouncementKind, ChatHook};

/// Card colour while the party runs (the theme gold) and after it ended.
const LIVE_COLOR: u32 = 0x00C9_A24D;
const ENDED_COLOR: u32 = 0x0074_7F8D;
/// Least time between two edits of one card.
pub const MIN_EDIT_GAP: Duration = Duration::from_secs(15);
/// Title shown instead of an age-restricted one.
const HIDDEN_TITLE: &str = "Age-restricted title";
/// Title of a party that has no media yet (an empty watch party room).
const EMPTY_TITLE: &str = "Watch party";

/// The embed of a card: what other clients show, and the snapshot the web client falls back to.
pub fn card_embed(a: &Announcement) -> Embed {
    let ended = a.kind == AnnouncementKind::Ended;
    let title = if a.age_restricted {
        HIDDEN_TITLE.to_owned()
    } else if a.title.is_empty() {
        EMPTY_TITLE.to_owned()
    } else {
        a.title.clone()
    };
    let who = match (a.viewers, ended) {
        (1, false) => "1 watching".to_owned(),
        (n, false) => format!("{n} watching"),
        (1, true) => "Ended after 1 viewer".to_owned(),
        (n, true) => format!("Ended after {n} viewers"),
    };
    let description = match (&a.episode_label, a.age_restricted) {
        (Some(ep), false) if !ep.is_empty() => format!("{ep} · {who}"),
        _ => who,
    };
    Embed {
        title: Some(title),
        kind: Some("rich".to_owned()),
        description: Some(description),
        url: Some(format!("/cinema/party/{}", a.party_id)),
        color: if ended { ENDED_COLOR } else { LIVE_COLOR },
        author: Some(EmbedAuthor {
            name: "Watch party".to_owned(),
            url: None,
            icon_url: None,
        }),
        ..Embed::default()
    }
}

#[derive(Default)]
struct Slot {
    last_edit: Option<Instant>,
    /// The newest change that arrived inside the gap; written when the gap is over.
    queued: Option<Announcement>,
    /// A task is waiting to write `queued`.
    waiting: bool,
}

/// Posts and rewrites the card.
pub struct PartyCards {
    pool: PgPool,
    chat: ChatSlot,
    slots: Arc<Mutex<HashMap<i64, Slot>>>,
    gap: Duration,
}

impl PartyCards {
    pub fn new(pool: PgPool, chat: ChatSlot) -> Self {
        Self {
            pool,
            chat,
            slots: Arc::new(Mutex::new(HashMap::new())),
            gap: MIN_EDIT_GAP,
        }
    }

    /// A different gap between edits (tests).
    #[must_use]
    pub fn with_gap(mut self, gap: Duration) -> Self {
        self.gap = gap;
        self
    }

    fn deps(&self) -> Option<&Arc<MessagesDeps>> {
        self.chat.get()
    }

    async fn message_of(&self, party: i64) -> Option<(i64, i64)> {
        sqlx::query_as("SELECT channel_id, message_id FROM watch_party_cards WHERE party_id = $1")
            .bind(party)
            .fetch_optional(&self.pool)
            .await
            .inspect_err(|e| tracing::warn!(error = %e, "reading a watch party card failed"))
            .ok()
            .flatten()
    }

    async fn post(&self, a: &Announcement) {
        let Some(deps) = self.deps() else { return };
        let (Ok(channel), id) = (
            u64::try_from(a.channel_id).map(ChannelId::new),
            MessageId::from(deps.next_id()),
        ) else {
            return;
        };
        let stored = sqlx::query(
            "INSERT INTO watch_party_cards (party_id, channel_id, message_id) VALUES ($1, $2, $3) \
             ON CONFLICT (party_id) DO NOTHING",
        )
        .bind(a.party_id)
        .bind(a.channel_id)
        .bind(i64::try_from(id.get()).unwrap_or_default())
        .execute(&self.pool)
        .await;
        match stored {
            // Another node posted this party's card first.
            Ok(done) if done.rows_affected() == 0 => return,
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, "storing a watch party card failed");
                return;
            }
        }
        if let Err(e) = post_system_card_of(
            deps,
            channel,
            id,
            MessageType::WatchParty,
            vec![card_embed(a)],
            Vec::new(),
        )
        .await
        {
            // The channel is gone or the system user cannot post there: forget the row so a later
            // update does not chase a message that does not exist.
            tracing::warn!(error = ?e, party = a.party_id, "posting a watch party card failed");
            let _ = sqlx::query("DELETE FROM watch_party_cards WHERE party_id = $1")
                .bind(a.party_id)
                .execute(&self.pool)
                .await;
        }
        self.slots
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(
                a.party_id,
                Slot {
                    last_edit: Some(Instant::now()),
                    ..Slot::default()
                },
            );
    }

    /// Rewrite the card now.
    async fn rewrite(&self, a: &Announcement) {
        let Some(deps) = self.deps() else { return };
        let Some((channel, message)) = self.message_of(a.party_id).await else {
            return;
        };
        let (Ok(channel), Ok(message)) = (u64::try_from(channel), u64::try_from(message)) else {
            return;
        };
        match rewrite_system_card(
            deps,
            ChannelId::new(channel),
            MessageId::new(message),
            &[card_embed(a)],
            &[],
        )
        .await
        {
            Ok(true) => {}
            Ok(false) => {
                // The message was deleted: stop following it.
                let _ = sqlx::query("DELETE FROM watch_party_cards WHERE party_id = $1")
                    .bind(a.party_id)
                    .execute(&self.pool)
                    .await;
            }
            Err(e) => tracing::warn!(error = ?e, party = a.party_id, "rewriting a watch party card failed"),
        }
    }

    /// An update: write it now when the gap is over, else queue it for when it is.
    async fn update(&self, a: Announcement) {
        let wait = {
            let mut slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
            let slot = slots.entry(a.party_id).or_default();
            let due = slot
                .last_edit
                .map_or(Duration::ZERO, |t| self.gap.saturating_sub(t.elapsed()));
            if due.is_zero() {
                slot.last_edit = Some(Instant::now());
                slot.queued = None;
                None
            } else {
                slot.queued = Some(a.clone());
                if slot.waiting {
                    return;
                }
                slot.waiting = true;
                Some(due)
            }
        };
        let Some(wait) = wait else {
            self.rewrite(&a).await;
            return;
        };
        // The newest queued change is written when the gap is over.
        let me = PartyCards {
            pool: self.pool.clone(),
            chat: self.chat.clone(),
            slots: self.slots.clone(),
            gap: self.gap,
        };
        let party = a.party_id;
        tokio::spawn(async move {
            tokio::time::sleep(wait).await;
            let next = {
                let mut slots = me.slots.lock().unwrap_or_else(|p| p.into_inner());
                let Some(slot) = slots.get_mut(&party) else {
                    return;
                };
                slot.waiting = false;
                let next = slot.queued.take();
                if next.is_some() {
                    slot.last_edit = Some(Instant::now());
                }
                next
            };
            if let Some(next) = next {
                me.rewrite(&next).await;
            }
        });
    }

    /// The end: write the final state now and forget the party's throttle.
    async fn end(&self, a: Announcement) {
        self.slots
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&a.party_id);
        self.rewrite(&a).await;
    }
}

#[async_trait]
impl ChatHook for PartyCards {
    async fn announce(&self, a: Announcement) {
        match a.kind {
            AnnouncementKind::Started => self.post(&a).await,
            AnnouncementKind::Updated => self.update(a).await,
            AnnouncementKind::Ended => self.end(a).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ann(kind: AnnouncementKind, viewers: usize) -> Announcement {
        Announcement {
            party_id: 77,
            channel_id: 5,
            host_id: 1,
            title: "Cowboy Bebop".into(),
            episode_label: Some("S1 E4".into()),
            viewers,
            age_restricted: false,
            kind,
        }
    }

    #[test]
    fn the_embed_names_the_title_the_episode_and_the_headcount() {
        let e = card_embed(&ann(AnnouncementKind::Started, 3));
        assert_eq!(e.title.as_deref(), Some("Cowboy Bebop"));
        assert_eq!(e.description.as_deref(), Some("S1 E4 · 3 watching"));
        assert_eq!(e.url.as_deref(), Some("/cinema/party/77"));
        assert_eq!(e.color, LIVE_COLOR);
        assert_eq!(e.author.as_ref().map(|a| a.name.as_str()), Some("Watch party"));
        let one = card_embed(&ann(AnnouncementKind::Updated, 1));
        assert_eq!(one.description.as_deref(), Some("S1 E4 · 1 watching"));
    }

    #[test]
    fn an_ended_card_greys_out_and_says_how_many_watched() {
        let e = card_embed(&ann(AnnouncementKind::Ended, 4));
        assert_eq!(e.color, ENDED_COLOR);
        assert_eq!(e.description.as_deref(), Some("S1 E4 · Ended after 4 viewers"));
        assert_eq!(
            card_embed(&ann(AnnouncementKind::Ended, 1)).description.as_deref(),
            Some("S1 E4 · Ended after 1 viewer")
        );
    }

    #[test]
    fn restricted_and_empty_parties_do_not_leak_or_go_blank() {
        let mut a = ann(AnnouncementKind::Started, 2);
        a.age_restricted = true;
        let e = card_embed(&a);
        assert_eq!(e.title.as_deref(), Some("Age-restricted title"));
        assert_eq!(e.description.as_deref(), Some("2 watching"));
        let mut empty = ann(AnnouncementKind::Started, 1);
        empty.title = String::new();
        empty.episode_label = None;
        let e = card_embed(&empty);
        assert_eq!(e.title.as_deref(), Some("Watch party"));
        assert_eq!(e.description.as_deref(), Some("1 watching"));
    }
}
