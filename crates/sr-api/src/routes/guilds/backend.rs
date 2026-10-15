//! The seam between the guild routes and `sr-chat`.
//!
//! `ChatService<R>` is generic over its repository while `router()` cannot be,
//! so handlers talk to the object-safe [`GuildBackend`]. [`ChatBackend`] is the
//! one implementation: it calls the service (which enforces permissions) and
//! then publishes the matching [`DomainEvent`] on the bus. The composition root
//! builds it with [`backend`] and registers it as a [`super::GuildApi`] extension.

use super::dto::{ChannelObject, GuildObject, MemberObject, RoleObject, UserRef};
use async_trait::async_trait;
use serde::Serialize;
use serde_json::{Value, json};
use sr_chat::admin_repo::{Ban, GuildAdminRepository, MemberFilter};
use sr_chat::admin_rules::{ChannelMove, GuildPatch, MemberPatch};
use sr_chat::channel_rules::{ChannelPatch, NewChannel};
use sr_chat::guild_rules::GuildBlueprint;
use sr_chat::repo::{ChatRepository, UserSummary};
use sr_chat::role_rules::{NewRole, RolePatch};
use sr_chat::settings::SettingsService;
use sr_chat::settings::audit::{AuditAction, AuditChange};
use sr_chat::settings::repo::SettingsRepository;
use sr_chat::settings::templates::TemplateSnapshot;
use sr_chat::{ChannelWithOverwrites, ChatResult, ChatService, Guild, Member, Role};
use sr_core::permissions::{Overwrite, Permissions};
use sr_core::{ChannelId, GuildId, RoleId, Snowflake, UserId};
use sr_events::{DomainEvent, EventBus};
use std::collections::HashMap;
use std::sync::Arc;

/// Everything the guild routes need, as one object-safe trait. The acting user
/// is always the first argument; all permission checks happen behind it.
#[async_trait]
pub trait GuildBackend: Send + Sync {
    // guilds
    async fn create_guild(&self, owner: UserId, name: &str) -> ChatResult<GuildBlueprint>;
    /// Create a guild from a template snapshot in one transaction; one `GUILD_CREATE`.
    async fn create_guild_from_snapshot(
        &self,
        owner: UserId,
        name: &str,
        snapshot: &TemplateSnapshot,
    ) -> ChatResult<GuildBlueprint>;
    async fn get_guild(&self, actor: UserId, id: GuildId) -> ChatResult<(Guild, Vec<Role>)>;
    async fn list_my_guilds(&self, user: UserId) -> ChatResult<Vec<(Guild, Permissions)>>;
    async fn edit_guild(&self, actor: UserId, id: GuildId, patch: GuildPatch) -> ChatResult<Guild>;
    async fn delete_guild(&self, actor: UserId, id: GuildId) -> ChatResult<()>;
    /// The vanity invite code (`MANAGE_GUILD`).
    async fn vanity_code(&self, actor: UserId, id: GuildId) -> ChatResult<Option<String>>;
    /// Claim, change or release (`None`) the vanity invite code (`MANAGE_GUILD`); emits
    /// `GUILD_UPDATE`.
    async fn set_vanity_code(
        &self,
        actor: UserId,
        id: GuildId,
        code: Option<String>,
    ) -> ChatResult<Guild>;
    /// Agree to the membership screening (LNG-GLD-008); emits `GUILD_MEMBER_UPDATE`.
    async fn accept_screening(&self, user: UserId, guild: GuildId) -> ChatResult<Member>;
    /// Grant the roles chosen through onboarding answers; returns the roles newly added.
    async fn apply_onboarding_roles(
        &self,
        user: UserId,
        guild: GuildId,
        roles: &[RoleId],
    ) -> ChatResult<Vec<RoleId>>;
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
        filter: MemberFilter,
        after: Option<UserId>,
        limit: usize,
    ) -> ChatResult<Vec<Member>>;
    async fn list_role_members(
        &self,
        actor: UserId,
        guild: GuildId,
        role: RoleId,
        after: Option<UserId>,
        limit: usize,
    ) -> ChatResult<Vec<Member>>;
    /// How many members a prune would remove.
    async fn prune_preview(
        &self,
        actor: UserId,
        guild: GuildId,
        days: u32,
        include_roles: Vec<RoleId>,
    ) -> ChatResult<usize>;
    /// Remove inactive members; returns how many were removed.
    async fn prune(
        &self,
        actor: UserId,
        guild: GuildId,
        days: u32,
        include_roles: Vec<RoleId>,
    ) -> ChatResult<usize>;
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
        delete_message_seconds: u32,
    ) -> ChatResult<()>;
    // users
    async fn user_summaries(&self, ids: &[UserId]) -> ChatResult<Vec<UserSummary>>;
    async fn unban(&self, actor: UserId, guild: GuildId, user: UserId) -> ChatResult<()>;
}

