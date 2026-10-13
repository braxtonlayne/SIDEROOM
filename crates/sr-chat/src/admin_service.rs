//! Guild administration use cases: guild, channel, role, member and ban
//! management with server-side permission checks. Orchestration only; the
//! decisions live in [`crate::admin_rules`] and the older rule modules.

use crate::admin_repo::{Ban, GuildAdminRepository};
use crate::admin_rules::{
    self, ChannelMove, GuildPatch, MemberPatch, MemberTarget, plan_channel_moves, plan_edit_guild,
    plan_edit_member, plan_role_order,
};
use crate::channel_rules::{self, ChannelPatch};
use crate::domain::{Channel, ChannelType, ChannelWithOverwrites, Guild, Member, Role};
use crate::error::{ChatError, ChatResult};
use crate::guild_rules::{self, ModAction, check_moderation};
use crate::perms::{GuildActor, channel_permissions, guild_permissions, top_role_position};
use crate::repo::ChatRepository;
use crate::role_rules::{self, RolePatch};
use crate::service::ChatService;
use sr_core::permissions::{Overwrite, OverwriteKind, Permissions};
use sr_core::{ChannelId, GuildId, MessageId, RoleId, Snowflake, UserId};

/// Page size bounds for member and ban listings (Discord: 1..=1000).
pub const MAX_PAGE: usize = 1000;

/// Loaded guild context for one acting member.
struct Ctx {
    guild: Guild,
    member: Member,
    roles: Vec<Role>,
    actor: GuildActor,
}

impl<R: ChatRepository + GuildAdminRepository> ChatService<R> {
    async fn ctx(&self, actor: UserId, guild_id: GuildId) -> ChatResult<Ctx> {
        let guild = self.guild(guild_id).await?;
        let (member, roles) = self.guild_actor(&guild, actor).await?;
        let a = GuildActor::new(&guild, &member, &roles);
        Ok(Ctx {
            guild,
            member,
            roles,
            actor: a,
        })
    }

    async fn target_member(&self, guild: GuildId, user: UserId) -> ChatResult<Member> {
        self.repo
            .get_member(guild, user)
            .await?
            .ok_or(ChatError::UnknownMember)
    }

    async fn with_overwrites(&self, channel: Channel) -> ChatResult<ChannelWithOverwrites> {
        let overwrites = self.repo.get_overwrites(channel.id).await?;
        Ok(ChannelWithOverwrites {
            channel,
            overwrites,
        })
    }

    // --------------------------------------------------------- guilds

    /// The guild and its roles; members only.
    pub async fn get_guild(&self, actor: UserId, id: GuildId) -> ChatResult<(Guild, Vec<Role>)> {
        let c = self.ctx(actor, id).await?;
        Ok((c.guild, c.roles))
    }

    /// Guilds the user belongs to with their guild-level permissions.
    pub async fn list_my_guilds(&self, user: UserId) -> ChatResult<Vec<(Guild, Permissions)>> {
        let guilds = self.repo.list_user_guilds(user).await?;
        let mut out = Vec::with_capacity(guilds.len());
        for g in guilds {
            let perms = match self.repo.get_member(g.id, user).await? {
                Some(m) => guild_permissions(&g, &m, &self.repo.list_roles(g.id).await?),
                None => Permissions::empty(),
            };
            out.push((g, perms));
        }
        Ok(out)
    }

    pub async fn edit_guild(
        &self,
        actor: UserId,
        id: GuildId,
        patch: GuildPatch,
    ) -> ChatResult<Guild> {
        let c = self.ctx(actor, id).await?;
        let new_owner = match patch.owner_id {
            Some(o) if o != c.guild.owner_id => Some(self.target_member(id, o).await?),
            _ => None,
        };
        let system_ok = match patch.system_channel_id.flatten() {
            Some(cid) => self
                .repo
                .get_channel(cid)
                .await?
                .is_some_and(|ch| ch.guild_id == Some(id) && ch.kind == ChannelType::Text),
            None => true,
        };
        let updated = plan_edit_guild(
            actor,
            &c.actor,
            &c.guild,
            patch,
            new_owner.as_ref(),
            system_ok,
        )?;
        self.repo.update_guild(&updated).await?;
        Ok(updated)
    }

