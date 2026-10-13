use serde::{Deserialize, Serialize};

use serde_json::Value;

use crate::ids::{ChannelId, GuildId, RoleId, UserId};

/// Routing key for the bus. Subscribers only see events published to the topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Topic {
    /// Platform-wide lifecycle and staff events.
    Platform,
    /// A tap on the whole bus: the bus fans every event published to any other
    /// topic out here too (see [`crate::InProcessBus`]). For platform-internal
    /// consumers that must see every event of a family regardless of where its
    /// producer routed it (the search indexer, caches). Never publish to it
    /// directly from a producer, and never expose it to clients.
    ///
    /// An event published to N topics shows up N times, adjacent to each other;
    /// DM events are published once per recipient. Consumers that need each event
    /// once wrap the stream in [`crate::FirehoseDedup`].
    Firehose,
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
    /// A serious ban raised suspicion on the banned account's inviter (D-031).
    InviterSuspicion,
    /// An inviter's suspicion reached the escalation threshold.
    InviterEscalated,
    /// A staff appointment was revoked.
    StaffRevoked,
    /// The staff access-log hash chain failed verification.
    AccessLogChainBroken,
}

impl StaffAlertKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BannedPersonAttempt => "banned_person_attempt",
            Self::InviterSuspicion => "inviter_suspicion",
            Self::InviterEscalated => "inviter_escalated",
            Self::StaffRevoked => "staff_revoked",
            Self::AccessLogChainBroken => "access_log_chain_broken",
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
    /// `THREAD_CREATE`; `payload` is the thread channel object. Delivered to members
    /// who can view the thread (private threads: members and moderators).
    ThreadCreated {
        guild: GuildId,
        thread: ChannelId,
        payload: Value,
    },
    /// `THREAD_UPDATE`; as [`Self::ThreadCreated`].
    ThreadUpdated {
        guild: GuildId,
        thread: ChannelId,
        payload: Value,
    },
    /// `THREAD_DELETE`; `payload` is `{id, guild_id, parent_id, type}`. `viewers` is
    /// the audience computed before the delete.
    ThreadDeleted {
        guild: GuildId,
        thread: ChannelId,
        payload: Value,
        viewers: Vec<UserId>,
    },
    /// `THREAD_MEMBERS_UPDATE`; `payload` is `{id, guild_id, member_count,
    /// added_members?, removed_member_ids?}`. `also` must receive it even if they can
    /// no longer view the thread (just removed).
    ThreadMembersUpdated {
        guild: GuildId,
        thread: ChannelId,
        payload: Value,
        also: Vec<UserId>,
    },
    /// `THREAD_MEMBER_UPDATE` for the user's own sessions; `payload` is the thread
    /// member object plus `guild_id`.
    ThreadMemberUpdated {
        user: UserId,
        payload: Value,
    },
    /// A staff-facing alert (`SR_STAFF_ALERT`), published on `Topic::Platform`
    /// (open question 142).
    StaffAlert {
        kind: StaffAlertKind,
        /// Human-readable detail for the staff alerts channel; never contains
        /// secrets.
        detail: String,
    },
    /// A Discord dispatch addressed to specific users (relationship, ack, note,
    /// user-settings and similar self-scoped events). Producers publish it to each
    /// addressed user's `Topic::User`; the gateway sends `name` and `payload`
    /// as-is to that user's sessions.
    UserScoped {
        users: Vec<UserId>,
        name: String,
        payload: serde_json::Value,
    },
    /// A user's chosen presence changed (status via `PATCH /users/@me/presence`,
    /// custom status). Published on the user's own `Topic::User`; the gateway, which
    /// knows the live sessions, merges it with them (open question 119) and raises
    /// `PRESENCE_UPDATE` for the user's friends, their own sessions and the guilds
    /// their sessions are in (honouring `GUILD_PRESENCES`). Not sent anywhere else.
    PresenceChanged {
        user: UserId,
        /// The chosen status: `online`, `idle`, `dnd` or `invisible`.
        status: String,
        /// Custom status as a Discord activity (`type` 4), when one is set.
        custom_status: Option<Value>,
    },
    /// `USER_UPDATE` for the members of one guild that share it with the changed
    /// user; `payload` is the public user object. Producers publish one per
    /// guild of the user, in addition to a [`Self::UserScoped`] to friends.
    UserUpdated {
        guild: GuildId,
        user: UserId,
        payload: Value,
    },
    /// `INVITE_CREATE`; `payload` is the Discord invite object.
    InviteCreated {
        guild: GuildId,
        channel: ChannelId,
        payload: Value,
    },
    /// `INVITE_DELETE`; `payload` is `{channel_id, guild_id, code}`.
    InviteDeleted {
        guild: GuildId,
        channel: ChannelId,
        payload: Value,
    },
    /// `GUILD_EMOJIS_UPDATE`; `payload` is `{guild_id, emojis}`.
    EmojisUpdated {
        guild: GuildId,
        payload: Value,
    },
    /// `GUILD_STICKERS_UPDATE`; `payload` is `{guild_id, stickers}`.
    StickersUpdated {
        guild: GuildId,
        payload: Value,
    },
    /// `WEBHOOKS_UPDATE`; the gateway builds `{guild_id, channel_id}`.
    WebhooksUpdated {
        guild: GuildId,
        channel: ChannelId,
    },
    /// `AUTO_MODERATION_RULE_CREATE`; `payload` is the rule object. Delivered only
    /// to members with MANAGE_GUILD.
    AutoModRuleCreated {
        guild: GuildId,
        payload: Value,
    },
    /// `AUTO_MODERATION_RULE_UPDATE`; as [`Self::AutoModRuleCreated`].
    AutoModRuleUpdated {
        guild: GuildId,
        payload: Value,
    },
    /// `AUTO_MODERATION_RULE_DELETE`; `payload` is the deleted rule object.
    AutoModRuleDeleted {
        guild: GuildId,
        payload: Value,
    },
    /// `AUTO_MODERATION_ACTION_EXECUTION`; Discord payload. Delivered only to
    /// members with MANAGE_GUILD.
    AutoModActionExecuted {
        guild: GuildId,
        payload: Value,
    },
    /// `GUILD_AUDIT_LOG_ENTRY_CREATE`; `payload` is the audit entry plus
    /// `guild_id`. Delivered only to members with VIEW_AUDIT_LOG.
    AuditLogEntryCreated {
        guild: GuildId,
        payload: Value,
    },
    // --- rich messages: polls (LNG-MSG-010) ---
    /// `MESSAGE_POLL_VOTE_ADD`.
    PollVoteAdded {
        guild: Option<GuildId>,
        channel: ChannelId,
        message_id: u64,
        user: UserId,
        answer_id: u32,
    },
    /// `MESSAGE_POLL_VOTE_REMOVE`.
    PollVoteRemoved {
        guild: Option<GuildId>,
        channel: ChannelId,
        message_id: u64,
        user: UserId,
        answer_id: u32,
    },
    // --- end rich messages ---
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
            | Self::PollVoteAdded {
                guild: None,
                channel,
                ..
            }
            | Self::PollVoteRemoved {
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
            | Self::MemberUnbanned { guild, .. }
            | Self::InviteCreated { guild, .. }
            | Self::InviteDeleted { guild, .. }
            | Self::EmojisUpdated { guild, .. }
            | Self::StickersUpdated { guild, .. }
            | Self::WebhooksUpdated { guild, .. }
            | Self::AutoModRuleCreated { guild, .. }
            | Self::AutoModRuleUpdated { guild, .. }
            | Self::AutoModRuleDeleted { guild, .. }
            | Self::AutoModActionExecuted { guild, .. }
            | Self::AuditLogEntryCreated { guild, .. }
            | Self::UserUpdated { guild, .. } => vec![Topic::Guild(*guild)],
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
            | Self::PollVoteAdded { guild, channel, .. }
            | Self::PollVoteRemoved { guild, channel, .. }
            | Self::ChannelPinsUpdated { guild, channel, .. }
            | Self::TypingStarted { guild, channel, .. } => {
                let mut t = vec![Topic::Channel(*channel)];
                if let Some(g) = guild {
                    t.push(Topic::Guild(*g));
                }
                t
            }
            Self::ThreadCreated { guild, thread, .. }
            | Self::ThreadUpdated { guild, thread, .. }
            | Self::ThreadDeleted { guild, thread, .. }
            | Self::ThreadMembersUpdated { guild, thread, .. } => {
                vec![Topic::Guild(*guild), Topic::Channel(*thread)]
            }
            Self::ThreadMemberUpdated { user, .. } => vec![Topic::User(*user)],
            Self::UserScoped { users, .. } => users.iter().map(|u| Topic::User(*u)).collect(),
            Self::PresenceChanged { user, .. } => vec![Topic::User(*user)],
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
    fn user_scoped_routes_to_each_user_topic() {
        let e = DomainEvent::UserScoped {
            users: vec![UserId::new(1), UserId::new(2)],
            name: "RELATIONSHIP_ADD".into(),
            payload: serde_json::json!({ "id": "2", "type": 1 }),
        };
        assert_eq!(
            e.topics(),
            vec![Topic::User(UserId::new(1)), Topic::User(UserId::new(2))]
        );
        let s = serde_json::to_string(&e).unwrap();
        assert_eq!(serde_json::from_str::<DomainEvent>(&s).unwrap(), e);
    }

    #[test]
    fn presence_changed_routes_to_the_users_own_topic() {
        let e = DomainEvent::PresenceChanged {
            user: UserId::new(7),
            status: "dnd".into(),
            custom_status: Some(serde_json::json!({ "type": 4, "state": "busy" })),
        };
        assert_eq!(e.topics(), vec![Topic::User(UserId::new(7))]);
        let s = serde_json::to_string(&e).unwrap();
        assert_eq!(serde_json::from_str::<DomainEvent>(&s).unwrap(), e);
    }

    #[test]
    fn user_updated_routes_to_its_guild_topic() {
        let e = DomainEvent::UserUpdated {
            guild: GuildId::new(3),
            user: UserId::new(7),
            payload: serde_json::json!({ "id": "7" }),
        };
        assert_eq!(e.topics(), vec![Topic::Guild(GuildId::new(3))]);
        let s = serde_json::to_string(&e).unwrap();
        assert_eq!(serde_json::from_str::<DomainEvent>(&s).unwrap(), e);
    }

    #[test]
    fn settings_events_route_to_the_guild_topic() {
        let g = GuildId::new(1);
        let c = ChannelId::new(2);
        let p = serde_json::json!({ "guild_id": "1" });
        let all = vec![
            DomainEvent::InviteCreated {
                guild: g,
                channel: c,
                payload: p.clone(),
            },
            DomainEvent::InviteDeleted {
                guild: g,
                channel: c,
                payload: p.clone(),
            },
            DomainEvent::EmojisUpdated {
                guild: g,
                payload: p.clone(),
            },
            DomainEvent::StickersUpdated {
                guild: g,
                payload: p.clone(),
            },
            DomainEvent::WebhooksUpdated {
                guild: g,
                channel: c,
            },
            DomainEvent::AutoModRuleCreated {
                guild: g,
                payload: p.clone(),
            },
            DomainEvent::AutoModRuleUpdated {
                guild: g,
                payload: p.clone(),
            },
            DomainEvent::AutoModRuleDeleted {
                guild: g,
                payload: p.clone(),
            },
            DomainEvent::AutoModActionExecuted {
                guild: g,
                payload: p.clone(),
            },
            DomainEvent::AuditLogEntryCreated {
                guild: g,
                payload: p,
            },
        ];
        for e in all {
            assert_eq!(e.topics(), vec![Topic::Guild(g)], "{e:?}");
            let s = serde_json::to_string(&e).unwrap();
            assert_eq!(serde_json::from_str::<DomainEvent>(&s).unwrap(), e);
        }
    }

    #[test]
    fn thread_events_route_and_roundtrip() {
        let g = GuildId::new(1);
        let t = ChannelId::new(2);
        let p = serde_json::json!({ "id": "2" });
        for e in [
            DomainEvent::ThreadCreated {
                guild: g,
                thread: t,
                payload: p.clone(),
            },
            DomainEvent::ThreadUpdated {
                guild: g,
                thread: t,
                payload: p.clone(),
            },
            DomainEvent::ThreadDeleted {
                guild: g,
                thread: t,
                payload: p.clone(),
                viewers: vec![UserId::new(3)],
            },
            DomainEvent::ThreadMembersUpdated {
                guild: g,
                thread: t,
                payload: p.clone(),
                also: vec![],
            },
        ] {
            assert_eq!(e.topics(), vec![Topic::Guild(g), Topic::Channel(t)]);
            let s = serde_json::to_string(&e).unwrap();
            assert_eq!(serde_json::from_str::<DomainEvent>(&s).unwrap(), e);
        }
        let m = DomainEvent::ThreadMemberUpdated {
            user: UserId::new(9),
            payload: p,
        };
        assert_eq!(m.topics(), vec![Topic::User(UserId::new(9))]);
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
