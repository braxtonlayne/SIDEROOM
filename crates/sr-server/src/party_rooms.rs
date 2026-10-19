//! Keeps the Watch party room flag of a channel object current for live clients (D-264).
//!
//! `sr_watch_party_room` is read with the channel (`CHANNEL_SELECT`), so it is true while the channel has a
//! standing party. When that party ends, or is made an ordinary one, nothing about the channel row changes,
//! so this hook re-reads the channel and publishes `CHANNEL_UPDATE` and clients drop the flag. (The other
//! direction, a room being created, is handled by the create-channel route.) Failures are logged only.

use std::sync::Arc;

use async_trait::async_trait;
use sr_api::routes::guilds::dto::ChannelObject;
use sr_chat::repo::ChannelRepository;
use sr_core::ChannelId;
use sr_events::{DomainEvent, EventBus};
use sr_watchparty::{Announcement, AnnouncementKind, ChatHook};

pub struct RoomFlagHook<R> {
    repo: R,
    bus: Arc<dyn EventBus>,
}

impl<R: ChannelRepository> RoomFlagHook<R> {
    pub fn new(repo: R, bus: Arc<dyn EventBus>) -> Self {
        Self { repo, bus }
    }
}

#[async_trait]
impl<R: ChannelRepository + 'static> ChatHook for RoomFlagHook<R> {
    async fn announce(&self, a: Announcement) {
        if a.kind != AnnouncementKind::RoomClosed {
            return;
        }
        let id = ChannelId::new(u64::try_from(a.channel_id).unwrap_or(0));
        let (channel, overwrites) = match (
            self.repo.get_channel(id).await,
            self.repo.get_overwrites(id).await,
        ) {
            (Ok(Some(c)), Ok(o)) => (c, o),
            (Err(e), _) | (_, Err(e)) => {
                tracing::warn!(error = %e, "re-reading a former watch party room failed");
                return;
            }
            _ => return,
        };
        let Some(guild) = channel.guild_id else {
            return;
        };
        let ev = DomainEvent::ChannelUpdated {
            guild,
            channel: channel.id,
            payload: serde_json::to_value(ChannelObject::new(&channel, &overwrites))
                .unwrap_or(serde_json::Value::Null),
        };
        for topic in ev.topics() {
            if let Err(e) = self.bus.publish(topic, ev.clone()).await {
                tracing::warn!(error = %e, "publishing CHANNEL_UPDATE failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use sr_chat::memory::MemoryRepo;
    use sr_chat::{Channel, ChannelType};
    use sr_core::GuildId;
    use sr_events::{InProcessBus, Topic};

    fn ann(kind: AnnouncementKind) -> Announcement {
        Announcement {
            party_id: 1,
            channel_id: 20,
            host_id: 1,
            title: String::new(),
            episode_label: None,
            viewers: 0,
            age_restricted: false,
            kind,
        }
    }

    #[tokio::test]
    async fn a_closed_room_sends_channel_update_with_the_flag_off() {
        let repo = MemoryRepo::default();
        let mut ch = Channel::new(
            ChannelId::new(20),
            Some(GuildId::new(10)),
            ChannelType::Voice,
        );
        ch.name = Some("Movie night".into());
        repo.insert_channel(&ch, &[]).await.unwrap();
        let bus = Arc::new(InProcessBus::new());
        let mut stream = bus.subscribe(Topic::Guild(GuildId::new(10)));
        let hook = RoomFlagHook::new(repo, bus.clone());

        // Other announcements are not its business.
        hook.announce(ann(AnnouncementKind::Ended)).await;
        hook.announce(ann(AnnouncementKind::RoomClosed)).await;

        let ev = stream.next().await.unwrap();
        let DomainEvent::ChannelUpdated {
            channel, payload, ..
        } = ev
        else {
            panic!("expected CHANNEL_UPDATE, got {ev:?}");
        };
        assert_eq!(channel, ChannelId::new(20));
        assert_eq!(payload["sr_watch_party_room"], false, "{payload}");
        assert_eq!(payload["name"], "Movie night");
    }
}
