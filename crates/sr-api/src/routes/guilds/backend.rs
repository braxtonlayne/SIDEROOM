//! The seam between the guild routes and `sr-chat`.
//!
//! `ChatService<R>` is generic over its repository while `router()` cannot be,
//! so handlers talk to the object-safe [`GuildBackend`]. [`ChatBackend`] is the
//! one implementation: it calls the service (which enforces permissions) and
//! then publishes the matching [`DomainEvent`] on the bus. The composition root
//! builds it with [`backend`] and installs it with [`super::with_backend`].

use super::dto::{ChannelObject, GuildObject, MemberObject, RoleObject, UserRef};
use async_trait::async_trait;
use serde::Serialize;
use serde_json::{Value, json};
use sr_chat::admin_repo::{Ban, GuildAdminRepository};
use sr_chat::admin_rules::{ChannelMove, GuildPatch, MemberPatch};
use sr_chat::channel_rules::{ChannelPatch, NewChannel};
use sr_chat::guild_rules::GuildBlueprint;
use sr_chat::repo::ChatRepository;
use sr_chat::role_rules::{NewRole, RolePatch};
use sr_chat::{ChannelWithOverwrites, ChatResult, ChatService, Guild, Member, Role};
use sr_core::permissions::{Overwrite, Permissions};
use sr_core::{ChannelId, GuildId, RoleId, Snowflake, UserId};
use sr_events::{DomainEvent, EventBus};
use std::sync::Arc;

/// Everything the guild routes need, as one object-safe trait. The acting user
/// is always the first argument; all permission checks happen behind it.
#[async_trait]
pub trait GuildBackend: Send + Sync {
    // guilds
    async fn create_guild(&self, owner: UserId, name: &str) -> ChatResult<GuildBlueprint>;
    async fn get_guild(&self, actor: UserId, id: GuildId) -> ChatResult<(Guild, Vec<Role>)>;
    async fn list_my_guilds(&self, user: UserId) -> ChatResult<Vec<(Guild, Permissions)>>;
    async fn edit_guild(&self, actor: UserId, id: GuildId, patch: GuildPatch) -> ChatResult<Guild>;
    async fn delete_guild(&self, actor: UserId, id: GuildId) -> ChatResult<()>;
    async fn leave_guild(&self, user: UserId, id: GuildId) -> ChatResult<()>;
    // channels
    async fn list_channels(
        &self,
        actor: UserId,
        guild: GuildId,
    ) -> ChatResult<Vec<ChannelWithOverwrites>>;
    async fn get_channel(&self, actor: UserId, id: ChannelId) -> ChatResult<ChannelWithOverwrites>;
    async fn create_channel(
        &self,
        actor: UserId,
        guild: GuildId,
        input: NewChannel,
        overwrites: Vec<Overwrite>,
    ) -> ChatResult<ChannelWithOverwrites>;
    async fn edit_channel(
        &self,
        actor: UserId,
        id: ChannelId,
        patch: ChannelPatch,
        mv: ChannelMove,
    ) -> ChatResult<ChannelWithOverwrites>;
    async fn delete_channel(
        &self,
        actor: UserId,
        id: ChannelId,
    ) -> ChatResult<ChannelWithOverwrites>;
    async fn reorder_channels(
        &self,
        actor: UserId,
        guild: GuildId,
        moves: Vec<(ChannelId, ChannelMove)>,
    ) -> ChatResult<()>;
    async fn set_overwrite(
        &self,
        actor: UserId,
        channel: ChannelId,
        overwrite: Overwrite,
    ) -> ChatResult<()>;
    async fn delete_overwrite(
        &self,
        actor: UserId,
        channel: ChannelId,
        target: Snowflake,
    ) -> ChatResult<()>;
    // roles
    async fn list_roles(&self, actor: UserId, guild: GuildId) -> ChatResult<Vec<Role>>;
    async fn create_role(&self, actor: UserId, guild: GuildId, input: NewRole) -> ChatResult<Role>;
    async fn edit_role(
        &self,
        actor: UserId,
        guild: GuildId,
        role: RoleId,
        patch: RolePatch,
    ) -> ChatResult<Role>;
    async fn reorder_roles(
        &self,
        actor: UserId,
        guild: GuildId,
        moves: Vec<(RoleId, i32)>,
    ) -> ChatResult<Vec<Role>>;
    async fn delete_role(&self, actor: UserId, guild: GuildId, role: RoleId) -> ChatResult<()>;
    async fn assign_role(
        &self,
        actor: UserId,
        guild: GuildId,
        user: UserId,
        role: RoleId,
        add: bool,
    ) -> ChatResult<()>;
    // members
    async fn list_members(
        &self,
        actor: UserId,
        guild: GuildId,
        after: Option<UserId>,
        limit: usize,
    ) -> ChatResult<Vec<Member>>;
    async fn get_member(&self, actor: UserId, guild: GuildId, user: UserId) -> ChatResult<Member>;
    async fn edit_member(
        &self,
        actor: UserId,
        guild: GuildId,
        user: UserId,
        patch: MemberPatch,
    ) -> ChatResult<Member>;
    async fn kick_member(&self, actor: UserId, guild: GuildId, user: UserId) -> ChatResult<()>;
    // bans
    async fn list_bans(
        &self,
        actor: UserId,
        guild: GuildId,
        before: Option<UserId>,
        after: Option<UserId>,
        limit: usize,
    ) -> ChatResult<Vec<Ban>>;
    async fn get_ban(&self, actor: UserId, guild: GuildId, user: UserId) -> ChatResult<Ban>;
    async fn ban_member(
        &self,
        actor: UserId,
        guild: GuildId,
        user: UserId,
        reason: Option<String>,
    ) -> ChatResult<()>;
    async fn unban(&self, actor: UserId, guild: GuildId, user: UserId) -> ChatResult<()>;
}

