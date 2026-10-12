//! Orchestration for the social domain: load state, run the pure rules, persist.

use super::dm::{self, AddPlan, DmAccess, DmChannel, DmKind, GroupEdit, RequestState, dm_access};
use super::notifications::{
    self, ChannelOverride, MentionKind, Mute, MuteDuration, NotificationLevel, ScopeSettings,
};
use super::presence::{self, CustomStatus, PresencePrefs, Status};
use super::profile::{self, Connection, Profile, ProfileEdit};
use super::read_state::{self, ReadState};
use super::relationships::{self, PairCounts, PairState, Plan, Relationship, RelationshipKind};
use super::repo::{SocialRepository, kind_of};
use super::{SocialError, SocialLimits, SocialResult};
use sr_core::{ChannelId, Clock, GuildId, MessageId, SnowflakeGenerator, UserId};
use std::sync::Arc;

pub struct SocialService<R> {
    repo: R,
    clock: Arc<dyn Clock>,
    ids: SnowflakeGenerator<Arc<dyn Clock>>,
    limits: SocialLimits,
}

/// Partial update of a guild (or DM-scope) notification setting.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeUpdate {
    /// Outer `None` = unchanged; inner `None` = inherit the guild default.
    pub level: Option<Option<NotificationLevel>>,
    pub suppress_everyone: Option<bool>,
    pub suppress_roles: Option<bool>,
    pub suppress_highlights: Option<bool>,
    pub mute_scheduled_events: Option<bool>,
    pub mobile_push: Option<bool>,
}

/// Outcome of opening a 1:1 conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenedDm {
    pub channel: DmChannel,
    pub created: bool,
    /// `Request` means it lands in the recipient's Message Requests.
    pub access: DmAccess,
}

/// Mutual context between a profile's viewer and its owner.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileExtras {
    pub mutual_friends: Vec<UserId>,
    /// Guilds both belong to; `nick` and `joined_at_ms` are the profile owner's.
    pub mutual_guilds: Vec<super::repo::GuildMembership>,
    pub friends_since_ms: Option<i64>,
}

/// Recipients of a user-level broadcast (`USER_UPDATE`, `PRESENCE_UPDATE`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Audience {
    pub friends: Vec<UserId>,
    pub guilds: Vec<GuildId>,
}

/// Resolved notification behaviour for a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedNotifications {
    pub level: NotificationLevel,
    pub muted: bool,
}

impl<R: SocialRepository> SocialService<R> {
    pub fn new(
        repo: R,
        clock: Arc<dyn Clock>,
        ids: SnowflakeGenerator<Arc<dyn Clock>>,
        limits: SocialLimits,
    ) -> Self {
        Self {
            repo,
            clock,
            ids,
            limits,
        }
    }

    /// The underlying repository (for composition and tests).
    pub fn repo(&self) -> &R {
        &self.repo
    }

    fn now(&self) -> i64 {
        self.clock.now_unix_ms()
    }

    fn channel_id(&self) -> ChannelId {
        ChannelId::from(self.ids.generate())
    }

    async fn pair_state(&self, me: UserId, them: UserId) -> SocialResult<PairState> {
        Ok(PairState {
            mine: kind_of(&self.repo.get_relationship(me, them).await?),
            theirs: kind_of(&self.repo.get_relationship(them, me).await?),
            they_ignore_me: self.repo.is_ignoring(them, me).await?,
        })
    }

    async fn pair_counts(&self, me: UserId, them: UserId) -> SocialResult<PairCounts> {
        Ok(PairCounts {
            mine: self.repo.counts(me).await?,
            theirs: self.repo.counts(them).await?,
        })
    }

    async fn require_user(&self, user: UserId) -> SocialResult<()> {
        if self.repo.user_exists(user).await? {
            Ok(())
        } else {
            Err(SocialError::UnknownUser)
        }
    }

    async fn commit(&self, plan: Plan) -> SocialResult<Plan> {
        if !plan.ops.is_empty() {
            self.repo.apply(&plan.ops).await?;
        }
        Ok(plan)
    }

    // ---- relationships -------------------------------------------------

    pub async fn list_relationships(&self, me: UserId) -> SocialResult<Vec<Relationship>> {
        Ok(self.repo.list_relationships(me).await?)
    }