/// Where guild mutations are recorded in the guild audit log (open questions 133
/// and 158). Object-safe so the backend need not know the settings repository.
#[async_trait]
pub trait AuditSink: Send + Sync {
    async fn record(
        &self,
        guild: GuildId,
        actor: UserId,
        action: AuditAction,
        target_id: Option<u64>,
        changes: Vec<AuditChange>,
        reason: Option<&str>,
    );

    /// Like [`Self::record`], with Discord's per-action `options` (`channel_id`,
    /// `count`, ...). Sinks that cannot store options drop them.
    async fn record_with_options(
        &self,
        guild: GuildId,
        actor: UserId,
        action: AuditAction,
        target_id: Option<u64>,
        options: serde_json::Value,
    ) {
        let _ = options;
        self.record(guild, actor, action, target_id, Vec::new(), None)
            .await;
    }
}

#[async_trait]
impl<R: SettingsRepository + 'static> AuditSink for SettingsService<R> {
    async fn record(
        &self,
        guild: GuildId,
        actor: UserId,
        action: AuditAction,
        target_id: Option<u64>,
        changes: Vec<AuditChange>,
        reason: Option<&str>,
    ) {
        // The change is already committed; a failed audit write is logged, not surfaced.
        // D-187: inside a built-in module's handler the entry names the system user and the module.
        let written = match sr_features::current_module() {
            Some((id, name)) => {
                self.record_audit_as_module(guild, action, target_id, changes, reason, (&id, &name))
                    .await
            }
            None => {
                self.record_audit(guild, actor, action, target_id, changes, reason)
                    .await
            }
        };
        if let Err(e) = written {
            tracing::warn!(error = %e, ?action, "failed to record audit entry");
        }
    }

    async fn record_with_options(
        &self,
        guild: GuildId,
        actor: UserId,
        action: AuditAction,
        target_id: Option<u64>,
        mut options: serde_json::Value,
    ) {
        let mut actor = actor;
        if let Some((id, name)) = sr_features::current_module() {
            // D-187: module writes act as the system user and name the module.
            actor = UserId::new(sr_chat::settings::audit::SYSTEM_USER_ID);
            if let Some(o) = options.as_object_mut() {
                o.insert("module".into(), name.into());
                o.insert("module_id".into(), id.into());
            }
        }
        if let Err(e) = self
            .record_audit_with_options(guild, actor, action, target_id, options)
            .await
        {
            tracing::warn!(error = %e, ?action, "failed to record audit entry");
        }
    }
}

/// [`GuildBackend`] over a [`ChatService`] and an [`EventBus`].
pub struct ChatBackend<R> {
    svc: ChatService<R>,
    bus: Arc<dyn EventBus>,
    audit: Option<Arc<dyn AuditSink>>,
}

/// Erase the repository type for registration as a [`super::GuildApi`].
pub fn backend<R>(svc: ChatService<R>, bus: Arc<dyn EventBus>) -> Arc<dyn GuildBackend>
where
    R: ChatRepository + GuildAdminRepository + 'static,
{
    Arc::new(ChatBackend {
        svc,
        bus,
        audit: None,
    })
}

