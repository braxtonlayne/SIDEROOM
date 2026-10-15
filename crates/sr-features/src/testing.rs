//! In-memory [`FeatureActions`] that records every call, for module tests here and
//! for wiring tests elsewhere.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use async_trait::async_trait;
use sr_events::{ChannelId, GuildId, UserId};

use crate::actions::{
    ActionError, ActionResult, ChannelInfo, FeatureActions, GuildInfo, MessageInfo, OutMessage,
    UserInfo,
};

/// What the fake was asked to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Call {
    Post(ChannelId, OutMessage),
    Edit(ChannelId, u64, String),
    Delete(ChannelId, u64),
    Dm(UserId, OutMessage),
    RemoveReaction(ChannelId, u64, String, UserId),
    AddRole(GuildId, UserId, u64),
    RemoveRole(GuildId, UserId, u64),
    Thread(ChannelId, u64, String),
    Slowmode(ChannelId, u32),
    Rename(ChannelId, String),
}

#[derive(Default)]
struct Inner {
    calls: Vec<Call>,
    next_id: u64,
    messages: BTreeMap<(u64, u64), MessageInfo>,
    channels: BTreeMap<u64, ChannelInfo>,
    users: BTreeMap<u64, UserInfo>,
    guilds: BTreeMap<u64, GuildInfo>,
    reactions: BTreeMap<(u64, String), BTreeSet<u64>>,
    roles: BTreeSet<(u64, u64, u64)>,
    fail_posts: bool,
}

/// The system user id the fake posts as.
pub const SELF_USER: UserId = UserId::new(1);

pub struct FakeActions {
    inner: Mutex<Inner>,
}

impl Default for FakeActions {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                next_id: 9_000_000,
                ..Inner::default()
            }),
        }
    }
}

impl FakeActions {
    pub fn new() -> Self {
        Self::default()
    }

    fn with<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> R {
        f(&mut self.inner.lock().unwrap_or_else(|p| p.into_inner()))
    }

    pub fn calls(&self) -> Vec<Call> {
        self.with(|i| i.calls.clone())
    }

    pub fn clear_calls(&self) {
        self.with(|i| i.calls.clear());
    }

    pub fn put_message(&self, m: MessageInfo) {
        self.with(|i| i.messages.insert((m.channel.get(), m.id), m));
    }

    pub fn put_channel(&self, c: ChannelInfo) {
        self.with(|i| i.channels.insert(c.id.get(), c));
    }

    pub fn put_user(&self, id: u64, name: &str) {
        self.with(|i| {
            i.users.insert(
                id,
                UserInfo {
                    id: UserId::new(id),
                    name: name.to_owned(),
                    bot: false,
                },
            )
        });
    }

    pub fn put_guild(&self, id: u64, name: &str, member_count: u64) {
        self.with(|i| {
            i.guilds.insert(
                id,
                GuildInfo {
                    id: GuildId::new(id),
                    name: name.to_owned(),
                    member_count,
                },
            )
        });
    }

    pub fn react(&self, message: u64, key: &str, user: u64) {
        self.with(|i| {
            i.reactions
                .entry((message, key.to_owned()))
                .or_default()
                .insert(user)
        });
    }

    pub fn unreact(&self, message: u64, key: &str, user: u64) {
        self.with(|i| {
            if let Some(s) = i.reactions.get_mut(&(message, key.to_owned())) {
                s.remove(&user);
            }
        });
    }

    pub fn give_role(&self, guild: u64, user: u64, role: u64) {
        self.with(|i| i.roles.insert((guild, user, role)));
    }

    pub fn roles_of(&self, guild: u64, user: u64) -> Vec<u64> {
        self.with(|i| {
            i.roles
                .iter()
                .filter(|(g, u, _)| *g == guild && *u == user)
                .map(|(_, _, r)| *r)
                .collect()
        })
    }

    pub fn fail_posts(&self, fail: bool) {
        self.with(|i| i.fail_posts = fail);
    }