    /// Send a friend request (or accept a pending incoming one).
    pub async fn send_friend_request(&self, me: UserId, them: UserId) -> SocialResult<Plan> {
        if me == them {
            return Err(SocialError::SelfTarget);
        }
        self.require_user(them).await?;
        let st = self.pair_state(me, them).await?;
        let counts = self.pair_counts(me, them).await?;
        let sources = self.repo.get_privacy(them).await?.friend_requests;
        let allow = if sources.everyone {
            true
        } else {
            let guild = self.repo.shares_guild(me, them).await?;
            let mutual = self.repo.has_mutual_friend(me, them).await?;
            sources.allows(guild, mutual)
        };
        let plan =
            relationships::plan_request(me, them, st, &counts, allow, &self.limits, self.now())?;
        self.commit(plan).await
    }

    /// Send a friend request to the user named `username` (case-insensitive).
    ///
    /// Discord answers identically whether the user is unknown, has blocked the
    /// caller or takes no requests, so none of those can be told apart: all three
    /// surface as [`SocialError::RequestsNotAccepted`]. Returns the resolved target
    /// with the plan.
    pub async fn send_friend_request_by_username(
        &self,
        me: UserId,
        username: &str,
    ) -> SocialResult<(UserId, Plan)> {
        let name = username.trim();
        let them = if name.is_empty() {
            None
        } else {
            self.repo.find_user_by_username(name).await?
        };
        let Some(them) = them else {
            return Err(SocialError::RequestsNotAccepted);
        };
        match self.send_friend_request(me, them).await {
            Ok(plan) => Ok((them, plan)),
            Err(SocialError::BlockedByThem | SocialError::UnknownUser) => {
                Err(SocialError::RequestsNotAccepted)
            }
            Err(e) => Err(e),
        }
    }

    pub async fn accept_friend_request(&self, me: UserId, them: UserId) -> SocialResult<Plan> {
        let st = self.pair_state(me, them).await?;
        let counts = self.pair_counts(me, them).await?;
        let plan = relationships::plan_accept(me, them, st, &counts, &self.limits, self.now())?;
        self.commit(plan).await
    }

    /// Remove a friend, cancel an outgoing request or decline an incoming one.
    pub async fn remove_relationship(&self, me: UserId, them: UserId) -> SocialResult<Plan> {
        let st = self.pair_state(me, them).await?;
        self.commit(relationships::plan_remove(me, them, st)?).await
    }

    pub async fn block(&self, me: UserId, them: UserId) -> SocialResult<Plan> {
        self.require_user(them).await?;
        let st = self.pair_state(me, them).await?;
        let counts = self.pair_counts(me, them).await?;
        let plan = relationships::plan_block(me, them, st, &counts, &self.limits, self.now())?;
        self.commit(plan).await
    }

    pub async fn unblock(&self, me: UserId, them: UserId) -> SocialResult<Plan> {
        let st = self.pair_state(me, them).await?;
        let plan = relationships::plan_clear(me, them, st, RelationshipKind::Blocked)?;
        self.commit(plan).await
    }

    /// Ignore `them`. Independent of friendship: friend and request rows stay.
    pub async fn ignore(&self, me: UserId, them: UserId) -> SocialResult<()> {
        self.require_user(them).await?;
        let st = self.pair_state(me, them).await?;
        let already = self.repo.is_ignoring(me, them).await?;
        if relationships::plan_ignore(me, them, st, already)? {
            self.repo.put_ignore(me, them).await?;
        }
        Ok(())
    }

    pub async fn unignore(&self, me: UserId, them: UserId) -> SocialResult<()> {
        if me == them {
            return Err(SocialError::SelfTarget);
        }
        Ok(self.repo.delete_ignore(me, them).await?)
    }

    /// Users `me` ignores.
    pub async fn list_ignores(&self, me: UserId) -> SocialResult<Vec<UserId>> {
        Ok(self.repo.list_ignores(me).await?)
    }

    /// Whether `viewer` has ignored `author` (message, activity and DM-notification filter).
    pub async fn is_ignoring(&self, viewer: UserId, author: UserId) -> SocialResult<bool> {
        Ok(self.repo.is_ignoring(viewer, author).await?)
    }

    /// Set (or with empty text, clear) `me`'s private note on `them` (open question 111).
    pub async fn set_note(&self, me: UserId, them: UserId, note: Option<&str>) -> SocialResult<()> {
        if me == them {
            return Err(SocialError::SelfTarget);
        }
        self.require_user(them).await?;
        let note = super::clean_text("note", note, super::MAX_NOTE_CHARS)?;
        Ok(self.repo.set_note(me, them, note.as_deref()).await?)
    }