    /// Owner-only; returns the deleted guild.
    pub async fn delete_guild(&self, actor: UserId, id: GuildId) -> ChatResult<Guild> {
        let c = self.ctx(actor, id).await?;
        admin_rules::check_delete_guild(&c.guild, actor)?;
        self.repo.delete_guild(id).await?;
        Ok(c.guild)
    }

    /// `DELETE /users/@me/guilds/{id}`.
    pub async fn leave_guild(&self, user: UserId, id: GuildId) -> ChatResult<()> {
        let c = self.ctx(user, id).await?;
        guild_rules::check_leave(&c.guild, user)?;
        self.repo.remove_member(id, user).await?;
        Ok(())
    }

    // ------------------------------------------------------- channels

    /// Channels of the guild the member can see, ordered by position.
    pub async fn list_channels(
        &self,
        actor: UserId,
        id: GuildId,
    ) -> ChatResult<Vec<ChannelWithOverwrites>> {
        let c = self.ctx(actor, id).await?;
        let mut channels = self.repo.list_guild_channels(id).await?;
        channels.sort_by_key(|ch| (ch.position, ch.id));
        let now = self.now();
        let mut out = Vec::with_capacity(channels.len());
        for ch in channels {
            let overwrites = self.repo.get_overwrites(ch.id).await?;
            let perms = channel_permissions(&c.guild, &c.member, &c.roles, &overwrites, now);
            if perms.has(Permissions::VIEW_CHANNEL) {
                out.push(ChannelWithOverwrites {
                    channel: ch,
                    overwrites,
                });
            }
        }
        Ok(out)
    }

    /// A guild channel the user may view. DMs belong to the DM endpoints.
    pub async fn get_channel_view(
        &self,
        actor: UserId,
        id: ChannelId,
    ) -> ChatResult<ChannelWithOverwrites> {
        let acc = self.access(actor, id).await?;
        if acc.channel.guild_id.is_none() {
            return Err(ChatError::NotInGuild);
        }
        if !acc.perms.has(Permissions::VIEW_CHANNEL) {
            return Err(ChatError::MissingAccess);
        }
        self.with_overwrites(acc.channel).await
    }

    async fn guild_channel(&self, id: ChannelId) -> ChatResult<(Channel, GuildId)> {
        let ch = self
            .repo
            .get_channel(id)
            .await?
            .ok_or(ChatError::UnknownChannel)?;
        let gid = ch.guild_id.ok_or(ChatError::NotInGuild)?;
        Ok((ch, gid))
    }

    /// Modify name/topic/etc. plus optional `position`/`parent_id` moves.
    pub async fn edit_channel(
        &self,
        actor: UserId,
        id: ChannelId,
        mut patch: ChannelPatch,
        mv: ChannelMove,
    ) -> ChatResult<ChannelWithOverwrites> {
        let (channel, gid) = self.guild_channel(id).await?;
        let c = self.ctx(actor, gid).await?;
        patch.forum.assign_tag_ids(|| self.ids.generate().get());
        let planned = channel_rules::plan_edit_channel(&c.actor, &channel, patch)?;
        let mut all = self.repo.list_guild_channels(gid).await?;
        all.retain(|x| x.id != id);
        all.push(planned);
        let lock = mv.lock_permissions;
        let moved = plan_channel_moves(&c.actor, gid, &all, &[(id, mv)])?;
        let updated = moved.into_iter().next().ok_or(ChatError::UnknownChannel)?;
        self.repo.update_channel(&updated).await?;
        if lock {
            self.sync_with_parent(&updated).await?;
        }
        self.with_overwrites(updated).await
    }

    async fn sync_with_parent(&self, channel: &Channel) -> ChatResult<()> {
        if let Some(parent) = channel.parent_id {
            let ov = channel_rules::synced_overwrites(&self.repo.get_overwrites(parent).await?);
            self.repo.replace_overwrites(channel.id, &ov).await?;
        }
        Ok(())
    }