    /// The posted messages, in order.
    pub fn posts(&self) -> Vec<(ChannelId, OutMessage)> {
        self.calls()
            .into_iter()
            .filter_map(|c| match c {
                Call::Post(ch, m) => Some((ch, m)),
                _ => None,
            })
            .collect()
    }
}

#[async_trait]
impl FeatureActions for FakeActions {
    fn self_user(&self) -> UserId {
        SELF_USER
    }

    async fn post_message(&self, channel: ChannelId, msg: OutMessage) -> ActionResult<u64> {
        self.with(|i| {
            if i.fail_posts {
                return Err(ActionError::Forbidden("cannot post".into()));
            }
            i.next_id += 1;
            let id = i.next_id;
            i.calls.push(Call::Post(channel, msg.clone()));
            i.messages.insert(
                (channel.get(), id),
                MessageInfo {
                    id,
                    channel,
                    guild: i.channels.get(&channel.get()).and_then(|c| c.guild),
                    author: SELF_USER,
                    content: msg.content,
                    embeds: msg.embed.into_iter().collect(),
                    attachments: Vec::new(),
                },
            );
            Ok(id)
        })
    }

    async fn edit_message(
        &self,
        channel: ChannelId,
        message: u64,
        content: String,
    ) -> ActionResult<()> {
        self.with(|i| {
            i.calls.push(Call::Edit(channel, message, content.clone()));
            match i.messages.get_mut(&(channel.get(), message)) {
                Some(m) => {
                    m.content = content;
                    Ok(())
                }
                None => Err(ActionError::NotFound),
            }
        })
    }

    async fn delete_message(&self, channel: ChannelId, message: u64) -> ActionResult<()> {
        self.with(|i| {
            i.calls.push(Call::Delete(channel, message));
            i.messages
                .remove(&(channel.get(), message))
                .map(|_| ())
                .ok_or(ActionError::NotFound)
        })
    }

    async fn send_dm(&self, user: UserId, msg: OutMessage) -> ActionResult<()> {
        self.with(|i| i.calls.push(Call::Dm(user, msg)));
        Ok(())
    }

    async fn get_message(
        &self,
        channel: ChannelId,
        message: u64,
    ) -> ActionResult<Option<MessageInfo>> {
        Ok(self.with(|i| i.messages.get(&(channel.get(), message)).cloned()))
    }

    async fn get_channel(&self, channel: ChannelId) -> ActionResult<Option<ChannelInfo>> {
        Ok(self.with(|i| i.channels.get(&channel.get()).cloned()))
    }

    async fn get_user(&self, user: UserId) -> ActionResult<Option<UserInfo>> {
        Ok(self.with(|i| i.users.get(&user.get()).cloned()))
    }

    async fn get_guild(&self, guild: GuildId) -> ActionResult<Option<GuildInfo>> {
        Ok(self.with(|i| i.guilds.get(&guild.get()).cloned()))
    }

    async fn reaction_count(
        &self,
        _channel: ChannelId,
        message: u64,
        emoji_key: &str,
        exclude: Option<UserId>,
    ) -> ActionResult<u32> {
        Ok(self.with(|i| {
            i.reactions
                .get(&(message, emoji_key.to_owned()))
                .map(|s| {
                    s.iter()
                        .filter(|u| exclude.is_none_or(|e| e.get() != **u))
                        .count() as u32
                })
                .unwrap_or(0)
        }))
    }

    async fn remove_reaction(
        &self,
        channel: ChannelId,
        message: u64,
        emoji_key: &str,
        user: UserId,
    ) -> ActionResult<()> {
        self.with(|i| {
            i.calls.push(Call::RemoveReaction(
                channel,
                message,
                emoji_key.to_owned(),
                user,
            ));
            if let Some(s) = i.reactions.get_mut(&(message, emoji_key.to_owned())) {
                s.remove(&user.get());
            }
        });
        Ok(())
    }