    pub async fn get_note(&self, me: UserId, them: UserId) -> SocialResult<Option<String>> {
        Ok(self.repo.get_note(me, them).await?)
    }

    pub async fn set_friend_nickname(
        &self,
        me: UserId,
        them: UserId,
        nickname: Option<&str>,
    ) -> SocialResult<Plan> {
        let st = self.pair_state(me, them).await?;
        let plan = relationships::plan_nickname(me, them, st, nickname, &self.limits)?;
        self.commit(plan).await
    }

    // ---- DMs and group DMs ----------------------------------------------

    async fn friend_candidates(
        &self,
        me: UserId,
        users: &[UserId],
    ) -> SocialResult<Vec<(UserId, Option<RelationshipKind>)>> {
        let mut out = Vec::with_capacity(users.len());
        for u in users {
            self.require_user(*u).await?;
            out.push((*u, kind_of(&self.repo.get_relationship(me, *u).await?)));
        }
        Ok(out)
    }

    /// Find or create the 1:1 DM with `target` (`POST /users/@me/channels`).
    pub async fn open_dm(&self, me: UserId, target: UserId) -> SocialResult<OpenedDm> {
        if me == target {
            return Err(SocialError::SelfTarget);
        }
        self.require_user(target).await?;
        let st = self.pair_state(me, target).await?;
        let privacy = self.repo.get_privacy(target).await?.dms;
        let shares = self.repo.shares_guild(me, target).await?;
        let existing = self.repo.find_dm(me, target).await?;
        let access = dm_access(st, &privacy, shares);
        // Friends/blocks decide; an existing conversation stays reachable unless blocked.
        if access == DmAccess::Denied && (existing.is_none() || dm_blocked(st)) {
            return Err(SocialError::DmNotAllowed);
        }
        match existing {
            Some(channel) => {
                self.repo.set_hidden(me, channel.id, false).await?;
                Ok(OpenedDm {
                    channel,
                    created: false,
                    access: if access == DmAccess::Denied {
                        DmAccess::Allowed
                    } else {
                        access
                    },
                })
            }
            None => {
                let id = self.channel_id();
                self.repo.insert_dm(id, me, target).await?;
                // A first DM from a non-friend lands in the recipient's Message Requests.
                if access == DmAccess::Request {
                    self.repo
                        .set_request_state(target, id, RequestState::Pending)
                        .await?;
                }
                let channel = self
                    .repo
                    .get_dm(id)
                    .await?
                    .ok_or(SocialError::UnknownChannel)?;
                Ok(OpenedDm {
                    channel,
                    created: true,
                    access,
                })
            }
        }
    }

    pub async fn create_group_dm(
        &self,
        owner: UserId,
        recipients: &[UserId],
        name: Option<&str>,
    ) -> SocialResult<DmChannel> {
        let cands = self.friend_candidates(owner, recipients).await?;
        let groups = self.repo.count_groups(owner).await?;
        let members = dm::plan_group_create(owner, &cands, groups, &self.limits)?;
        let name = super::clean_text("name", name, self.limits.max_group_name_chars)?;
        let id = self.channel_id();
        self.repo
            .insert_group(id, &members, name.as_deref())
            .await?;
        self.get_dm_for(owner, id).await
    }

    async fn get_dm_for(&self, user: UserId, id: ChannelId) -> SocialResult<DmChannel> {
        let ch = self
            .repo
            .get_dm(id)
            .await?
            .ok_or(SocialError::UnknownChannel)?;
        if !ch.has(user) {
            return Err(SocialError::NotParticipant);
        }
        Ok(ch)
    }

    /// Returns the affected conversation: the group itself, or a freshly created
    /// group when adding to a 1:1 DM; `None` when nobody new was added.
    pub async fn add_recipients(
        &self,
        actor: UserId,
        channel: ChannelId,
        users: &[UserId],
    ) -> SocialResult<Option<DmChannel>> {
        let ch = self.get_dm_for(actor, channel).await?;
        let cands = self.friend_candidates(actor, users).await?;
        let groups = self.repo.count_groups(actor).await?;
        match dm::plan_add_recipients(actor, &ch, &cands, groups, &self.limits)? {
            AddPlan::Nothing => Ok(None),
            AddPlan::ToGroup(new) => {
                self.repo.add_recipients(channel, &new).await?;
                Ok(Some(self.get_dm_for(actor, channel).await?))
            }
            AddPlan::NewGroup { members } => {
                let id = self.channel_id();
                self.repo.insert_group(id, &members, None).await?;
                Ok(Some(self.get_dm_for(actor, id).await?))
            }
        }
    }

