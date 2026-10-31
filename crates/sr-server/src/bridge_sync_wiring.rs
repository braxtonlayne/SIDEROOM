//! BRG-SYN-001: the whole-guild mirror wired into the server process.
//!
//! * [`ChatSrStructure`] is the SIDE ROOM side of the mirror (`sr_bridge::sync::SrStructure`): it changes roles,
//!   channels and categories through the same guild backend the routes use, so the gateway, the audit log and the
//!   rules of every edit apply as for any other change.
//! * [`spawn_sync_worker`] runs the ops queue, the scheduled reconcile, the access check and the hold job.
//!
//! Known limit (handoff): the backend has no `Origin::Bridge` parameter yet, so a change applied here from Discord
//! is performed as the server owner (the one actor that passes every rule) and the audit row names the owner, not
//! the bridge user. Loop prevention rests on the state comparison of `synced_state` (design 11.3, layer 2), which
//! is sufficient on its own; the origin tag only saves work. An owner-confirmed permission import is the owner's
//! act, which is what the design wants.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use sqlx::PgPool;
use sr_api::routes::guilds::backend::GuildBackend;
use sr_bridge::sync::{
    BridgeOrigin, SrNewChannel, SrNewRole, SrStructuralPatch, SrStructure, SyncEngine, store,
};
use sr_bridge::{BridgeError, BridgeResult};
use sr_chat::admin_rules::ChannelMove;
use sr_chat::channel_rules::{ChannelPatch, NewChannel};
use sr_chat::domain::ChannelType;
use sr_chat::role_rules::{NewRole, RolePatch};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, GuildId, RoleId, UserId};
use tokio::task::JoinHandle;

pub struct ChatSrStructure {
    pub backend: Arc<dyn GuildBackend>,
    pub pool: PgPool,
}

fn uid(v: i64) -> UserId {
    UserId::new(u64::try_from(v).unwrap_or(0))
}

fn gid(v: i64) -> GuildId {
    GuildId::new(u64::try_from(v).unwrap_or(0))
}

fn cid(v: i64) -> ChannelId {
    ChannelId::new(u64::try_from(v).unwrap_or(0))
}

fn rid(v: i64) -> RoleId {
    RoleId::new(u64::try_from(v).unwrap_or(0))
}

fn id_of<T: Into<u64>>(v: T) -> i64 {
    i64::try_from(v.into()).unwrap_or(i64::MAX)
}

fn fail(e: impl std::fmt::Display) -> BridgeError {
    BridgeError::Conflict(format!("SIDE ROOM refused the change: {e}"))
}

impl ChatSrStructure {
    /// The actor of a change: the person who confirmed it when there is one, else the server owner.
    async fn actor(&self, guild: i64, origin: &BridgeOrigin) -> BridgeResult<UserId> {
        if let Some(u) = origin.sr_actor {
            return Ok(uid(u));
        }
        let owner: Option<i64> = sqlx::query_scalar("SELECT owner_id FROM guilds WHERE id = $1")
            .bind(guild)
            .fetch_optional(&self.pool)
            .await?;
        owner.map(uid).ok_or(BridgeError::NotFound("guild"))
    }
}

#[async_trait]
impl SrStructure for ChatSrStructure {
    async fn create_role(
        &self,
        guild: i64,
        new: SrNewRole,
        origin: &BridgeOrigin,
    ) -> BridgeResult<i64> {
        let actor = self.actor(guild, origin).await?;
        let role = self
            .backend
            .create_role(
                actor,
                gid(guild),
                NewRole {
                    name: Some(new.name),
                    // A role created from Discord has NO permissions.
                    permissions: Some(Permissions::empty()),
                    color: u32::try_from(new.color).ok(),
                    colors: None,
                    hoist: new.hoist,
                    mentionable: new.mentionable,
                    unicode_emoji: new.unicode_emoji,
                },
            )
            .await
            .map_err(fail)?;
        Ok(id_of(role.id.get()))
    }

    async fn create_channel(
        &self,
        guild: i64,
        new: SrNewChannel,
        origin: &BridgeOrigin,
    ) -> BridgeResult<i64> {
        let actor = self.actor(guild, origin).await?;
        let kind = ChannelType::from_code(new.kind)
            .ok_or_else(|| BridgeError::Invalid("kind: unknown channel type".into()))?;
        let mut input = NewChannel::new(kind, new.name);
        input.topic = new.topic;
        input.parent_id = new.parent_id.map(cid);
        input.position = Some(new.position);
        input.nsfw = new.nsfw;
        input.rate_limit_per_user = u32::try_from(new.rate_limit_per_user).unwrap_or(0);
        input.bitrate = new.bitrate.and_then(|b| u32::try_from(b).ok());
        input.user_limit = new.user_limit.and_then(|b| u32::try_from(b).ok());
        // As Discord's "synced to category": copy the SIDE ROOM parent's overwrites, nothing more.
        let overwrites = match new.parent_id {
            Some(p) => self
                .backend
                .get_channel(actor, cid(p))
                .await
                .map(|c| c.overwrites)
                .unwrap_or_default(),
            None => vec![],
        };
        let created = self
            .backend
            .create_channel(actor, gid(guild), input, overwrites)
            .await
            .map_err(fail)?;
        Ok(id_of(created.channel.id.get()))
    }