    async fn add_role(&self, guild: GuildId, user: UserId, role: u64) -> ActionResult<()> {
        self.with(|i| {
            i.calls.push(Call::AddRole(guild, user, role));
            i.roles.insert((guild.get(), user.get(), role));
        });
        Ok(())
    }

    async fn remove_role(&self, guild: GuildId, user: UserId, role: u64) -> ActionResult<()> {
        self.with(|i| {
            i.calls.push(Call::RemoveRole(guild, user, role));
            i.roles.remove(&(guild.get(), user.get(), role));
        });
        Ok(())
    }

    async fn has_role(&self, guild: GuildId, user: UserId, role: u64) -> ActionResult<bool> {
        Ok(self.with(|i| i.roles.contains(&(guild.get(), user.get(), role))))
    }

    async fn create_thread(
        &self,
        channel: ChannelId,
        message: u64,
        name: &str,
    ) -> ActionResult<ChannelId> {
        self.with(|i| {
            i.calls
                .push(Call::Thread(channel, message, name.to_owned()));
            i.next_id += 1;
            Ok(ChannelId::new(i.next_id))
        })
    }

    async fn set_slowmode(&self, channel: ChannelId, seconds: u32) -> ActionResult<()> {
        self.with(|i| {
            i.calls.push(Call::Slowmode(channel, seconds));
            if let Some(c) = i.channels.get_mut(&channel.get()) {
                c.rate_limit_per_user = seconds;
            }
        });
        Ok(())
    }

    async fn set_channel_name(&self, channel: ChannelId, name: &str) -> ActionResult<()> {
        self.with(|i| {
            i.calls.push(Call::Rename(channel, name.to_owned()));
            if let Some(c) = i.channels.get_mut(&channel.get()) {
                c.name = name.to_owned();
            }
        });
        Ok(())
    }
}

/// A `FeatureCtx` over the fake, in-memory stores and a manual clock, plus the
/// handles tests poke at.
pub struct Harness {
    pub ctx: crate::FeatureCtx,
    pub actions: std::sync::Arc<FakeActions>,
    pub clock: std::sync::Arc<sr_core::ManualClock>,
    pub service: crate::FeatureService,
}

impl Harness {
    /// Build a harness with `registry` and every feature in it enabled in guild 1.
    pub async fn new(registry: crate::FeatureRegistry) -> Self {
        use std::sync::Arc;
        let ids: Vec<&'static str> = registry.iter().map(|f| f.id()).collect();
        let service = crate::FeatureService::new(
            Arc::new(registry),
            Arc::new(crate::InMemorySettingsStore::new()),
            None,
        );
        for id in ids {
            service
                .set_enabled(
                    id,
                    &crate::ScopeRef::Guild {
                        guild: GuildId::new(1),
                    },
                    Some(true),
                )
                .await
                .expect("enable feature");
        }
        let actions = Arc::new(FakeActions::new());
        let clock = Arc::new(sr_core::ManualClock::new(1_000_000));
        let bus: Arc<dyn sr_events::EventBus> = Arc::new(sr_events::InProcessBus::new());
        let ctx = crate::FeatureCtx {
            bus,
            settings: service.clone(),
            clock: clock.clone(),
            actions: actions.clone(),
            state: Arc::new(crate::state::InMemoryState::new()),
        };
        Self {
            ctx,
            actions,
            clock,
            service,
        }
    }

    pub fn now(&self) -> i64 {
        use sr_core::Clock;
        self.clock.now_unix_ms()
    }

    /// Replace the settings of `feature` in guild 1.
    pub async fn configure(&self, feature: &str, values: serde_json::Value) {
        self.service
            .set_settings(
                feature,
                &crate::ScopeRef::Guild {
                    guild: GuildId::new(1),
                },
                values.as_object().cloned().expect("settings object"),
            )
            .await
            .expect("valid settings");
    }
}
