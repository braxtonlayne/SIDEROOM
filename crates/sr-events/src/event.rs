use serde::{Deserialize, Serialize};

use crate::ids::{ChannelId, GuildId, UserId};

/// Routing key for the bus. Subscribers only see events published to the topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Topic {
    Platform,
    Guild(GuildId),
    Channel(ChannelId),
    User(UserId),
}

/// Events emitted by domain logic. Extend as modules need them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DomainEvent {
    GuildCreated {
        guild: GuildId,
        owner: UserId,
    },
    GuildDeleted {
        guild: GuildId,
    },
    MemberJoined {
        guild: GuildId,
        user: UserId,
    },
    MemberLeft {
        guild: GuildId,
        user: UserId,
    },
    MessageCreated {
        guild: Option<GuildId>,
        channel: ChannelId,
        message_id: u64,
        author: UserId,
    },
    ChannelCreated {
        guild: GuildId,
        channel: ChannelId,
    },
    /// A feature's enabled flag or settings changed at `scope`. Caches key off this.
    FeatureSettingsChanged {
        feature_id: String,
        scope: Topic,
    },
}

impl DomainEvent {
    /// Default topics this event should be routed to.
    pub fn topics(&self) -> Vec<Topic> {
        match self {
            Self::GuildCreated { guild, .. }
            | Self::GuildDeleted { guild }
            | Self::MemberJoined { guild, .. }
            | Self::MemberLeft { guild, .. } => vec![Topic::Guild(*guild), Topic::Platform],
            Self::MessageCreated { guild, channel, .. } => {
                let mut t = vec![Topic::Channel(*channel)];
                if let Some(g) = guild {
                    t.push(Topic::Guild(*g));
                }
                t
            }
            Self::ChannelCreated { guild, channel } => {
                vec![Topic::Guild(*guild), Topic::Channel(*channel)]
            }
            Self::FeatureSettingsChanged { .. } => vec![Topic::Platform],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing() {
        let e = DomainEvent::MessageCreated {
            guild: Some(GuildId(1)),
            channel: ChannelId(2),
            message_id: 3,
            author: UserId(4),
        };
        assert_eq!(
            e.topics(),
            vec![Topic::Channel(ChannelId(2)), Topic::Guild(GuildId(1))]
        );
        let dm = DomainEvent::MessageCreated {
            guild: None,
            channel: ChannelId(2),
            message_id: 3,
            author: UserId(4),
        };
        assert_eq!(dm.topics(), vec![Topic::Channel(ChannelId(2))]);
    }

    #[test]
    fn serde_roundtrip() {
        let e = DomainEvent::GuildDeleted { guild: GuildId(9) };
        let s = serde_json::to_string(&e).unwrap();
        assert_eq!(serde_json::from_str::<DomainEvent>(&s).unwrap(), e);
    }
}