    async fn apply(
        &self,
        guild: i64,
        patch: &SrStructuralPatch,
        origin: &BridgeOrigin,
    ) -> BridgeResult<()> {
        let actor = self.actor(guild, origin).await?;
        match patch {
            SrStructuralPatch::Role { role_id, patch: p } => {
                self.backend
                    .edit_role(
                        actor,
                        gid(guild),
                        rid(*role_id),
                        RolePatch {
                            name: p.name.clone(),
                            color: p.color.and_then(|c| u32::try_from(c).ok()),
                            hoist: p.hoist,
                            mentionable: p.mentionable,
                            unicode_emoji: p.unicode_emoji.clone(),
                            ..RolePatch::default()
                        },
                    )
                    .await
                    .map_err(fail)?;
            }
            SrStructuralPatch::Channel {
                channel_id,
                patch: p,
            } => {
                let mv = ChannelMove {
                    position: p.position,
                    parent_id: p.parent_id.map(|o| o.map(cid)),
                    lock_permissions: false,
                };
                self.backend
                    .edit_channel(
                        actor,
                        cid(*channel_id),
                        ChannelPatch {
                            name: p.name.clone(),
                            topic: p.topic.clone(),
                            nsfw: p.nsfw,
                            rate_limit_per_user: p
                                .rate_limit_per_user
                                .and_then(|v| u32::try_from(v).ok()),
                            bitrate: p.bitrate.and_then(|v| u32::try_from(v).ok()),
                            user_limit: p.user_limit.and_then(|v| u32::try_from(v).ok()),
                            ..ChannelPatch::default()
                        },
                        mv,
                    )
                    .await
                    .map_err(fail)?;
            }
            // Thread state (archived, locked, tags) is applied by the thread routes' own service; not wired yet.
            SrStructuralPatch::Thread { .. } => {}
        }
        Ok(())
    }

    async fn delete_role(&self, guild: i64, role: i64, origin: &BridgeOrigin) -> BridgeResult<()> {
        let actor = self.actor(guild, origin).await?;
        self.backend
            .delete_role(actor, gid(guild), rid(role))
            .await
            .map_err(fail)
    }

    async fn delete_channel(
        &self,
        guild: i64,
        channel: i64,
        origin: &BridgeOrigin,
    ) -> BridgeResult<()> {
        let actor = self.actor(guild, origin).await?;
        self.backend
            .delete_channel(actor, cid(channel))
            .await
            .map(|_| ())
            .map_err(fail)
    }

    async fn import_role_permissions(
        &self,
        guild: i64,
        role: i64,
        shared: Permissions,
        origin: &BridgeOrigin,
    ) -> BridgeResult<()> {
        // The owner confirmed the plan; the audit row is theirs.
        let actor = self.actor(guild, origin).await?;
        let current: Option<i64> =
            sqlx::query_scalar("SELECT permissions FROM roles WHERE id = $1 AND guild_id = $2")
                .bind(role)
                .bind(guild)
                .fetch_optional(&self.pool)
                .await?;
        let now = Permissions::from_bits_truncate(u64::try_from(current.unwrap_or(0)).unwrap_or(0));
        self.backend
            .edit_role(
                actor,
                gid(guild),
                rid(role),
                RolePatch {
                    permissions: Some(now | shared),
                    ..RolePatch::default()
                },
            )
            .await
            .map_err(fail)?;
        Ok(())
    }

    async fn role_has_denies(&self, guild: i64, role: i64) -> BridgeResult<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM permission_overwrites o JOIN channels c ON c.id = o.channel_id \
             WHERE c.guild_id = $1 AND o.target_id = $2 AND o.target_type = 'role' AND o.deny <> 0)",
        )
        .bind(guild)
        .bind(role)
        .fetch_one(&self.pool)
        .await?)
    }
}

/// How often the sync queue is looked at.
const SYNC_TICK: Duration = Duration::from_secs(2);