    /// Kick (owner) or leave (`target == actor`). Returns the updated group, or
    /// `None` when the channel was deleted because nobody remains.
    pub async fn remove_recipient(
        &self,
        actor: UserId,
        channel: ChannelId,
        target: UserId,
    ) -> SocialResult<Option<DmChannel>> {
        let ch = self.get_dm_for(actor, channel).await?;
        let plan = dm::plan_remove_recipient(actor, &ch, target)?;
        if plan.delete_channel {
            self.repo.delete_dm(channel).await?;
            return Ok(None);
        }
        self.repo.remove_recipient(channel, plan.removed).await?;
        if let Some(owner) = plan.new_owner {
            self.repo.set_owner(channel, owner).await?;
        }
        Ok(self.repo.get_dm(channel).await?)
    }

    pub async fn transfer_owner(
        &self,
        actor: UserId,
        channel: ChannelId,
        new_owner: UserId,
    ) -> SocialResult<DmChannel> {
        let ch = self.get_dm_for(actor, channel).await?;
        dm::plan_transfer_owner(actor, &ch, new_owner)?;
        self.repo.set_owner(channel, new_owner).await?;
        self.get_dm_for(actor, channel).await
    }

    pub async fn edit_group(
        &self,
        actor: UserId,
        channel: ChannelId,
        edit: &GroupEdit,
    ) -> SocialResult<DmChannel> {
        let ch = self.get_dm_for(actor, channel).await?;
        let up = dm::plan_group_edit(actor, &ch, edit, &self.limits)?;
        self.repo
            .update_group(channel, up.name.as_deref(), up.icon_key.as_deref())
            .await?;
        self.get_dm_for(actor, channel).await
    }

    /// "Close DM": hide it from the list; it reappears on the next message.
    pub async fn close_dm(&self, user: UserId, channel: ChannelId) -> SocialResult<()> {
        self.get_dm_for(user, channel).await?;
        self.repo.set_hidden(user, channel, true).await?;
        Ok(())
    }

    /// `user`'s Message Request state in a conversation (`None` if not a recipient).
    pub async fn request_state(
        &self,
        user: UserId,
        channel: ChannelId,
    ) -> SocialResult<Option<RequestState>> {
        Ok(self.repo.get_request_state(user, channel).await?)
    }

    /// Move a pending request to accepted, ignored or spam. `Pending` cannot be
    /// chosen by a user; only a new non-friend DM sets it.
    pub async fn respond_to_request(
        &self,
        user: UserId,
        channel: ChannelId,
        state: RequestState,
    ) -> SocialResult<()> {
        if state == RequestState::Pending {
            return Err(SocialError::invalid(
                "state",
                "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
                "pending is not a choice",
            ));
        }
        self.get_dm_for(user, channel).await?;
        Ok(self.repo.set_request_state(user, channel, state).await?)
    }

    /// Reopen for every recipient who closed it (called when a message arrives).
    /// Returns the users whose row was actually un-hidden.
    pub async fn reopen_for_message(&self, channel: ChannelId) -> SocialResult<Vec<UserId>> {
        let mut reopened = Vec::new();
        if let Some(ch) = self.repo.get_dm(channel).await? {
            for u in &ch.recipients {
                if self.repo.set_hidden(*u, channel, false).await? {
                    reopened.push(*u);
                }
            }
        }
        Ok(reopened)
    }

    /// Recipients other than `author` who have not been told about this DM yet and
    /// may be: those who closed it, plus everyone when `first_message` (nobody but
    /// the opener has heard of a brand-new DM). Message requests that are still
    /// pending, ignored or spam stay hidden, as in Discord. Call before the
    /// message is stored, because storing it reopens closed DMs.
    pub async fn dm_announcees(
        &self,
        channel: ChannelId,
        author: UserId,
        first_message: bool,
    ) -> SocialResult<Vec<UserId>> {
        let Some(ch) = self.repo.get_dm(channel).await? else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for u in ch.recipients.iter().copied().filter(|u| *u != author) {
            if !first_message {
                let open = self
                    .repo
                    .list_dms(u, false)
                    .await?
                    .iter()
                    .any(|d| d.id == channel);
                if open {
                    continue;
                }
            }
            let state = self.repo.get_request_state(u, channel).await?;
            if matches!(state, None | Some(RequestState::Accepted)) {
                out.push(u);
            }
        }
        Ok(out)
    }