    /// Bulk position/parent update (`PATCH /guilds/{id}/channels`).
    pub async fn reorder_channels(
        &self,
        actor: UserId,
        id: GuildId,
        moves: Vec<(ChannelId, ChannelMove)>,
    ) -> ChatResult<Vec<ChannelWithOverwrites>> {
        let c = self.ctx(actor, id).await?;
        let all = self.repo.list_guild_channels(id).await?;
        let changed = plan_channel_moves(&c.actor, id, &all, &moves)?;
        self.repo.update_channel_layout(&changed).await?;
        let mut out = Vec::with_capacity(changed.len());
        for ch in changed {
            let locked = moves
                .iter()
                .any(|(cid, m)| *cid == ch.id && m.lock_permissions);
            if locked {
                self.sync_with_parent(&ch).await?;
            }
            out.push(self.with_overwrites(ch).await?);
        }
        Ok(out)
    }

    pub async fn delete_channel(&self, actor: UserId, id: ChannelId) -> ChatResult<Channel> {
        let (channel, gid) = self.guild_channel(id).await?;
        let c = self.ctx(actor, gid).await?;
        channel_rules::check_delete_channel(&c.actor, &channel)?;
        self.repo.delete_channel(id).await?;
        Ok(channel)
    }

    /// Create or replace one overwrite (`PUT /channels/{id}/permissions/{target}`).
    pub async fn set_overwrite(
        &self,
        actor: UserId,
        channel: ChannelId,
        overwrite: Overwrite,
    ) -> ChatResult<ChannelWithOverwrites> {
        let (ch, gid) = self.guild_channel(channel).await?;
        let c = self.ctx(actor, gid).await?;
        let acc = self.access(actor, channel).await?;
        let role = match overwrite.kind {
            OverwriteKind::Role => Some(
                c.roles
                    .iter()
                    .find(|r| r.id.snowflake() == overwrite.id)
                    .ok_or(ChatError::UnknownRole)?,
            ),
            OverwriteKind::Member => {
                self.target_member(gid, UserId::from(overwrite.id)).await?;
                None
            }
        };
        channel_rules::check_overwrite_edit(
            &c.actor,
            acc.perms,
            overwrite.kind,
            role,
            overwrite.allow,
            overwrite.deny,
        )?;
        if overwrite.allow.is_empty() && overwrite.deny.is_empty() {
            self.repo
                .delete_overwrite(channel, overwrite.id, overwrite.kind)
                .await?;
        } else {
            self.repo.put_overwrite(channel, &overwrite).await?;
        }
        self.with_overwrites(ch).await
    }

    /// Remove an overwrite; idempotent when none exists.
    pub async fn delete_overwrite(
        &self,
        actor: UserId,
        channel: ChannelId,
        target: Snowflake,
    ) -> ChatResult<ChannelWithOverwrites> {
        let (ch, gid) = self.guild_channel(channel).await?;
        let c = self.ctx(actor, gid).await?;
        let acc = self.access(actor, channel).await?;
        let existing = self.repo.get_overwrites(channel).await?;
        if let Some(o) = existing.iter().find(|o| o.id == target) {
            let role = match o.kind {
                OverwriteKind::Role => c.roles.iter().find(|r| r.id.snowflake() == target),
                OverwriteKind::Member => None,
            };
            channel_rules::check_overwrite_edit(
                &c.actor,
                acc.perms,
                o.kind,
                role,
                Permissions::empty(),
                Permissions::empty(),
            )?;
            self.repo.delete_overwrite(channel, target, o.kind).await?;
        } else if !acc.perms.has(Permissions::MANAGE_ROLES) {
            return Err(ChatError::MissingPermissions);
        }
        self.with_overwrites(ch).await
    }

    // ---------------------------------------------------------- roles

    /// Roles ordered by position (members only).
    pub async fn list_roles(&self, actor: UserId, id: GuildId) -> ChatResult<Vec<Role>> {
        let mut roles = self.ctx(actor, id).await?.roles;
        roles.sort_by_key(|r| (r.position, r.id));
        Ok(roles)
    }