/// [`GuildBackend`] over a [`ChatService`] and an [`EventBus`].
pub struct ChatBackend<R> {
    svc: ChatService<R>,
    bus: Arc<dyn EventBus>,
}

/// Erase the repository type for installation with [`super::with_backend`].
pub fn backend<R>(svc: ChatService<R>, bus: Arc<dyn EventBus>) -> Arc<dyn GuildBackend>
where
    R: ChatRepository + GuildAdminRepository + 'static,
{
    Arc::new(ChatBackend { svc, bus })
}

fn val<T: Serialize>(t: &T) -> Value {
    serde_json::to_value(t).unwrap_or(Value::Null)
}

fn role_payload(guild: GuildId, role: &Role) -> Value {
    json!({ "guild_id": guild.to_string(), "role": val(&RoleObject::from(role)) })
}

impl<R: ChatRepository + GuildAdminRepository> ChatBackend<R> {
    /// Publish to every topic of the event. A bus failure never fails the
    /// request: the change is already committed.
    async fn emit(&self, ev: DomainEvent) {
        for topic in ev.topics() {
            if let Err(e) = self.bus.publish(topic, ev.clone()).await {
                tracing::warn!(error = %e, "failed to publish domain event");
            }
        }
    }

    async fn emit_channel_updated(&self, c: &ChannelWithOverwrites) {
        if let Some(guild) = c.channel.guild_id {
            self.emit(DomainEvent::ChannelUpdated {
                guild,
                channel: c.channel.id,
                payload: val(&ChannelObject::from(c)),
            })
            .await;
        }
    }

    async fn emit_member_updated(&self, m: &Member) {
        self.emit(DomainEvent::MemberUpdated {
            guild: m.guild_id,
            user: m.user_id,
            payload: val(&MemberObject::new(m, true)),
        })
        .await;
    }

    async fn emit_member_left(&self, guild: GuildId, user: UserId) {
        self.emit(DomainEvent::MemberLeft { guild, user }).await;
    }
}