    pub async fn list_dms(&self, user: UserId) -> SocialResult<Vec<DmChannel>> {
        Ok(self.repo.list_dms(user, false).await?)
    }

    /// Whether `sender` may post in a 1:1 DM with `other` right now.
    pub async fn can_send_dm(&self, sender: UserId, other: UserId) -> SocialResult<bool> {
        let st = self.pair_state(sender, other).await?;
        Ok(!dm_blocked(st))
    }

    // ---- read states ---------------------------------------------------

    pub async fn ack(
        &self,
        user: UserId,
        channel: ChannelId,
        up_to: MessageId,
    ) -> SocialResult<ReadState> {
        let prev = self.repo.get_read_state(user, channel).await?;
        let s = read_state::ack(prev, user, channel, up_to);
        self.repo.put_read_state(&s).await?;
        Ok(s)
    }

    /// Mark many channels read at once ("mark server as read").
    pub async fn ack_all(
        &self,
        user: UserId,
        channels: &[(ChannelId, MessageId)],
    ) -> SocialResult<usize> {
        for (c, m) in channels {
            self.ack(user, *c, *m).await?;
        }
        Ok(channels.len())
    }

    pub async fn mark_unread(
        &self,
        user: UserId,
        channel: ChannelId,
        previous_message: Option<MessageId>,
        mention_count: u32,
    ) -> SocialResult<ReadState> {
        let prev = self.repo.get_read_state(user, channel).await?;
        let s = read_state::mark_unread(prev, user, channel, previous_message, mention_count);
        self.repo.put_read_state(&s).await?;
        Ok(s)
    }

    pub async fn ack_pins(&self, user: UserId, channel: ChannelId) -> SocialResult<ReadState> {
        let prev = self.repo.get_read_state(user, channel).await?;
        let s = read_state::ack_pins(prev, user, channel, self.now());
        self.repo.put_read_state(&s).await?;
        Ok(s)
    }

    /// Bump mention counts for the recipients of a new message (never the author).
    pub async fn record_mentions(
        &self,
        author: UserId,
        mentioned: &[UserId],
        channel: ChannelId,
    ) -> SocialResult<()> {
        let mut users: Vec<UserId> = mentioned.iter().copied().filter(|u| *u != author).collect();
        users.sort();
        users.dedup();
        if !users.is_empty() {
            self.repo.add_mentions(&users, channel, 1).await?;
        }
        Ok(())
    }

    /// Mention badges for a new message, counting only users `can_view` admits
    /// (open question 117). The send path supplies the VIEW_CHANNEL check.
    pub async fn record_eligible_mentions(
        &self,
        author: UserId,
        mentioned: &[UserId],
        channel: ChannelId,
        can_view: impl Fn(UserId) -> bool,
    ) -> SocialResult<()> {
        let users = read_state::mention_recipients(author, mentioned, can_view);
        if !users.is_empty() {
            self.repo.add_mentions(&users, channel, 1).await?;
        }
        Ok(())
    }

    pub async fn read_states(&self, user: UserId) -> SocialResult<Vec<ReadState>> {
        Ok(self.repo.list_read_states(user).await?)
    }

    // ---- notification settings -----------------------------------------

    pub async fn scope_settings(
        &self,
        user: UserId,
        guild: Option<GuildId>,
    ) -> SocialResult<ScopeSettings> {
        Ok(self
            .repo
            .get_scope(user, guild)
            .await?
            .unwrap_or_else(|| ScopeSettings::new(user, guild)))
    }

    pub async fn update_scope(
        &self,
        user: UserId,
        guild: Option<GuildId>,
        up: &ScopeUpdate,
    ) -> SocialResult<ScopeSettings> {
        let mut s = self.scope_settings(user, guild).await?;
        if let Some(l) = up.level {
            s.level = l;
        }
        if let Some(v) = up.suppress_everyone {
            s.suppress_everyone = v;
        }
        if let Some(v) = up.suppress_roles {
            s.suppress_roles = v;
        }
        if let Some(v) = up.suppress_highlights {
            s.suppress_highlights = v;
        }
        if let Some(v) = up.mute_scheduled_events {
            s.mute_scheduled_events = v;
        }
        if let Some(v) = up.mobile_push {
            s.mobile_push = v;
        }
        self.repo.put_scope(&s).await?;
        Ok(s)
    }

