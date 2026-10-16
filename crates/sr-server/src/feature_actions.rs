//! The production [`FeatureActions`]: what the built-in feature modules (starboard,
//! welcome, reaction roles, ...) do to the platform.
//!
//! * **Messages** are written as the SIDE ROOM system user (id 1) straight through the
//!   message repository, the way published announcement copies are, so the system user
//!   does not have to be a member of every server. Nobody is pinged except the users a
//!   module names; `@everyone` and role mentions are never set.
//! * **Roles, slowmode and channel names** go through the guild backend as the server's
//!   owner, so they get the same validation, gateway events and audit-log entries as a
//!   manual change. Audit entries therefore name the owner.
//! * **Threads** are started as the author of the message (falling back to the owner when
//!   the author may not start threads), through the normal thread service.
//! * **Reactions** are counted from Postgres and removed through the message store, with
//!   the usual `MESSAGE_REACTION_REMOVE` event.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;
use sr_admin::SYSTEM_USER_ID;
use sr_api::routes::{
    guilds::backend::GuildBackend,
    messages::{MessagesDeps, render::event_payload},
    threads::{ThreadsDeps, start::announce_started},
};
use sr_chat::{
    ChannelType, Member, Message, MessageType, Visibility,
    admin_rules::ChannelMove,
    channel_rules::ChannelPatch,
    message::Embed,
    message_rules::{DeletePlan, EditPlan, ResolvedMentions},
    repo::EmojiKey,
    social::repo::DmRepository,
    threads::StartThread,
};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, GuildId, MessageId, RoleId, UserId};
use sr_db::social_repo::PgSocialRepo;
use sr_events::DomainEvent;
use sr_features::actions::{
    ActionError, ActionResult, AttachmentInfo, ChannelInfo, FeatureActions, GuildInfo, MessageInfo,
    Occupancy, OutMessage, UserInfo,
};

/// Content limit of a message (Discord: 2000 characters).
const CONTENT_MAX: usize = 2000;

/// Why a voice move failed.
#[derive(Debug)]
pub enum VoiceMoveError {
    NotInVoice,
    Refused(String),
}

/// What join-to-create needs from the live voice layer (the gateway keeps voice states in
/// memory, so this cannot be answered from Postgres).
#[async_trait]
pub trait VoiceControl: Send + Sync {
    /// Members connected to `channel`, or `Unknown` when other nodes may hold members.
    fn occupants(&self, channel: ChannelId) -> Occupancy;
    /// Move `user` to `to` as `actor` (a moderator action: checked and announced like one).
    async fn move_member(
        &self,
        actor: UserId,
        guild: GuildId,
        user: UserId,
        to: ChannelId,
    ) -> Result<(), VoiceMoveError>;
}

/// The gateway as a [`VoiceControl`]. `single_node` is `EventBus::is_single_node()`: only then is the
/// gateway's in-memory voice state the whole picture, otherwise occupancy is `Unknown` (H-15).
pub struct GatewayVoice(pub sr_gateway::Gateway, pub bool);

#[async_trait]
impl VoiceControl for GatewayVoice {
    fn occupants(&self, channel: ChannelId) -> Occupancy {
        if !self.1 {
            return Occupancy::Unknown;
        }
        Occupancy::Known(
            self.0
                .voice_occupants(channel)
                .into_iter()
                .map(|s| s.user_id)
                .collect(),
        )
    }

    async fn move_member(
        &self,
        actor: UserId,
        guild: GuildId,
        user: UserId,
        to: ChannelId,
    ) -> Result<(), VoiceMoveError> {
        use sr_gateway::{ModError, ModPatch};
        self.0
            .moderate_voice(
                actor,
                guild,
                user,
                ModPatch {
                    channel: Some(Some(to)),
                    ..ModPatch::default()
                },
            )
            .await
            .map(|_| ())
            .map_err(|e| match e {
                ModError::NotInVoice => VoiceMoveError::NotInVoice,
                other => VoiceMoveError::Refused(format!("{other:?}")),
            })
    }
}

pub struct PgFeatureActions {
    pool: PgPool,
    messages: Arc<MessagesDeps>,
    threads: Arc<ThreadsDeps>,
    guilds: Arc<dyn GuildBackend>,
    voice: Option<Arc<dyn VoiceControl>>,
    base_url: String,
}

fn backend(e: impl std::fmt::Display) -> ActionError {
    ActionError::Backend(e.to_string())
}