#[async_trait]
impl<R: ChatRepository + GuildAdminRepository + 'static> GuildBackend for ChatBackend<R> {
    async fn create_guild(&self, owner: UserId, name: &str) -> ChatResult<GuildBlueprint> {
        let bp = self.svc.create_guild(owner, name).await?;
        self.emit(DomainEvent::GuildCreated {
            guild: bp.guild.id,
            owner,
        })
        .await;
        Ok(bp)
    }

    async fn get_guild(&self, actor: UserId, id: GuildId) -> ChatResult<(Guild, Vec<Role>)> {
        self.svc.get_guild(actor, id).await
    }

    async fn list_my_guilds(&self, user: UserId) -> ChatResult<Vec<(Guild, Permissions)>> {
        self.svc.list_my_guilds(user).await
    }

    async fn edit_guild(&self, actor: UserId, id: GuildId, patch: GuildPatch) -> ChatResult<Guild> {
        let g = self.svc.edit_guild(actor, id, patch).await?;
        let roles = self.svc.list_roles(actor, id).await.unwrap_or_default();
        self.emit(DomainEvent::GuildUpdated {
            guild: id,
            payload: val(&GuildObject::new(&g, Some(&roles), None)),
        })
        .await;
        Ok(g)
    }

    async fn delete_guild(&self, actor: UserId, id: GuildId) -> ChatResult<()> {
        self.svc.delete_guild(actor, id).await?;
        self.emit(DomainEvent::GuildDeleted { guild: id }).await;
        Ok(())
    }

    async fn leave_guild(&self, user: UserId, id: GuildId) -> ChatResult<()> {
        self.svc.leave_guild(user, id).await?;
        self.emit_member_left(id, user).await;
        Ok(())
    }

    async fn list_channels(
        &self,
        actor: UserId,
        guild: GuildId,
    ) -> ChatResult<Vec<ChannelWithOverwrites>> {
        self.svc.list_channels(actor, guild).await
    }

    async fn get_channel(&self, actor: UserId, id: ChannelId) -> ChatResult<ChannelWithOverwrites> {
        self.svc.get_channel_view(actor, id).await
    }

    async fn create_channel(
        &self,
        actor: UserId,
        guild: GuildId,
        input: NewChannel,
        overwrites: Vec<Overwrite>,
    ) -> ChatResult<ChannelWithOverwrites> {
        let ch = self.svc.create_channel(actor, guild, input).await?;
        let mut view = ChannelWithOverwrites {
            channel: ch,
            overwrites: Vec::new(),
        };
        for o in overwrites {
            view = match self.svc.set_overwrite(actor, view.channel.id, o).await {
                Ok(v) => v,
                Err(e) => {
                    // Do not leave a half-configured (possibly world-visible) channel behind.
                    let _ = self.svc.delete_channel(actor, view.channel.id).await;
                    return Err(e);
                }
            };
        }
        self.emit(DomainEvent::ChannelCreatedFull {
            guild,
            channel: view.channel.id,
            payload: val(&ChannelObject::from(&view)),
        })
        .await;
        Ok(view)
    }

    async fn edit_channel(
        &self,
        actor: UserId,
        id: ChannelId,
        patch: ChannelPatch,
        mv: ChannelMove,
    ) -> ChatResult<ChannelWithOverwrites> {
        let view = self.svc.edit_channel(actor, id, patch, mv).await?;
        self.emit_channel_updated(&view).await;
        Ok(view)
    }

    async fn delete_channel(
        &self,
        actor: UserId,
        id: ChannelId,
    ) -> ChatResult<ChannelWithOverwrites> {
        let before = self.svc.get_channel_view(actor, id).await.ok();
        let ch = self.svc.delete_channel(actor, id).await?;
        let view = ChannelWithOverwrites {
            overwrites: before.map(|b| b.overwrites).unwrap_or_default(),
            channel: ch,
        };
        if let Some(guild) = view.channel.guild_id {
            self.emit(DomainEvent::ChannelDeleted {
                guild,
                channel: id,
                payload: val(&ChannelObject::from(&view)),
            })
            .await;
        }
        Ok(view)
    }

    async fn reorder_channels(
        &self,
        actor: UserId,
        guild: GuildId,
        moves: Vec<(ChannelId, ChannelMove)>,
    ) -> ChatResult<()> {
        for c in self.svc.reorder_channels(actor, guild, moves).await? {
            self.emit_channel_updated(&c).await;
        }
        Ok(())
    }

    async fn set_overwrite(
        &self,
        actor: UserId,
        channel: ChannelId,
        overwrite: Overwrite,
    ) -> ChatResult<()> {
        let view = self.svc.set_overwrite(actor, channel, overwrite).await?;
        self.emit_channel_updated(&view).await;
        Ok(())
    }

    async fn delete_overwrite(
        &self,
        actor: UserId,
        channel: ChannelId,
        target: Snowflake,
    ) -> ChatResult<()> {
        let view = self.svc.delete_overwrite(actor, channel, target).await?;
        self.emit_channel_updated(&view).await;
        Ok(())
    }

    async fn list_roles(&self, actor: UserId, guild: GuildId) -> ChatResult<Vec<Role>> {
        self.svc.list_roles(actor, guild).await
    }

    async fn create_role(&self, actor: UserId, guild: GuildId, input: NewRole) -> ChatResult<Role> {
        let role = self.svc.create_role(actor, guild, input).await?;
        self.emit(DomainEvent::RoleCreated {
            guild,
            role: role.id,
            payload: role_payload(guild, &role),
        })
        .await;
        Ok(role)
    }

    async fn edit_role(
        &self,
        actor: UserId,
        guild: GuildId,
        role: RoleId,
        patch: RolePatch,
    ) -> ChatResult<Role> {
        let r = self.svc.edit_role(actor, guild, role, patch).await?;
        self.emit(DomainEvent::RoleUpdated {
            guild,
            role,
            payload: role_payload(guild, &r),
        })
        .await;
        Ok(r)
    }

    async fn reorder_roles(
        &self,
        actor: UserId,
        guild: GuildId,
        moves: Vec<(RoleId, i32)>,
    ) -> ChatResult<Vec<Role>> {
        let changed = self.svc.reorder_roles(actor, guild, moves).await?;
        for r in &changed {
            self.emit(DomainEvent::RoleUpdated {
                guild,
                role: r.id,
                payload: role_payload(guild, r),
            })
            .await;
        }
        self.svc.list_roles(actor, guild).await
    }

    async fn delete_role(&self, actor: UserId, guild: GuildId, role: RoleId) -> ChatResult<()> {
        self.svc.delete_role(actor, guild, role).await?;
        self.emit(DomainEvent::RoleDeleted {
            guild,
            role,
            payload: json!({ "guild_id": guild.to_string(), "role_id": role.to_string() }),
        })
        .await;
        Ok(())
    }

    async fn assign_role(
        &self,
        actor: UserId,
        guild: GuildId,
        user: UserId,
        role: RoleId,
        add: bool,
    ) -> ChatResult<()> {
        let m = self.svc.assign_role(actor, guild, user, role, add).await?;
        self.emit_member_updated(&m).await;
        Ok(())
    }

    async fn list_members(
        &self,
        actor: UserId,
        guild: GuildId,
        after: Option<UserId>,
        limit: usize,
    ) -> ChatResult<Vec<Member>> {
        self.svc.list_members(actor, guild, after, limit).await
    }

    async fn get_member(&self, actor: UserId, guild: GuildId, user: UserId) -> ChatResult<Member> {
        self.svc.get_member_view(actor, guild, user).await
    }

    async fn edit_member(
        &self,
        actor: UserId,
        guild: GuildId,
        user: UserId,
        patch: MemberPatch,
    ) -> ChatResult<Member> {
        let m = self.svc.edit_member(actor, guild, user, patch).await?;
        self.emit_member_updated(&m).await;
        Ok(m)
    }

    async fn kick_member(&self, actor: UserId, guild: GuildId, user: UserId) -> ChatResult<()> {
        self.svc.kick_member(actor, guild, user).await?;
        self.emit_member_left(guild, user).await;
        Ok(())
    }

    async fn list_bans(
        &self,
        actor: UserId,
        guild: GuildId,
        before: Option<UserId>,
        after: Option<UserId>,
        limit: usize,
    ) -> ChatResult<Vec<Ban>> {
        self.svc.list_bans(actor, guild, before, after, limit).await
    }

    async fn get_ban(&self, actor: UserId, guild: GuildId, user: UserId) -> ChatResult<Ban> {
        self.svc.get_ban(actor, guild, user).await
    }

    async fn ban_member(
        &self,
        actor: UserId,
        guild: GuildId,
        user: UserId,
        reason: Option<String>,
    ) -> ChatResult<()> {
        let (ban, was_member) = self.svc.ban_member(actor, guild, user, reason).await?;
        if was_member {
            self.emit_member_left(guild, user).await;
        }
        self.emit(DomainEvent::MemberBanned {
            guild,
            user,
            payload: json!({ "guild_id": guild.to_string(), "user": val(&UserRef::new(ban.user_id)) }),
        })
        .await;
        Ok(())
    }

    async fn unban(&self, actor: UserId, guild: GuildId, user: UserId) -> ChatResult<()> {
        self.svc.unban(actor, guild, user).await?;
        self.emit(DomainEvent::MemberUnbanned {
            guild,
            user,
            payload: json!({ "guild_id": guild.to_string(), "user": val(&UserRef::new(user)) }),
        })
        .await;
        Ok(())
    }
}
