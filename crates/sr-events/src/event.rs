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

/// What a [`DomainEvent::StaffAlert`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaffAlertKind {
    /// A registration was refused because the Person is banned.
    BannedPersonAttempt,
}

impl StaffAlertKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BannedPersonAttempt => "banned_person_attempt",
        }
    }
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
    /// `GUILD_MEMBER_ADD`; `payload` is the full member object (Discord shape).
    MemberJoined {
        guild: GuildId,
        user: UserId,
        payload: Value,
    },
    MemberLeft {
        guild: GuildId,
        user: UserId,
    },
    /// Full `CHANNEL_CREATE` body (Discord channel object) in `payload`.
    ChannelCreated {
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
    /// `CHANNEL_DELETE`; `payload` is the deleted channel object. `viewers` is the
    /// set of members who could see the channel before it was deleted, computed by
    /// the producer; the gateway delivers only to them (open question 159).
    ChannelDeleted {
        guild: GuildId,
        channel: ChannelId,
        payload: Value,
        viewers: Vec<UserId>,
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
    /// A message was created; `data` is the full Discord-shaped message object
    /// (dispatched as `MESSAGE_CREATE`). The only message-create event.
    MessageSent {
        guild: Option<GuildId>,
        channel: ChannelId,
        message_id: u64,
        author: UserId,
        data: serde_json::Value,
    },
    /// A message was edited or its flags changed (`MESSAGE_UPDATE`); `data` is the
    /// full updated message object.
    MessageEdited {
        guild: Option<GuildId>,
        channel: ChannelId,
        message_id: u64,
        data: serde_json::Value,
    },
    /// `MESSAGE_DELETE`.
    MessageDeleted {
        guild: Option<GuildId>,
        channel: ChannelId,
        message_id: u64,
    },
    /// `MESSAGE_DELETE_BULK`.
    MessagesBulkDeleted {
        guild: Option<GuildId>,
        channel: ChannelId,
        message_ids: Vec<u64>,
    },
    /// `MESSAGE_REACTION_ADD`; `emoji` is `{id, name}`.
    ReactionAdded {
        guild: Option<GuildId>,
        channel: ChannelId,
        message_id: u64,
        user: UserId,
        emoji: serde_json::Value,
    },
    /// `MESSAGE_REACTION_REMOVE`.
    ReactionRemoved {
        guild: Option<GuildId>,
        channel: ChannelId,
        message_id: u64,
        user: UserId,
        emoji: serde_json::Value,
    },
    /// `MESSAGE_REACTION_REMOVE_ALL`.
    ReactionsCleared {
        guild: Option<GuildId>,
        channel: ChannelId,
        message_id: u64,
    },
    /// `MESSAGE_REACTION_REMOVE_EMOJI`.
    ReactionEmojiCleared {
        guild: Option<GuildId>,
        channel: ChannelId,
        message_id: u64,
        emoji: serde_json::Value,
    },
    /// `CHANNEL_PINS_UPDATE`.
    ChannelPinsUpdated {
        guild: Option<GuildId>,
        channel: ChannelId,
        /// Unix ms of the most recent pin, if any remain.
        last_pin_ms: Option<i64>,
    },
    /// A staff-facing alert (`SR_STAFF_ALERT`), published on `Topic::Platform`
    /// (open question 142).
    StaffAlert {
        kind: StaffAlertKind,
        /// Human-readable detail for the staff alerts channel; never contains
        /// secrets.
        detail: String,
    },
    /// `TYPING_START` triggered by `POST /channels/{id}/typing`.
    TypingStarted {
        guild: Option<GuildId>,
        channel: ChannelId,
        user: UserId,
        /// Unix seconds.
        timestamp: u64,
    },
}

impl DomainEvent {
    /// The channel of a message-family event that happened in a DM or group DM
    /// (no guild). Producers route these to each recipient's `Topic::User`
    /// (open question 50) instead of using [`Self::topics`].
    pub fn dm_channel(&self) -> Option<ChannelId> {
        match self {
            Self::MessageSent {
                guild: None,
                channel,
                ..
            }
            | Self::MessageEdited {
                guild: None,
                channel,
                ..
            }
            | Self::MessageDeleted {
                guild: None,
                channel,
                ..
            }
            | Self::MessagesBulkDeleted {
                guild: None,
                channel,
                ..
            }
            | Self::ReactionAdded {
                guild: None,
                channel,
                ..
            }
            | Self::ReactionRemoved {
                guild: None,
                channel,
                ..
            }
            | Self::ReactionsCleared {
                guild: None,
                channel,
                ..
            }
            | Self::ReactionEmojiCleared {
                guild: None,
                channel,
                ..
            }
            | Self::ChannelPinsUpdated {
                guild: None,
                channel,
                ..
            }
            | Self::TypingStarted {
                guild: None,
                channel,
                ..
            } => Some(*channel),
            _ => None,
        }
    }

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
            Self::ChannelCreated { guild, channel, .. }
            | Self::ChannelUpdated { guild, channel, .. }
            | Self::ChannelDeleted { guild, channel, .. } => {
                vec![Topic::Guild(*guild), Topic::Channel(*channel)]
            }
            Self::MessageSent { guild, channel, .. }
            | Self::MessageEdited { guild, channel, .. }
            | Self::MessageDeleted { guild, channel, .. }
            | Self::MessagesBulkDeleted { guild, channel, .. }
            | Self::ReactionAdded { guild, channel, .. }
            | Self::ReactionRemoved { guild, channel, .. }
            | Self::ReactionsCleared { guild, channel, .. }
            | Self::ReactionEmojiCleared { guild, channel, .. }
            | Self::ChannelPinsUpdated { guild, channel, .. }
            | Self::TypingStarted { guild, channel, .. } => {
                let mut t = vec![Topic::Channel(*channel)];
                if let Some(g) = guild {
                    t.push(Topic::Guild(*g));
                }
                t
            }
            Self::FeatureSettingsChanged { .. } | Self::StaffAlert { .. } => vec![Topic::Platform],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing() {
        let e = DomainEvent::MessageSent {
            guild: Some(GuildId::new(1)),
            channel: ChannelId::new(2),
            message_id: 3,
            author: UserId::new(4),
            data: serde_json::Value::Null,
        };
        assert_eq!(
            e.topics(),
            vec![
                Topic::Channel(ChannelId::new(2)),
                Topic::Guild(GuildId::new(1))
            ]
        );
        let dm = DomainEvent::MessageSent {
            guild: None,
            channel: ChannelId::new(2),
            message_id: 3,
            author: UserId::new(4),
            data: serde_json::Value::Null,
        };
        assert_eq!(dm.topics(), vec![Topic::Channel(ChannelId::new(2))]);
    }

    #[test]
    fn message_family_routes_to_channel_and_guild() {
        let e = DomainEvent::ReactionAdded {
            guild: Some(GuildId::new(1)),
            channel: ChannelId::new(2),
            message_id: 3,
            user: UserId::new(4),
            emoji: serde_json::json!({"id": null, "name": "x"}),
        };
        assert_eq!(
            e.topics(),
            vec![
                Topic::Channel(ChannelId::new(2)),
                Topic::Guild(GuildId::new(1))
            ]
        );
        let s = serde_json::to_string(&e).unwrap();
        assert_eq!(serde_json::from_str::<DomainEvent>(&s).unwrap(), e);
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
    fn dm_events_are_identified_for_per_recipient_routing() {
        let dm = DomainEvent::MessageSent {
            guild: None,
            channel: ChannelId::new(2),
            message_id: 3,
            author: UserId::new(4),
            data: serde_json::Value::Null,
        };
        assert_eq!(dm.dm_channel(), Some(ChannelId::new(2)));
        let guild_msg = DomainEvent::MessageDeleted {
            guild: Some(GuildId::new(1)),
            channel: ChannelId::new(2),
            message_id: 3,
        };
        assert_eq!(guild_msg.dm_channel(), None);
    }

    #[test]
    fn staff_alert_routes_to_the_platform_topic() {
        let e = DomainEvent::StaffAlert {
            kind: StaffAlertKind::BannedPersonAttempt,
            detail: "x".into(),
        };
        assert_eq!(e.topics(), vec![Topic::Platform]);
        let s = serde_json::to_string(&e).unwrap();
        assert_eq!(serde_json::from_str::<DomainEvent>(&s).unwrap(), e);
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