fn chat(e: sr_chat::ChatError) -> ActionError {
    match e {
        sr_chat::ChatError::UnknownMessage
        | sr_chat::ChatError::UnknownChannel
        | sr_chat::ChatError::UnknownGuild
        | sr_chat::ChatError::UnknownMember
        | sr_chat::ChatError::UnknownRole => ActionError::NotFound,
        sr_chat::ChatError::MissingPermissions | sr_chat::ChatError::MissingAccess => {
            ActionError::Forbidden(e.to_string())
        }
        other => ActionError::Backend(other.to_string()),
    }
}

fn id64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

impl PgFeatureActions {
    pub fn new(
        pool: PgPool,
        messages: Arc<MessagesDeps>,
        threads: Arc<ThreadsDeps>,
        guilds: Arc<dyn GuildBackend>,
        public_url: &str,
    ) -> Self {
        Self {
            pool,
            messages,
            threads,
            guilds,
            voice: None,
            base_url: public_url.trim_end_matches('/').to_owned(),
        }
    }

    /// Give modules a view of, and a hand in, live voice (join-to-create).
    pub fn with_voice(mut self, voice: Arc<dyn VoiceControl>) -> Self {
        self.voice = Some(voice);
        self
    }

    fn system() -> UserId {
        UserId::new(SYSTEM_USER_ID as u64)
    }

    /// The owner of the guild a channel belongs to, the acting user for guild edits.
    async fn owner_of(&self, guild: GuildId) -> ActionResult<UserId> {
        self.messages
            .repo
            .get_guild(guild)
            .await
            .map_err(backend)?
            .map(|g| g.owner_id)
            .ok_or(ActionError::NotFound)
    }

    async fn channel(&self, id: ChannelId) -> ActionResult<sr_chat::Channel> {
        self.messages
            .repo
            .get_channel(id)
            .await
            .map_err(backend)?
            .ok_or(ActionError::NotFound)
    }

    /// Store a system-user message in `channel` and announce it.
    async fn store(&self, channel: ChannelId, msg: OutMessage) -> ActionResult<Message> {
        let ch = self.channel(channel).await?;
        if !ch.kind.accepts_messages() {
            return Err(ActionError::Forbidden("channel takes no messages".into()));
        }
        let embeds: Vec<Embed> = match msg.embed {
            Some(v) => vec![serde_json::from_value(v).map_err(backend)?],
            None => Vec::new(),
        };
        let content: String = msg.content.chars().take(CONTENT_MAX).collect();
        let m = Message {
            id: MessageId::from(self.messages.next_id()),
            channel_id: ch.id,
            guild_id: ch.guild_id,
            author_id: Self::system(),
            kind: MessageType::Default,
            content,
            flags: 0,
            edited_at_ms: None,
            reference: None,
            attachment_ids: Vec::new(),
            embeds,
            components: Vec::new(),
            sticker_ids: Vec::new(),
            poll: None,
            mention_everyone: false,
            mention_user_ids: msg.mention_users,
            mention_role_ids: Vec::new(),
            mention_channel_ids: Vec::new(),
            pinned: false,
            visibility: Visibility::Visible,
            deleted_at_ms: None,
            purge_after_ms: None,
            hold_id: None,
        };
        self.messages
            .repo
            .insert_message(&m)
            .await
            .map_err(backend)?;
        Ok(m)
    }

    async fn announce(&self, m: &Message) {
        match event_payload(&self.messages, m).await {
            Ok(data) => {
                self.messages
                    .publish(DomainEvent::MessageSent {
                        guild: m.guild_id,
                        channel: m.channel_id,
                        message_id: m.id.get(),
                        author: m.author_id,
                        data,
                    })
                    .await;
            }
            Err(_) => tracing::warn!("rendering a feature message failed"),
        }
    }

    /// Whether the `@everyone` role can read `ch`: the permissions of a member with no
    /// roles, computed with the channel's overwrites (a thread follows its parent).
    async fn everyone_can_view(&self, ch: &sr_chat::Channel) -> bool {
        let Some(gid) = ch.guild_id else { return false };
        let repo = &self.messages.repo;
        let (Ok(Some(guild)), Ok(roles)) = (repo.get_guild(gid).await, repo.list_roles(gid).await)
        else {
            return false;
        };
        // A member id no real account has, holding no roles.
        let ghost = Member {
            guild_id: gid,
            user_id: UserId::new(1 << 62),
            nick: None,
            role_ids: Vec::new(),
            joined_at_ms: 0,
            communication_disabled_until_ms: None,
            pending: false,
            avatar_key: None,
            banner_key: None,
            bio: None,
            extras: Default::default(),
        };
        sr_chat::threads::access::guild_channel_permissions(
            &**repo,
            &guild,
            &ghost,
            &roles,
            ch,
            self.messages.now_ms(),
        )
        .await
        .map(|p| p.has(Permissions::VIEW_CHANNEL))
        .unwrap_or(false)
    }
}