    pub async fn edit_role(
        &self,
        actor: UserId,
        id: GuildId,
        role: RoleId,
        patch: RolePatch,
    ) -> ChatResult<Role> {
        let c = self.ctx(actor, id).await?;
        let current = c
            .roles
            .iter()
            .find(|r| r.id == role)
            .ok_or(ChatError::UnknownRole)?;
        let updated = role_rules::plan_edit_role(&c.actor, current, patch)?;
        self.repo.update_role(&updated).await?;
        Ok(updated)
    }

    /// Returns the roles whose position changed.
    pub async fn reorder_roles(
        &self,
        actor: UserId,
        id: GuildId,
        moves: Vec<(RoleId, i32)>,
    ) -> ChatResult<Vec<Role>> {
        let c = self.ctx(actor, id).await?;
        let changed = plan_role_order(&c.actor, &c.roles, &moves)?;
        let positions: Vec<_> = changed.iter().map(|r| (r.id, r.position)).collect();
        self.repo.set_role_positions(id, &positions).await?;
        Ok(changed)
    }

    /// Add (`add == true`) or remove one role on a member.
    pub async fn assign_role(
        &self,
        actor: UserId,
        id: GuildId,
        user: UserId,
        role: RoleId,
        add: bool,
    ) -> ChatResult<Member> {
        let c = self.ctx(actor, id).await?;
        let r = c
            .roles
            .iter()
            .find(|r| r.id == role)
            .ok_or(ChatError::UnknownRole)?;
        role_rules::check_assign_role(&c.actor, r)?;
        let mut m = self.target_member(id, user).await?;
        if add {
            if !m.role_ids.contains(&role) {
                m.role_ids.push(role);
            }
        } else {
            m.role_ids.retain(|x| *x != role);
        }
        self.repo.update_member(&m).await?;
        Ok(m)
    }

    // -------------------------------------------------------- members

    fn clamp_page(limit: usize) -> usize {
        limit.clamp(1, MAX_PAGE)
    }

    pub async fn list_members(
        &self,
        actor: UserId,
        id: GuildId,
        after: Option<UserId>,
        limit: usize,
    ) -> ChatResult<Vec<Member>> {
        self.ctx(actor, id).await?;
        Ok(self
            .repo
            .list_members(id, after, Self::clamp_page(limit))
            .await?)
    }

    pub async fn get_member_view(
        &self,
        actor: UserId,
        id: GuildId,
        user: UserId,
    ) -> ChatResult<Member> {
        self.ctx(actor, id).await?;
        self.target_member(id, user).await
    }

    pub async fn edit_member(
        &self,
        actor: UserId,
        id: GuildId,
        user: UserId,
        patch: MemberPatch,
    ) -> ChatResult<Member> {
        let c = self.ctx(actor, id).await?;
        let target = self.target_member(id, user).await?;
        let t = MemberTarget {
            top_position: top_role_position(&target, &c.roles),
            perms: guild_permissions(&c.guild, &target, &c.roles),
            member: &target,
        };
        let updated = plan_edit_member(&c.guild, actor, &c.actor, &t, &c.roles, patch, self.now())?;
        self.repo.update_member(&updated).await?;
        Ok(updated)
    }

    pub async fn kick_member(&self, actor: UserId, id: GuildId, user: UserId) -> ChatResult<()> {
        let c = self.ctx(actor, id).await?;
        let target = self.target_member(id, user).await?;
        check_moderation(
            &c.guild,
            ModAction::Kick,
            actor,
            &c.actor,
            &target,
            top_role_position(&target, &c.roles),
            guild_permissions(&c.guild, &target, &c.roles),
        )?;
        self.repo.remove_member(id, user).await?;
        Ok(())
    }

    // ----------------------------------------------------------- bans