    /// Mute (`Some`) or unmute (`None`) a whole guild / the DM scope.
    pub async fn mute_scope(
        &self,
        user: UserId,
        guild: Option<GuildId>,
        duration: Option<MuteDuration>,
    ) -> SocialResult<ScopeSettings> {
        let mut s = self.scope_settings(user, guild).await?;
        s.mute = duration.map_or_else(Mute::default, |d| Mute::new(d, self.now()));
        self.repo.put_scope(&s).await?;
        Ok(s)
    }

    /// Set a channel/category override. Outer `None` leaves a field unchanged;
    /// `mute: Some(None)` unmutes.
    pub async fn update_override(
        &self,
        user: UserId,
        channel: ChannelId,
        level: Option<Option<NotificationLevel>>,
        mute: Option<Option<MuteDuration>>,
    ) -> SocialResult<Option<ChannelOverride>> {
        let mut o = self
            .repo
            .get_override(user, channel)
            .await?
            .unwrap_or_else(|| ChannelOverride::new(user, channel));
        if let Some(l) = level {
            o.level = l;
        }
        if let Some(m) = mute {
            o.mute = m.map_or_else(Mute::default, |d| Mute::new(d, self.now()));
        }
        if o.level.is_none() && !o.mute.muted {
            self.repo.delete_override(user, channel).await?;
            return Ok(None);
        }
        self.repo.put_override(&o).await?;
        Ok(Some(o))
    }

    /// Resolve level and mute for a channel. `category` is the parent category's id.
    pub async fn resolve_notifications(
        &self,
        user: UserId,
        guild: Option<GuildId>,
        channel: ChannelId,
        category: Option<ChannelId>,
        guild_default: NotificationLevel,
    ) -> SocialResult<ResolvedNotifications> {
        let scope = self.repo.get_scope(user, guild).await?;
        let ch = self.repo.get_override(user, channel).await?;
        let cat = match category {
            Some(c) => self.repo.get_override(user, c).await?,
            None => None,
        };
        let now = self.now();
        Ok(ResolvedNotifications {
            level: notifications::effective_level(
                guild_default,
                scope.as_ref(),
                cat.as_ref(),
                ch.as_ref(),
            ),
            muted: notifications::is_muted(now, scope.as_ref(), cat.as_ref(), ch.as_ref()),
        })
    }

    /// Should a message raise a notification for `user`?
    #[allow(clippy::too_many_arguments)]
    pub async fn should_notify(
        &self,
        user: UserId,
        guild: Option<GuildId>,
        channel: ChannelId,
        category: Option<ChannelId>,
        guild_default: NotificationLevel,
        kind: MentionKind,
    ) -> SocialResult<bool> {
        let r = self
            .resolve_notifications(user, guild, channel, category, guild_default)
            .await?;
        let scope = self.repo.get_scope(user, guild).await?;
        Ok(notifications::should_notify(
            r.level,
            r.muted,
            kind,
            scope.as_ref(),
        ))
    }

    // ---- profiles -------------------------------------------------------

    pub async fn get_profile(&self, viewer: UserId, user: UserId) -> SocialResult<Profile> {
        let mut p = self
            .repo
            .get_profile(user)
            .await?
            .ok_or(SocialError::UnknownUser)?;
        p.connections = profile::visible_connections(&p, viewer);
        Ok(p)
    }

    /// What the viewer shares with `user`: mutual friends, mutual guilds (with the
    /// user's nickname and join time there) and the friendship's start. Empty for
    /// the viewer's own profile.
    pub async fn profile_extras(
        &self,
        viewer: UserId,
        user: UserId,
    ) -> SocialResult<ProfileExtras> {
        if viewer == user {
            return Ok(ProfileExtras::default());
        }
        let mine = self.repo.list_guild_memberships(viewer).await?;
        let theirs = self.repo.list_guild_memberships(user).await?;
        let mutual_guilds = theirs
            .into_iter()
            .filter(|t| mine.iter().any(|m| m.guild == t.guild))
            .collect();
        let friends_since_ms = self
            .repo
            .get_relationship(viewer, user)
            .await?
            .filter(|r| r.kind == RelationshipKind::Friend)
            .map(|r| r.since_ms);
        Ok(ProfileExtras {
            mutual_friends: self.repo.mutual_friends(viewer, user).await?,
            mutual_guilds,
            friends_since_ms,
        })
    }