/// Like [`backend`], and every guild mutation is also written to the audit log.
pub fn backend_with_audit<R>(
    svc: ChatService<R>,
    bus: Arc<dyn EventBus>,
    audit: Arc<dyn AuditSink>,
) -> Arc<dyn GuildBackend>
where
    R: ChatRepository + GuildAdminRepository + 'static,
{
    Arc::new(ChatBackend {
        svc,
        bus,
        audit: Some(audit),
    })
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

    async fn audit(
        &self,
        guild: GuildId,
        actor: UserId,
        action: AuditAction,
        target: u64,
        reason: Option<&str>,
    ) {
        if let Some(a) = &self.audit {
            a.record(guild, actor, action, Some(target), Vec::new(), reason)
                .await;
        }
    }

    /// Display fields for event payloads; a failed lookup degrades to bare ids.
    async fn users_of(&self, ids: &[UserId]) -> HashMap<UserId, UserSummary> {
        match self.svc.user_summaries(ids).await {
            Ok(v) => v.into_iter().map(|u| (u.id, u)).collect(),
            Err(e) => {
                tracing::warn!(error = %e, "user lookup for an event payload failed");
                HashMap::new()
            }
        }
    }

    async fn emit_member_updated(&self, m: &Member) {
        self.emit(DomainEvent::MemberUpdated {
            guild: m.guild_id,
            user: m.user_id,
            payload: val(&MemberObject::new(
                m,
                true,
                &self.users_of(&[m.user_id]).await,
            )),
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

    async fn create_guild_from_snapshot(
        &self,
        owner: UserId,
        name: &str,
        snapshot: &TemplateSnapshot,
    ) -> ChatResult<GuildBlueprint> {
        let bp = self
            .svc
            .create_guild_from_snapshot(owner, name, snapshot)
            .await?;
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
        self.audit(id, actor, AuditAction::GuildUpdate, id.get(), None)
            .await;
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

    async fn vanity_code(&self, actor: UserId, id: GuildId) -> ChatResult<Option<String>> {
        self.svc.vanity_code(actor, id).await
    }

    async fn set_vanity_code(
        &self,
        actor: UserId,
        id: GuildId,
        code: Option<String>,
    ) -> ChatResult<Guild> {
        let g = self.svc.set_vanity_code(actor, id, code.as_deref()).await?;
        self.audit(id, actor, AuditAction::GuildUpdate, id.get(), None)
            .await;
        let roles = self.svc.list_roles(actor, id).await.unwrap_or_default();
        self.emit(DomainEvent::GuildUpdated {
            guild: id,
            payload: val(&GuildObject::new(&g, Some(&roles), None)),
        })
        .await;
        Ok(g)
    }

    async fn accept_screening(&self, user: UserId, guild: GuildId) -> ChatResult<Member> {
        let before = self.svc.get_member_view(user, guild, user).await?;
        let m = self.svc.accept_screening(user, guild).await?;
        if before.pending {
            self.emit_member_updated(&m).await;
        }
        Ok(m)
    }

    async fn apply_onboarding_roles(
        &self,
        user: UserId,
        guild: GuildId,
        roles: &[RoleId],
    ) -> ChatResult<Vec<RoleId>> {
        let added = self.svc.apply_onboarding_roles(user, guild, roles).await?;
        if !added.is_empty() {
            let m = self.svc.get_member_view(user, guild, user).await?;
            self.emit_member_updated(&m).await;
        }
        Ok(added)
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
        self.emit(DomainEvent::ChannelCreated {
            guild,
            channel: view.channel.id,
            payload: val(&ChannelObject::from(&view)),
        })
        .await;
        self.audit(
            guild,
            actor,
            AuditAction::ChannelCreate,
            view.channel.id.get(),
            None,
        )
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
        if let Some(g) = view.channel.guild_id {
            self.audit(g, actor, AuditAction::ChannelUpdate, id.get(), None)
                .await;
        }
        Ok(view)
    }

    async fn delete_channel(
        &self,
        actor: UserId,
        id: ChannelId,
    ) -> ChatResult<ChannelWithOverwrites> {
        let before = self.svc.get_channel_view(actor, id).await.ok();
        // The viewer set must be computed while the channel still exists (Q159).
        let viewers = self.svc.channel_viewers(id).await.unwrap_or_default();
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
                viewers,
            })
            .await;
            self.audit(guild, actor, AuditAction::ChannelDelete, id.get(), None)
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
            self.audit(
                guild,
                actor,
                AuditAction::ChannelUpdate,
                c.channel.id.get(),
                None,
            )
            .await;
        }
        Ok(())
    }

    async fn set_overwrite(
        &self,
        actor: UserId,
        channel: ChannelId,
        overwrite: Overwrite,
    ) -> ChatResult<()> {
        let target = overwrite.id;
        let kind = overwrite.kind;
        let cleared = overwrite.allow.is_empty() && overwrite.deny.is_empty();
        // Discord logs a new overwrite as CREATE (13) and a changed one as UPDATE (14); an
        // empty one removes the row, which is a DELETE (15).
        let existed = self
            .svc
            .get_channel_view(actor, channel)
            .await
            .map(|v| {
                v.overwrites
                    .iter()
                    .any(|o| o.id == target && o.kind == kind)
            })
            .unwrap_or(false);
        let view = self.svc.set_overwrite(actor, channel, overwrite).await?;
        self.emit_channel_updated(&view).await;
        let action = match (existed, cleared) {
            (false, true) => None,
            (true, true) => Some(AuditAction::ChannelOverwriteDelete),
            (false, false) => Some(AuditAction::ChannelOverwriteCreate),
            (true, false) => Some(AuditAction::ChannelOverwriteUpdate),
        };
        if let (Some(g), Some(action)) = (view.channel.guild_id, action) {
            self.audit(g, actor, action, target.get(), None).await;
        }
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
        if let Some(g) = view.channel.guild_id {
            self.audit(
                g,
                actor,
                AuditAction::ChannelOverwriteDelete,
                target.get(),
                None,
            )
            .await;
        }
        Ok(())
    }

    async fn list_roles(&self, actor: UserId, guild: GuildId) -> ChatResult<Vec<Role>> {
        self.svc.list_roles(actor, guild).await
    }

    async fn create_role(&self, actor: UserId, guild: GuildId, input: NewRole) -> ChatResult<Role> {
        let before = self.svc.list_roles(actor, guild).await.unwrap_or_default();
        let role = self.svc.create_role(actor, guild, input).await?;
        self.emit(DomainEvent::RoleCreated {
            guild,
            role: role.id,
            payload: role_payload(guild, &role),
        })
        .await;
        // Creating a role shifts the roles at or above it up one position; each of
        // them gets a GUILD_ROLE_UPDATE (open question 159).
        for r in self.svc.list_roles(actor, guild).await.unwrap_or_default() {
            let moved = before
                .iter()
                .any(|o| o.id == r.id && o.position != r.position);
            if moved && r.id != role.id {
                self.emit(DomainEvent::RoleUpdated {
                    guild,
                    role: r.id,
                    payload: role_payload(guild, &r),
                })
                .await;
            }
        }
        self.audit(guild, actor, AuditAction::RoleCreate, role.id.get(), None)
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
        self.audit(guild, actor, AuditAction::RoleUpdate, role.get(), None)
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
            self.audit(guild, actor, AuditAction::RoleUpdate, r.id.get(), None)
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
        self.audit(guild, actor, AuditAction::RoleDelete, role.get(), None)
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
        self.audit(
            guild,
            actor,
            AuditAction::MemberRoleUpdate,
            user.get(),
            None,
        )
        .await;
        Ok(())
    }

    async fn list_members(
        &self,
        actor: UserId,
        guild: GuildId,
        filter: MemberFilter,
        after: Option<UserId>,
        limit: usize,
    ) -> ChatResult<Vec<Member>> {
        self.svc
            .list_members(actor, guild, filter, after, limit)
            .await
    }

    async fn list_role_members(
        &self,
        actor: UserId,
        guild: GuildId,
        role: RoleId,
        after: Option<UserId>,
        limit: usize,
    ) -> ChatResult<Vec<Member>> {
        self.svc
            .list_role_members(actor, guild, role, after, limit)
            .await
    }

    async fn prune_preview(
        &self,
        actor: UserId,
        guild: GuildId,
        days: u32,
        include_roles: Vec<RoleId>,
    ) -> ChatResult<usize> {
        self.svc
            .prune_preview(actor, guild, days, &include_roles)
            .await
    }

    async fn prune(
        &self,
        actor: UserId,
        guild: GuildId,
        days: u32,
        include_roles: Vec<RoleId>,
    ) -> ChatResult<usize> {
        let removed = self.svc.prune(actor, guild, days, &include_roles).await?;
        for user in &removed {
            self.emit_member_left(guild, *user).await;
        }
        if let Some(a) = self.audit.as_ref().filter(|_| !removed.is_empty()) {
            a.record(
                guild,
                actor,
                AuditAction::MemberPrune,
                None,
                vec![
                    AuditChange::new("delete_member_days", None, Some(json!(days))),
                    AuditChange::new("members_removed", None, Some(json!(removed.len()))),
                ],
                None,
            )
            .await;
        }
        Ok(removed.len())
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
        self.audit(guild, actor, AuditAction::MemberUpdate, user.get(), None)
            .await;
        Ok(m)
    }

    async fn kick_member(&self, actor: UserId, guild: GuildId, user: UserId) -> ChatResult<()> {
        self.svc.kick_member(actor, guild, user).await?;
        self.emit_member_left(guild, user).await;
        self.audit(guild, actor, AuditAction::MemberKick, user.get(), None)
            .await;
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
        delete_message_seconds: u32,
    ) -> ChatResult<()> {
        let audit_reason = reason.clone();
        let out = self
            .svc
            .ban_member_purging(actor, guild, user, reason, delete_message_seconds)
            .await?;
        self.audit(
            guild,
            actor,
            AuditAction::MemberBanAdd,
            user.get(),
            audit_reason.as_deref(),
        )
        .await;
        if out.was_member {
            self.emit_member_left(guild, user).await;
        }
        self.emit(DomainEvent::MemberBanned {
            guild,
            user,
            payload: json!({ "guild_id": guild.to_string(), "user": val(&UserRef::new(out.ban.user_id)) }),
        })
        .await;
        // One bulk-delete dispatch per channel that lost messages (open question 158).
        for (channel, ids) in out.purged {
            if ids.is_empty() {
                continue;
            }
            self.emit(DomainEvent::MessagesBulkDeleted {
                guild: Some(guild),
                channel,
                message_ids: ids.iter().map(|m| m.get()).collect(),
            })
            .await;
            if let Some(a) = &self.audit {
                a.record(
                    guild,
                    actor,
                    AuditAction::MessageBulkDelete,
                    Some(channel.get()),
                    vec![AuditChange::new("count", None, Some(json!(ids.len())))],
                    audit_reason.as_deref(),
                )
                .await;
            }
        }
        Ok(())
    }

    async fn user_summaries(&self, ids: &[UserId]) -> ChatResult<Vec<UserSummary>> {
        self.svc.user_summaries(ids).await
    }

    async fn unban(&self, actor: UserId, guild: GuildId, user: UserId) -> ChatResult<()> {
        self.svc.unban(actor, guild, user).await?;
        self.audit(guild, actor, AuditAction::MemberBanRemove, user.get(), None)
            .await;
        self.emit(DomainEvent::MemberUnbanned {
            guild,
            user,
            payload: json!({ "guild_id": guild.to_string(), "user": val(&UserRef::new(user)) }),
        })
        .await;
        Ok(())
    }
}