#[async_trait]
impl FeatureActions for PgFeatureActions {
    fn self_user(&self) -> UserId {
        Self::system()
    }

    fn base_url(&self) -> String {
        self.base_url.clone()
    }

    async fn post_message(&self, channel: ChannelId, msg: OutMessage) -> ActionResult<u64> {
        let m = self.store(channel, msg).await?;
        self.announce(&m).await;
        Ok(m.id.get())
    }

    async fn edit_message(
        &self,
        channel: ChannelId,
        message: u64,
        content: String,
    ) -> ActionResult<()> {
        let id = MessageId::new(message);
        let current = self
            .messages
            .repo
            .get_message(channel, id)
            .await
            .map_err(backend)?
            .filter(Message::is_visible)
            .ok_or(ActionError::NotFound)?;
        if current.author_id != Self::system() {
            return Err(ActionError::Forbidden(
                "only the system user's messages can be edited".into(),
            ));
        }
        let content: String = content.chars().take(CONTENT_MAX).collect();
        let plan = EditPlan {
            message_id: id,
            content: content.clone(),
            flags: current.flags,
            edited_at_ms: self.messages.now_ms(),
            mentions: ResolvedMentions {
                users: current.mention_user_ids.clone(),
                ..ResolvedMentions::default()
            },
            history_previous_content: (current.content != content).then(|| current.content.clone()),
        };
        self.messages
            .repo
            .apply_edit(&plan)
            .await
            .map_err(backend)?;
        if let Ok(Some(fresh)) = self.messages.repo.get_message(channel, id).await
            && let Ok(data) = event_payload(&self.messages, &fresh).await
        {
            self.messages
                .publish(DomainEvent::MessageEdited {
                    guild: fresh.guild_id,
                    channel,
                    message_id: message,
                    data,
                })
                .await;
        }
        Ok(())
    }

    async fn delete_message(&self, channel: ChannelId, message: u64) -> ActionResult<()> {
        let id = MessageId::new(message);
        let current = self
            .messages
            .repo
            .get_message(channel, id)
            .await
            .map_err(backend)?
            .filter(Message::is_visible)
            .ok_or(ActionError::NotFound)?;
        let now = self.messages.now_ms();
        let plan = DeletePlan {
            message_id: id,
            visibility: Visibility::UserDeleted,
            deleted_at_ms: now,
            purge_after_ms: current
                .hold_id
                .is_none()
                .then(|| now + self.messages.limits.purge_after_ms),
        };
        self.messages
            .repo
            .apply_delete(&plan)
            .await
            .map_err(backend)?;
        self.messages
            .publish(DomainEvent::MessageDeleted {
                guild: current.guild_id,
                channel,
                message_id: message,
            })
            .await;
        Ok(())
    }

    async fn send_dm(&self, user: UserId, msg: OutMessage) -> ActionResult<()> {
        let social = PgSocialRepo::new(self.pool.clone());
        let channel = match social
            .find_dm(Self::system(), user)
            .await
            .map_err(backend)?
        {
            Some(c) => c.id,
            None => {
                let id = ChannelId::new(self.messages.next_id().get());
                social
                    .insert_dm(id, Self::system(), user)
                    .await
                    .map_err(backend)?;
                id
            }
        };
        let m = self.store(channel, msg).await?;
        let plan = match self.messages.access(Self::system(), channel).await {
            Ok(acc) => {
                self.messages
                    .plan_dm_announce(&acc.channel, Self::system())
                    .await
            }
            Err(_) => Vec::new(),
        };
        self.messages.announce_dm(channel, &plan).await;
        self.announce(&m).await;
        Ok(())
    }