/// The sync worker: the ops queue every tick; every minute the scheduled reconcile (`bridge.reconcile_minutes`),
/// the invalid-request breaker, the bot-access check and the hold job. The three `bridge.*` settings are read
/// each minute, so an operator's change takes effect without a restart.
pub fn spawn_sync_worker(engine: SyncEngine, admin: sr_admin::AdminService) -> JoinHandle<()> {
    use sr_admin::settings as k;
    tokio::spawn(async move {
        if let Err(e) = sr_bridge::sync::ops::recover_running(&engine.svc.pool).await {
            tracing::warn!(error = %e, "bridge sync: could not recover running ops");
        }
        // A seed cut short by a restart resumes (the seed is idempotent: what exists is matched, not made twice).
        match store::seeding_guilds(&engine.svc.pool).await {
            Ok(guilds) => {
                for g in guilds {
                    let seed = engine.clone();
                    tokio::spawn(async move {
                        if let Err(e) = seed.run_seed(g).await {
                            tracing::error!(error = %e, guild = g, "bridge sync: resuming the seed failed");
                        }
                    });
                }
            }
            Err(e) => tracing::warn!(error = %e, "bridge sync: could not list seeding bridges"),
        }
        let mut tick = tokio::time::interval(SYNC_TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut n: u64 = 0;
        let (mut hold_hours, mut reconcile_minutes) = (24_i64, 15_i64);
        loop {
            tick.tick().await;
            if n.is_multiple_of(30) {
                hold_hours = admin
                    .setting_value_i64(k::BRIDGE_DELETE_HOLD_HOURS)
                    .await
                    .unwrap_or(24);
                reconcile_minutes = admin
                    .setting_value_i64(k::BRIDGE_RECONCILE_MINUTES)
                    .await
                    .unwrap_or(15);
                let per_sec = admin
                    .setting_value_i64(k::BRIDGE_STRUCT_WRITES_PER_SEC)
                    .await
                    .unwrap_or(2);
                engine
                    .svc
                    .discord
                    .set_struct_writes_per_sec(per_sec as f64)
                    .await;
            }
            n += 1;
            let live = engine.clone().with_hold_hours(hold_hours);
            if let Err(e) = live.run_ops_pass(20).await {
                tracing::warn!(error = %e, "bridge sync: ops pass failed");
            }
            if n.is_multiple_of(30) {
                minute(&live, reconcile_minutes).await;
            }
        }
    })
}

/// SIDE ROOM role, channel and thread changes of whole-guild bridges queue a look (design 11.1). A lagged
/// receiver loses nothing that matters: the scheduled reconcile looks at everything.
pub fn spawn_sync_listener(
    mut events: futures::stream::BoxStream<'static, crate::lag_bus::TapItem>,
    engine: SyncEngine,
) -> JoinHandle<()> {
    use futures::StreamExt;
    tokio::spawn(async move {
        while let Some(item) = events.next().await {
            if let Ok(ev) = item {
                engine.handle_sr_event(&ev).await;
            }
        }
    })
}

async fn minute(engine: &SyncEngine, reconcile_minutes: i64) {
    if let Err(e) = engine.enqueue_scheduled_reconciles(reconcile_minutes).await {
        tracing::warn!(error = %e, "bridge sync: could not queue the scheduled reconcile");
    }
    match store::live_whole_guild(&engine.svc.pool).await {
        Ok(guilds) => {
            // The invalid-request breakers (design 11.5): Discord's per-IP limit is shared by every bridge on the
            // instance, so past the instance limit every bridge pauses; below it, each bridge by its own count.
            let window = Duration::from_secs(600);
            let instance = engine.svc.discord.invalid_requests_within(window);
            for g in guilds {
                let invalid = if instance >= sr_bridge::discord_structure::INSTANCE_INVALID_LIMIT {
                    instance
                } else {
                    match engine.svc.open_bot_token(g).await {
                        Ok(t) => engine.svc.discord.invalid_requests_for(&t, window),
                        Err(_) => 0,
                    }
                };
                if let Err(e) = engine.trip_if_invalid(g, invalid).await {
                    tracing::warn!(error = %e, guild = g, "bridge sync: breaker check failed");
                }
                if let Err(e) = engine.check_access(g).await {
                    tracing::warn!(error = %e, guild = g, "bridge sync: access check failed");
                }
            }
        }
        Err(e) => tracing::warn!(error = %e, "bridge sync: could not list live bridges"),
    }
    if let Err(e) = engine.run_due_holds().await {
        tracing::warn!(error = %e, "bridge sync: hold job failed");
    }
    // Retention (doc 09): closed conflict rows after 90 days, plans a day after they lapsed.
    if let Err(e) = engine.svc.prune_closed_conflicts().await {
        tracing::warn!(error = %e, "bridge sync: could not prune closed conflicts");
    }
    if let Err(e) = store::prune_plans(&engine.svc.pool).await {
        tracing::warn!(error = %e, "bridge sync: could not prune lapsed plans");
    }
    if let Err(e) = sr_bridge::sync::ops::prune_done(&engine.svc.pool).await {
        tracing::warn!(error = %e, "bridge sync: could not prune finished ops");
    }
}
