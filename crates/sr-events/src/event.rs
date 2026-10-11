use serde::{Deserialize, Serialize};

use serde_json::Value;

use crate::ids::{ChannelId, GuildId, RoleId, UserId};

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
    /// Full `CHANNEL_CREATE` body (Discord channel object) in `payload`.
    ChannelCreatedFull {
        guild: GuildId,
        channel: ChannelId,
        payload: Value,
    },
    /// `CHANNEL_UPDATE`; `payload` is the channel object.
    ChannelUpdated {
        guild: GuildId,
        channel: ChannelId,
        payload: Value,
    },
    /// `CHANNEL_DELETE`; `payload` is the deleted channel object.
    ChannelDeleted {
        guild: GuildId,
        channel: ChannelId,
        payload: Value,
    },
    /// `GUILD_UPDATE`; `payload` is the guild object.
    GuildUpdated {
        guild: GuildId,
        payload: Value,
    },
    /// `GUILD_ROLE_CREATE`; `payload` is `{guild_id, role}`.
    RoleCreated {
        guild: GuildId,
        role: RoleId,
        payload: Value,
    },
    /// `GUILD_ROLE_UPDATE`; `payload` is `{guild_id, role}`.
    RoleUpdated {
        guild: GuildId,
        role: RoleId,
        payload: Value,
    },
    /// `GUILD_ROLE_DELETE`; `payload` is `{guild_id, role_id}`.
    RoleDeleted {
        guild: GuildId,
        role: RoleId,
        payload: Value,
    },
    /// `GUILD_MEMBER_UPDATE`; `payload` is the member object plus `guild_id`.
    MemberUpdated {
        guild: GuildId,
        user: UserId,
        payload: Value,
    },
    /// `GUILD_BAN_ADD`; `payload` is `{guild_id, user}`.
    MemberBanned {
        guild: GuildId,
        user: UserId,
        payload: Value,
    },
    /// `GUILD_BAN_REMOVE`; `payload` is `{guild_id, user}`.
    MemberUnbanned {
        guild: GuildId,
        user: UserId,
        payload: Value,
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
            Self::GuildUpdated { guild, .. }
            | Self::RoleCreated { guild, .. }
            | Self::RoleUpdated { guild, .. }
            | Self::RoleDeleted { guild, .. }
            | Self::MemberUpdated { guild, .. }
            | Self::MemberBanned { guild, .. }
            | Self::MemberUnbanned { guild, .. } => vec![Topic::Guild(*guild)],
            Self::ChannelCreatedFull { guild, channel, .. }
            | Self::ChannelUpdated { guild, channel, .. }
            | Self::ChannelDeleted { guild, channel, .. } => {
                vec![Topic::Guild(*guild), Topic::Channel(*channel)]
            }
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
            guild: Some(GuildId::new(1)),
            channel: ChannelId::new(2),
            message_id: 3,
            author: UserId::new(4),
        };
        assert_eq!(
            e.topics(),
            vec![
                Topic::Channel(ChannelId::new(2)),
                Topic::Guild(GuildId::new(1))
            ]
        );
        let dm = DomainEvent::MessageCreated {
            guild: None,
            channel: ChannelId::new(2),
            message_id: 3,
            author: UserId::new(4),
        };
        assert_eq!(dm.topics(), vec![Topic::Channel(ChannelId::new(2))]);
    }

    #[test]
    fn guild_events_route_to_the_guild_topic() {
        let e = DomainEvent::RoleUpdated {
            guild: GuildId::new(1),
            role: RoleId::new(5),
            payload: serde_json::json!({ "guild_id": "1" }),
        };
        assert_eq!(e.topics(), vec![Topic::Guild(GuildId::new(1))]);
        let c = DomainEvent::ChannelUpdated {
            guild: GuildId::new(1),
            channel: ChannelId::new(2),
            payload: serde_json::Value::Null,
        };
        assert_eq!(c.topics().len(), 2);
        let s = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<DomainEvent>(&s).unwrap(), c);
    }

    #[test]
    fn serde_roundtrip() {
        let e = DomainEvent::GuildDeleted {
            guild: GuildId::new(9),
        };
        let s = serde_json::to_string(&e).unwrap();
        assert_eq!(serde_json::from_str::<DomainEvent>(&s).unwrap(), e);
    }
}