    async fn get_message(
        &self,
        channel: ChannelId,
        message: u64,
    ) -> ActionResult<Option<MessageInfo>> {
        let Some(m) = self
            .messages
            .repo
            .get_message(channel, MessageId::new(message))
            .await
            .map_err(backend)?
            .filter(Message::is_visible)
        else {
            return Ok(None);
        };
        // Attachment URLs come from the rendered Discord-shaped object.
        let attachments = match event_payload(&self.messages, &m).await {
            Ok(v) => v
                .get("attachments")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| {
                            Some(AttachmentInfo {
                                url: x.get("url")?.as_str()?.to_owned(),
                                filename: x
                                    .get("filename")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned(),
                                content_type: x
                                    .get("content_type")
                                    .and_then(Value::as_str)
                                    .map(str::to_owned),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        Ok(Some(MessageInfo {
            id: m.id.get(),
            channel: m.channel_id,
            guild: m.guild_id,
            author: m.author_id,
            content: m.content.clone(),
            embeds: m
                .embeds
                .iter()
                .filter_map(|e| serde_json::to_value(e).ok())
                .collect(),
            attachments,
        }))
    }

    async fn get_channel(&self, channel: ChannelId) -> ActionResult<Option<ChannelInfo>> {
        let Some(ch) = self
            .messages
            .repo
            .get_channel(channel)
            .await
            .map_err(backend)?
        else {
            return Ok(None);
        };
        let everyone = self.everyone_can_view(&ch).await;
        Ok(Some(ChannelInfo {
            id: ch.id,
            guild: ch.guild_id,
            kind: ch.kind.code(),
            name: ch.name.clone().unwrap_or_default(),
            nsfw: ch.nsfw,
            parent: ch.parent_id,
            rate_limit_per_user: ch.rate_limit_per_user,
            everyone_can_view: everyone,
        }))
    }

    async fn get_user(&self, user: UserId) -> ActionResult<Option<UserInfo>> {
        let found = self.messages.repo.users(&[user]).await.map_err(backend)?;
        Ok(found.into_iter().next().map(|u| UserInfo {
            id: u.id,
            name: u.display_name.unwrap_or(u.username),
            bot: u.bot,
        }))
    }

    async fn get_guild(&self, guild: GuildId) -> ActionResult<Option<GuildInfo>> {
        let Some(g) = self.messages.repo.get_guild(guild).await.map_err(backend)? else {
            return Ok(None);
        };
        let count = self
            .messages
            .repo
            .count_members(guild)
            .await
            .map_err(backend)?;
        Ok(Some(GuildInfo {
            id: g.id,
            name: g.name,
            member_count: count,
        }))
    }

    async fn reaction_count(
        &self,
        _channel: ChannelId,
        message: u64,
        emoji_key: &str,
        exclude: Option<UserId>,
    ) -> ActionResult<u32> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM reactions \
             WHERE message_id = $1 AND emoji_key = $2 AND ($3::BIGINT IS NULL OR user_id <> $3)",
        )
        .bind(id64(message))
        .bind(emoji_key)
        .bind(exclude.map(|u| id64(u.get())))
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    }

    async fn remove_reaction(
        &self,
        channel: ChannelId,
        message: u64,
        emoji_key: &str,
        user: UserId,
    ) -> ActionResult<()> {
        let Some(key) = EmojiKey::parse(emoji_key) else {
            return Ok(());
        };
        let removed = self
            .messages
            .store
            .remove_reaction(MessageId::new(message), user, &key)
            .await
            .map_err(backend)?;
        if removed {
            let guild = self
                .messages
                .repo
                .get_channel(channel)
                .await
                .ok()
                .flatten()
                .and_then(|c| c.guild_id);
            self.messages
                .publish(DomainEvent::ReactionRemoved {
                    guild,
                    channel,
                    message_id: message,
                    user,
                    emoji: key.json(),
                })
                .await;
        }
        Ok(())
    }

    async fn add_role(&self, guild: GuildId, user: UserId, role: u64) -> ActionResult<()> {
        let owner = self.owner_of(guild).await?;
        self.guilds
            .assign_role(owner, guild, user, RoleId::new(role), true)
            .await
            .map_err(chat)
    }

    async fn remove_role(&self, guild: GuildId, user: UserId, role: u64) -> ActionResult<()> {
        let owner = self.owner_of(guild).await?;
        self.guilds
            .assign_role(owner, guild, user, RoleId::new(role), false)
            .await
            .map_err(chat)
    }

    async fn has_role(&self, guild: GuildId, user: UserId, role: u64) -> ActionResult<bool> {
        let has: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM member_roles \
             WHERE guild_id = $1 AND user_id = $2 AND role_id = $3)",
        )
        .bind(id64(guild.get()))
        .bind(id64(user.get()))
        .bind(id64(role))
        .fetch_one(&self.pool)
        .await
        .map_err(backend)?;
        Ok(has)
    }