    /// Who should hear about a change to `user`'s profile or presence: friends and
    /// the guilds they belong to.
    pub async fn audience(&self, user: UserId) -> SocialResult<Audience> {
        let friends = self
            .repo
            .list_relationships(user)
            .await?
            .into_iter()
            .filter(|r| r.kind == RelationshipKind::Friend)
            .map(|r| r.target)
            .collect();
        let guilds = self
            .repo
            .list_guild_memberships(user)
            .await?
            .into_iter()
            .map(|m| m.guild)
            .collect();
        Ok(Audience { friends, guilds })
    }

    pub async fn edit_profile(&self, user: UserId, edit: &ProfileEdit) -> SocialResult<Profile> {
        let current = self
            .repo
            .get_profile(user)
            .await?
            .ok_or(SocialError::UnknownUser)?;
        let updated = profile::apply_edit(current, edit, &self.limits)?;
        self.repo.put_profile(&updated).await?;
        Ok(updated)
    }

    /// Add or refresh a plain (unverified) link. Never sets `verified`; a re-link
    /// keeps an existing verified flag.
    pub async fn upsert_connection(&self, c: Connection) -> SocialResult<()> {
        self.store_connection(Connection {
            verified: false,
            ..c
        })
        .await
    }

    /// Store a connection proven by an OAuth flow (open question 118). Only OAuth
    /// completion code calls this.
    pub async fn upsert_verified_connection(&self, c: Connection) -> SocialResult<()> {
        self.store_connection(Connection {
            verified: true,
            ..c
        })
        .await
    }

    async fn store_connection(&self, mut c: Connection) -> SocialResult<()> {
        profile::validate_connection(&c)?;
        c.created_ms = self.now();
        let existing = self.repo.count_connections(c.user).await?;
        if existing >= self.limits.max_connections {
            return Err(SocialError::invalid(
                "connections",
                "BASE_TYPE_MAX_ITEMS",
                "too many connections",
            ));
        }
        Ok(self.repo.upsert_connection(&c).await?)
    }

    pub async fn remove_connection(
        &self,
        user: UserId,
        provider: &str,
        external_id: &str,
    ) -> SocialResult<()> {
        Ok(self
            .repo
            .delete_connection(user, provider, external_id)
            .await?)
    }

    // ---- presence -------------------------------------------------------

    /// Stored preferences with expiries applied (default: online, no custom status).
    pub async fn presence(&self, user: UserId) -> SocialResult<PresencePrefs> {
        Ok(self
            .repo
            .get_presence(user)
            .await?
            .unwrap_or_else(|| PresencePrefs::new(user))
            .resolved(self.now()))
    }

    /// The status other users see for `user`, given the live per-session statuses
    /// (open question 119). The chosen status comes from the stored prefs.
    pub async fn effective_status(
        &self,
        user: UserId,
        sessions: &[Status],
    ) -> SocialResult<Status> {
        let chosen = self.presence(user).await?.status;
        Ok(presence::merge_sessions(chosen, sessions))
    }

    pub async fn set_status(
        &self,
        user: UserId,
        status: Status,
        duration_ms: Option<i64>,
    ) -> SocialResult<PresencePrefs> {
        let cur = self.presence(user).await?;
        let new = presence::plan_set_status(cur, status, duration_ms, self.now())?;
        self.repo.put_presence(&new).await?;
        Ok(new)
    }

    pub async fn set_custom_status(
        &self,
        user: UserId,
        custom: CustomStatus,
    ) -> SocialResult<PresencePrefs> {
        let cur = self.presence(user).await?;
        let new = presence::plan_set_custom_status(cur, custom, &self.limits, self.now())?;
        self.repo.put_presence(&new).await?;
        Ok(new)
    }
}

fn dm_blocked(st: PairState) -> bool {
    relationships::is_blocked_either_way(st)
}

/// Kind check helper kept public for route layers.
pub fn is_group(dm: &DmChannel) -> bool {
    dm.kind == DmKind::Group
}