    pub async fn list_bans(
        &self,
        actor: UserId,
        id: GuildId,
        before: Option<UserId>,
        after: Option<UserId>,
        limit: usize,
    ) -> ChatResult<Vec<Ban>> {
        let c = self.ctx(actor, id).await?;
        if !c.actor.perms.has(Permissions::BAN_MEMBERS) {
            return Err(ChatError::MissingPermissions);
        }
        Ok(self
            .repo
            .list_bans(id, before, after, Self::clamp_page(limit))
            .await?)
    }

    pub async fn get_ban(&self, actor: UserId, id: GuildId, user: UserId) -> ChatResult<Ban> {
        let c = self.ctx(actor, id).await?;
        if !c.actor.perms.has(Permissions::BAN_MEMBERS) {
            return Err(ChatError::MissingPermissions);
        }
        self.repo
            .get_ban(id, user)
            .await?
            .ok_or(ChatError::UnknownBan)
    }

    /// Ban a user (member or not) and remove their membership. Returns whether
    /// the user was a member.
    pub async fn ban_member(
        &self,
        actor: UserId,
        id: GuildId,
        user: UserId,
        reason: Option<String>,
    ) -> ChatResult<(Ban, bool)> {
        let out = self.ban_member_purging(actor, id, user, reason, 0).await?;
        Ok((out.ban, out.was_member))
    }

    /// [`Self::ban_member`] that also bulk-hides the user's messages from the last
    /// `delete_message_seconds` (open question 158, D-121). The hidden ids per
    /// channel come back for the caller's `MESSAGE_DELETE_BULK` dispatch.
    pub async fn ban_member_purging(
        &self,
        actor: UserId,
        id: GuildId,
        user: UserId,
        reason: Option<String>,
        delete_message_seconds: u32,
    ) -> ChatResult<BanOutcome> {
        let c = self.ctx(actor, id).await?;
        let reason = admin_rules::validate_ban_reason(reason)?;
        let was_member = match self.repo.get_member(id, user).await? {
            Some(target) => {
                check_moderation(
                    &c.guild,
                    ModAction::Ban,
                    actor,
                    &c.actor,
                    &target,
                    top_role_position(&target, &c.roles),
                    guild_permissions(&c.guild, &target, &c.roles),
                )?;
                true
            }
            None => {
                admin_rules::check_ban_non_member(&c.guild, actor, &c.actor, user)?;
                false
            }
        };
        let ban = Ban {
            guild_id: id,
            user_id: user,
            reason,
            banned_by: Some(actor),
            created_at_ms: self.now(),
        };
        self.repo.insert_ban(&ban).await?;
        let mut purged = Vec::new();
        if delete_message_seconds > 0 {
            let channels: Vec<ChannelId> = self
                .repo
                .list_guild_channels(id)
                .await?
                .into_iter()
                .map(|ch| ch.id)
                .collect();
            let now = self.now();
            let since = now - i64::from(delete_message_seconds) * 1000;
            purged = self
                .repo
                .hide_user_messages(
                    user,
                    &channels,
                    since,
                    now,
                    now + self.limits.purge_after_ms,
                )
                .await?;
        }
        Ok(BanOutcome {
            ban,
            was_member,
            purged,
        })
    }

    pub async fn unban(&self, actor: UserId, id: GuildId, user: UserId) -> ChatResult<()> {
        let c = self.ctx(actor, id).await?;
        if !c.actor.perms.has(Permissions::BAN_MEMBERS) {
            return Err(ChatError::MissingPermissions);
        }
        if !self.repo.delete_ban(id, user).await? {
            return Err(ChatError::UnknownBan);
        }
        Ok(())
    }
}

/// Result of [`ChatService::ban_member_purging`].
#[derive(Debug, Clone)]
pub struct BanOutcome {
    pub ban: Ban,
    pub was_member: bool,
    /// Hidden message ids per channel (empty unless messages were purged).
    pub purged: Vec<(ChannelId, Vec<MessageId>)>,
}

impl<R: ChatRepository> ChatService<R> {
    /// Display fields for member, ban and message objects (open question 157).
    pub async fn user_summaries(
        &self,
        ids: &[UserId],
    ) -> ChatResult<Vec<crate::repo::UserSummary>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self.repo.users(ids).await?)
    }
}