    async fn create_thread(
        &self,
        channel: ChannelId,
        message: u64,
        name: &str,
    ) -> ActionResult<ChannelId> {
        let msg = self
            .messages
            .repo
            .get_message(channel, MessageId::new(message))
            .await
            .map_err(backend)?
            .ok_or(ActionError::NotFound)?;
        let ch = self.channel(channel).await?;
        let guild = ch.guild_id.ok_or(ActionError::NotFound)?;
        let input = || StartThread {
            name: name.to_owned(),
            ..StartThread::default()
        };
        // The message's author starts the thread; if they may not, the owner does.
        let started = match self
            .threads
            .ops
            .start_from_message(msg.author_id, channel, msg.id, input())
            .await
        {
            Ok(s) => s,
            Err(sr_chat::ChatError::MissingPermissions | sr_chat::ChatError::MissingAccess) => {
                let owner = self.owner_of(guild).await?;
                self.threads
                    .ops
                    .start_from_message(owner, channel, msg.id, input())
                    .await
                    .map_err(chat)?
            }
            Err(e) => return Err(chat(e)),
        };
        announce_started(&self.threads, &self.messages, &started)
            .await
            .map_err(|_| ActionError::Backend("announcing the thread failed".into()))?;
        Ok(started.view.channel.id)
    }

    async fn set_slowmode(&self, channel: ChannelId, seconds: u32) -> ActionResult<()> {
        let ch = self.channel(channel).await?;
        let owner = self
            .owner_of(ch.guild_id.ok_or(ActionError::NotFound)?)
            .await?;
        self.guilds
            .edit_channel(
                owner,
                channel,
                ChannelPatch {
                    rate_limit_per_user: Some(seconds),
                    ..ChannelPatch::default()
                },
                ChannelMove::default(),
            )
            .await
            .map(|_| ())
            .map_err(chat)
    }

    async fn voice_occupants(&self, channel: ChannelId) -> ActionResult<Occupancy> {
        Ok(self
            .voice
            .as_ref()
            .map_or(Occupancy::Known(Vec::new()), |v| v.occupants(channel)))
    }

    async fn move_member(&self, guild: GuildId, user: UserId, to: ChannelId) -> ActionResult<()> {
        let voice = self
            .voice
            .as_ref()
            .ok_or_else(|| ActionError::Backend("voice is not available".into()))?;
        let owner = self.owner_of(guild).await?;
        voice
            .move_member(owner, guild, user, to)
            .await
            .map_err(|e| match e {
                VoiceMoveError::NotInVoice => ActionError::NotFound,
                VoiceMoveError::Refused(why) => ActionError::Forbidden(why),
            })
    }

    async fn create_voice_channel(
        &self,
        guild: GuildId,
        name: &str,
        parent: Option<ChannelId>,
        user_limit: u32,
        owner: Option<UserId>,
    ) -> ActionResult<ChannelId> {
        let acting = self.owner_of(guild).await?;
        let mut input = sr_chat::channel_rules::NewChannel::new(ChannelType::Voice, name);
        input.parent_id = parent;
        input.user_limit = (user_limit > 0).then_some(user_limit);
        // The room's creator manages that one channel: rename, limit, lock, move members.
        let overwrites = owner
            .map(|o| {
                vec![sr_core::permissions::Overwrite {
                    id: sr_core::Snowflake::new(o.get()),
                    kind: sr_core::permissions::OverwriteKind::Member,
                    allow: Permissions::VIEW_CHANNEL
                        | Permissions::CONNECT
                        | Permissions::MANAGE_CHANNELS
                        | Permissions::MOVE_MEMBERS,
                    deny: Permissions::empty(),
                }]
            })
            .unwrap_or_default();
        self.guilds
            .create_channel(acting, guild, input, overwrites)
            .await
            .map(|v| v.channel.id)
            .map_err(chat)
    }

    async fn delete_channel(&self, channel: ChannelId) -> ActionResult<()> {
        let ch = self.channel(channel).await?;
        let owner = self
            .owner_of(ch.guild_id.ok_or(ActionError::NotFound)?)
            .await?;
        self.guilds
            .delete_channel(owner, channel)
            .await
            .map(|_| ())
            .map_err(chat)
    }

    async fn set_channel_name(&self, channel: ChannelId, name: &str) -> ActionResult<()> {
        let ch = self.channel(channel).await?;
        if ch.kind == ChannelType::Category {
            return Err(ActionError::Forbidden(
                "categories are not renamed here".into(),
            ));
        }
        let owner = self
            .owner_of(ch.guild_id.ok_or(ActionError::NotFound)?)
            .await?;
        self.guilds
            .edit_channel(
                owner,
                channel,
                ChannelPatch {
                    name: Some(name.to_owned()),
                    ..ChannelPatch::default()
                },
                ChannelMove::default(),
            )
            .await
            .map(|_| ())
            .map_err(chat)
    }
}
