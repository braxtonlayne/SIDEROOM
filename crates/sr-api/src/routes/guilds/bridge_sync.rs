//! The whole-guild mirror's routes (BRG-SYN-001, wave 35b; migrations 1750 to 1753).
//!
//! Session-only, like every bridge route. Two AUTHZ classes (the matrix rows are BRG-SYN-002's):
//!
//! * **`guild_owner`** (the server owner alone, with the server's second-factor step): `POST .../bridge/mode`
//!   (switch between `selective` and `whole_guild`, confirming a plan by its hash), `POST .../bridge/teardown`
//!   (switch the bridge off with the whole_guild options) and `GET .../bridge/teardown-preview`.
//! * **`bridge_admin`** (`MANAGE_GUILD` once the owner switched the bridge on): build, read and edit a plan (so a
//!   manager can prepare it for the owner), the sync status, reconcile now, pause and resume, the held deletions.
//!   Restoring or deleting a held channel additionally needs `MANAGE_CHANNELS`.
//!
//! Nothing here accepts a permission value: the Discord-to-SIDE ROOM side takes no permission, overwrite or role
//! position at all (`SrStructuralPatch`), and the plan only chooses which side's names and order win.

use super::bridge::{Caller, bridge_admin, guild_owner, map_err};
use crate::auth::SessionOnly;
use crate::error::ApiResult;
use crate::openapi::ErrorBody;
use crate::routes::Ext;
use crate::routes::guilds::safety::MfaGate;
use crate::routes::messages::error::ApiJson;
use crate::routes::settings::deps::{SettingsDeps, id, invalid};
use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sr_bridge::sync::conflicts::{ConflictKind, Resolution};
use sr_bridge::sync::{
    Action, ItemKind, PlanEdit, PlanView, SyncEngine, TeardownPreview, holds, ops, store,
};
use sr_bridge::{BridgeService, TeardownOptions, TeardownReason};
use sr_core::permissions::Permissions;
use sr_core::{ApiError, ErrorCode};
use utoipa::ToSchema;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/guilds/{guild_id}/bridge/plan", post(create_plan))
        .route(
            "/guilds/{guild_id}/bridge/plan/{plan_id}",
            get(get_plan).patch(edit_plan),
        )
        .route("/guilds/{guild_id}/bridge/mode", post(set_mode))
        .route("/guilds/{guild_id}/bridge/sync", get(sync_status))
        .route("/guilds/{guild_id}/bridge/reconcile", post(reconcile_now))
        .route("/guilds/{guild_id}/bridge/pause", post(pause))
        .route("/guilds/{guild_id}/bridge/resume", post(resume))
        .route("/guilds/{guild_id}/bridge/holds", get(list_holds))
        .route(
            "/guilds/{guild_id}/bridge/holds/{channel_id}/restore",
            post(restore_hold),
        )
        .route(
            "/guilds/{guild_id}/bridge/holds/{channel_id}/delete",
            post(delete_hold),
        )
        .route(
            "/guilds/{guild_id}/bridge/teardown-preview",
            get(teardown_preview),
        )
        .route("/guilds/{guild_id}/bridge/teardown", post(teardown))
        .route("/guilds/{guild_id}/bridge/preflight", get(preflight))
        .route("/guilds/{guild_id}/bridge/mirrors", get(mirrors))
}

// ---- wire types ------------------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePlanBody {
    /// Whose names, topics and order win for matched channels: `sr` (default) or `discord`.
    #[serde(default)]
    pub seed_structure: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PlanCreated {
    pub plan_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PlanItemObject {
    /// `role`, `category`, `channel` or `thread`.
    pub kind: String,
    /// `match`, `pinned`, `create_on_discord` or `create_here`.
    pub action: String,
    pub sr_id: Option<String>,
    pub discord_id: Option<String>,
    pub name: String,
    pub channel_type: Option<i64>,
    /// Roles: the shared permission word SIDE ROOM will push (matched) as a decimal string.
    pub sr_permissions: String,
    /// Roles: the word Discord holds now.
    pub discord_permissions: String,
    /// Roles that exist only on Discord: the dangerous bits of the word that would be imported.
    pub dangerous_bits: String,
    /// Exists only on Discord: import its shared permission bits (the owner decides, Q4).
    pub import: bool,
    pub degraded: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NotMirroredObject {
    pub kind: String,
    pub sr_id: Option<String>,
    pub discord_id: Option<String>,
    pub name: String,
    pub reason: String,
    /// `managed_role`, `dm`, `e2ee`, `staff_guild` or `degraded_type`.
    pub code: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BlockerObject {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PlanObject {
    pub id: String,
    /// `building`, `ready`, `failed`, `confirmed`, `expired` or `superseded`.
    pub status: String,
    pub seed_structure: String,
    /// The hash the owner confirms with `POST .../bridge/mode`.
    pub plan_hash: String,
    pub expires_at: i64,
    pub blockers: Vec<BlockerObject>,
    pub items: Vec<PlanItemObject>,
    pub not_mirrored: Vec<NotMirroredObject>,
    pub estimated_writes: u32,
}

/// One edit of an open plan. `op` is `unlink`, `link`, `import` or `seed_structure`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PlanEditBody {
    pub op: String,
    #[serde(default)]
    pub index: Option<usize>,
    /// `role`, `category`, `channel` or `thread` (for `link`).
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub sr_id: Option<String>,
    #[serde(default)]
    pub discord_id: Option<String>,
    #[serde(default)]
    pub import: Option<bool>,
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BridgeModeBody {
    /// `selective` or `whole_guild`.
    pub mode: String,
    /// The plan to confirm (required for `whole_guild`).
    #[serde(default)]
    pub plan_id: Option<String>,
    /// The plan's hash as the client saw it.
    #[serde(default)]
    pub plan_hash: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BridgeModeObject {
    pub mode: String,
    pub sync_state: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SyncStatusObject {
    pub mode: String,
    /// `off`, `seeding`, `live`, `paused`, `paused_discord_access` or `paused_invalid_requests`.
    pub sync_state: String,
    pub last_reconcile_at: Option<i64>,
    pub queue_pending: i64,
    pub queue_running: i64,
    pub dead_ops: i64,
    pub role_maps: i64,
    pub channel_maps: i64,
    pub degraded: i64,
    pub frozen: i64,
    pub held_deletions: i64,
    /// Discord granted the Server Members and Message Content intents (from the shard's last READY).
    pub intents_ok: bool,
    /// When "Reconcile now" is next allowed (Unix ms); null when it is allowed now.
    pub reconcile_available_at: Option<i64>,
}

/// One line of the preflight checklist (design 9.2): `bot_admin`, `bot_top_role`, `intents`, `caps` or
/// `unique_guild`. Every one of them blocks a plan while not ok.
#[derive(Debug, Serialize, ToSchema)]
pub struct PreflightCheckObject {
    pub id: String,
    pub ok: bool,
    pub blocker: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PreflightObject {
    pub checks: Vec<PreflightCheckObject>,
    /// The bot's invite link with Administrator (whole-server mode needs it).
    pub invite_url: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MirrorRoleObject {
    pub role_id: String,
    pub discord_role_id: String,
    /// Bit numbers of the permissions Discord has on this role that SIDE ROOM has no bit for (set on Discord,
    /// kept there); empty when none.
    pub discord_only_bits: Vec<u32>,
    /// `mapped` or `frozen`.
    pub state: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MirrorChannelObject {
    pub channel_id: String,
    pub discord_channel_id: String,
    /// `mapped`, `degraded`, `frozen` or `held`.
    pub state: String,
    pub degraded_reason: Option<String>,
}

/// Which roles and channels are mirrored, for the badges of the role editor and the channel settings.
#[derive(Debug, Serialize, ToSchema)]
pub struct MirrorsObject {
    /// `selective` or `whole_guild`.
    pub mode: String,
    pub roles: Vec<MirrorRoleObject>,
    pub channels: Vec<MirrorChannelObject>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BridgeHoldObject {
    pub channel_id: String,
    pub discord_channel_id: String,
    pub discord_actor_id: Option<String>,
    pub held_at: i64,
    pub delete_after: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BridgeRestoredObject {
    pub discord_channel_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TeardownPreviewObject {
    pub whole_guild: bool,
    pub created_roles: i64,
    pub created_categories: i64,
    pub created_channels: i64,
    pub created_threads: i64,
    pub created_with_discord_messages: i64,
    pub restorable_roles: i64,
    pub restorable_channels: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct BridgeTeardownBody {
    /// Also delete the roles, categories, channels and threads the bridge created on Discord.
    #[serde(default)]
    pub remove_created: bool,
    /// Also restore the permissions Discord had before the bridge (roles and overwrites that existed then).
    #[serde(default)]
    pub restore_permissions: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TeardownResultObject {
    pub maps_removed: i64,
    pub webhooks_deleted: i64,
    pub roles_removed: i64,
    pub channels_removed: i64,
    pub roles_restored: i64,
    pub channels_restored: i64,
    /// Discord objects (ids) the bridge could not remove or restore: do those by hand on Discord.
    pub failed_ids: Vec<String>,
    /// Webhook ids still on Discord.
    pub leftover_webhooks: Vec<String>,
}

// ---- helpers ---------------------------------------------------------------------------------------

fn s(v: i64) -> String {
    v.to_string()
}

fn kind_str(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Role => "role",
        ItemKind::Category => "category",
        ItemKind::Channel => "channel",
        ItemKind::Thread => "thread",
    }
}

fn kind_parse(raw: &str) -> Option<ItemKind> {
    [
        ItemKind::Role,
        ItemKind::Category,
        ItemKind::Channel,
        ItemKind::Thread,
    ]
    .into_iter()
    .find(|k| kind_str(*k) == raw)
}

fn action_str(a: Action) -> &'static str {
    match a {
        Action::Match => "match",
        Action::Pinned => "pinned",
        Action::CreateOnDiscord => "create_on_discord",
        Action::CreateHere => "create_here",
    }
}

fn plan_object(v: &PlanView) -> PlanObject {
    let danger = sr_bridge::sync::DANGEROUS_BITS;
    PlanObject {
        id: s(v.id),
        status: v.status.clone(),
        seed_structure: v.seed_structure.clone(),
        plan_hash: v.plan_hash.clone(),
        expires_at: v.expires_at_ms,
        blockers: v
            .blockers
            .iter()
            .map(|b| BlockerObject {
                code: b.code.clone(),
                message: b.message.clone(),
            })
            .collect(),
        items: v
            .plan
            .items
            .iter()
            .map(|i| PlanItemObject {
                kind: kind_str(i.kind).into(),
                action: action_str(i.action).into(),
                sr_id: i.sr_id.map(s),
                discord_id: i.discord_id.map(s),
                name: i.name.clone(),
                channel_type: i.channel_type,
                sr_permissions: i.sr_permissions.to_string(),
                discord_permissions: i.discord_permissions.to_string(),
                dangerous_bits: if i.action == Action::CreateHere {
                    (i.discord_permissions & danger).to_string()
                } else {
                    "0".into()
                },
                import: i.import,
                degraded: i.degraded.clone(),
            })
            .collect(),
        not_mirrored: v
            .plan
            .not_mirrored
            .iter()
            .map(|n| NotMirroredObject {
                kind: kind_str(n.kind).into(),
                sr_id: n.sr_id.map(s),
                discord_id: n.discord_id.map(s),
                name: n.name.clone(),
                reason: n.reason.clone(),
                code: n.code.clone(),
            })
            .collect(),
        estimated_writes: v.plan.estimated_writes,
    }
}

fn parse_edit(b: PlanEditBody) -> ApiResult<PlanEdit> {
    let need = |name: &str| {
        invalid(
            name,
            "BRIDGE_INVALID",
            "This field is required for this edit.",
        )
    };
    Ok(match b.op.as_str() {
        "unlink" => PlanEdit::Unlink {
            index: b.index.ok_or_else(|| need("index"))?,
        },
        "import" => PlanEdit::Import {
            index: b.index.ok_or_else(|| need("index"))?,
            import: b.import.ok_or_else(|| need("import"))?,
        },
        "link" => PlanEdit::Link {
            kind: b
                .kind
                .as_deref()
                .and_then(kind_parse)
                .ok_or_else(|| need("kind"))?,
            sr_id: id("sr_id", b.sr_id.as_deref().ok_or_else(|| need("sr_id"))?)?,
            discord_id: id(
                "discord_id",
                b.discord_id.as_deref().ok_or_else(|| need("discord_id"))?,
            )?,
        },
        "seed_structure" => PlanEdit::SeedStructure {
            value: b.value.ok_or_else(|| need("value"))?,
        },
        _ => {
            return Err(invalid(
                "op",
                "ENUM_TYPE_INVALID",
                "op must be unlink, link, import or seed_structure.",
            ));
        }
    })
}

/// `bridge_admin` plus `MANAGE_CHANNELS` (restoring or deleting a held channel is a channel action here).
fn needs_manage_channels(c: &Caller) -> ApiResult<()> {
    if c.acc.perms.has(Permissions::MANAGE_CHANNELS) {
        Ok(())
    } else {
        Err(ErrorCode::MissingPermissions.into())
    }
}

// ---- plans (bridge_admin) --------------------------------------------------------------------------

/// Build a plan, a dry run that writes nothing to either side (`MANAGE_GUILD`; a manager can prepare it for the
/// owner). A new plan supersedes the open one.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/bridge/plan",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = CreatePlanBody,
    responses((status = 201, body = PlanCreated), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create_plan(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Ext(engine): Ext<SyncEngine>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<CreatePlanBody>,
) -> ApiResult<(StatusCode, Json<PlanCreated>)> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let seed = body.seed_structure.as_deref().unwrap_or("sr");
    let plan_id = engine
        .create_plan(c.gid, c.user, seed)
        .await
        .map_err(map_err)?;
    Ok((
        StatusCode::CREATED,
        Json(PlanCreated {
            plan_id: s(plan_id),
        }),
    ))
}

/// Read a plan (`MANAGE_GUILD`). Another server's plan id is a 404.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/bridge/plan/{plan_id}",
    params(("guild_id" = String, Path, description = "Guild snowflake"), ("plan_id" = String, Path, description = "Plan snowflake")),
    responses((status = 200, body = PlanObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_plan(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Ext(engine): Ext<SyncEngine>,
    Path((guild_id, plan_id)): Path<(String, String)>,
) -> ApiResult<Json<PlanObject>> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let plan_id: i64 = id("plan_id", &plan_id)?;
    let v = engine.get_plan(c.gid, plan_id).await.map_err(map_err)?;
    Ok(Json(plan_object(&v)))
}

/// Edit an open plan: unlink or link a pair, switch an import, choose the seed structure (`MANAGE_GUILD`). The
/// plan's hash changes with it.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/bridge/plan/{plan_id}",
    params(("guild_id" = String, Path, description = "Guild snowflake"), ("plan_id" = String, Path, description = "Plan snowflake")),
    request_body = PlanEditBody,
    responses((status = 200, body = PlanObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn edit_plan(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Ext(engine): Ext<SyncEngine>,
    Path((guild_id, plan_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<PlanEditBody>,
) -> ApiResult<Json<PlanObject>> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let plan_id: i64 = id("plan_id", &plan_id)?;
    let edit = parse_edit(body)?;
    let v = engine
        .edit_plan(c.gid, plan_id, edit)
        .await
        .map_err(map_err)?;
    Ok(Json(plan_object(&v)))
}

// ---- the mode (guild_owner) ------------------------------------------------------------------------

/// Switch the mode (the server owner alone, with the second-factor step). `whole_guild` confirms a plan by its
/// hash: a plan that no longer matches either server is a 409 `BRIDGE_PLAN_STALE`; the seed then runs in the
/// background. `selective` leaves everything where it is on both sides.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/bridge/mode",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = BridgeModeBody,
    responses((status = 200, body = BridgeModeObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn set_mode(
    SessionOnly(p): SessionOnly,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Ext(engine): Ext<SyncEngine>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<BridgeModeBody>,
) -> ApiResult<Json<BridgeModeObject>> {
    let c = guild_owner(&p, &gate, &deps, &guild_id).await?;
    super::bridge::switches(&bridge, c.gid).await?;
    match body.mode.as_str() {
        "whole_guild" => {
            let plan_id: i64 = id(
                "plan_id",
                body.plan_id.as_deref().ok_or_else(|| {
                    invalid("plan_id", "BRIDGE_INVALID", "A confirmed plan is required.")
                })?,
            )?;
            let hash = body.plan_hash.as_deref().ok_or_else(|| {
                invalid("plan_hash", "BRIDGE_INVALID", "The plan hash is required.")
            })?;
            engine
                .confirm_mode(c.gid, c.user, plan_id, hash)
                .await
                .map_err(map_err)?;
            let seed = engine.clone();
            let gid = c.gid;
            tokio::spawn(async move {
                if let Err(e) = seed.run_seed(gid).await {
                    tracing::error!(error = %e, guild = gid, "the whole-guild seed failed");
                }
            });
        }
        "selective" => {
            engine
                .switch_to_selective(c.gid, c.user)
                .await
                .map_err(map_err)?;
        }
        _ => {
            return Err(invalid(
                "mode",
                "ENUM_TYPE_INVALID",
                "Mode must be selective or whole_guild.",
            ));
        }
    }
    let st = store::get_sync(&bridge.pool, c.gid)
        .await
        .map_err(map_err)?
        .ok_or_else(|| ApiError::from(ErrorCode::SrNotFound))?;
    Ok(Json(BridgeModeObject {
        mode: st.mode.as_str().into(),
        sync_state: st.state.as_str().into(),
    }))
}

// ---- status, reconcile, pause (bridge_admin) --------------------------------------------------------

/// The sync status page (`MANAGE_GUILD`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/bridge/sync",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = SyncStatusObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn sync_status(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Ext(engine): Ext<SyncEngine>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<SyncStatusObject>> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let st = store::get_sync(&bridge.pool, c.gid)
        .await
        .map_err(map_err)?
        .ok_or_else(|| ApiError::from(ErrorCode::SrNotFound))?;
    let q = ops::counts(&bridge.pool, c.gid).await.map_err(map_err)?;
    let maps = store::list_structure_maps(&bridge.pool, c.gid)
        .await
        .map_err(map_err)?;
    let roles = store::list_role_maps(&bridge.pool, c.gid)
        .await
        .map_err(map_err)?;
    let held = holds::list_held(&bridge.pool, c.gid)
        .await
        .map_err(map_err)?;
    Ok(Json(SyncStatusObject {
        mode: st.mode.as_str().into(),
        sync_state: st.state.as_str().into(),
        last_reconcile_at: st.last_reconcile_ms,
        queue_pending: q.pending,
        queue_running: q.running,
        dead_ops: q.dead,
        role_maps: roles.len() as i64,
        channel_maps: maps.len() as i64,
        degraded: maps.iter().filter(|m| m.sync_status == "degraded").count() as i64,
        frozen: maps.iter().filter(|m| m.sync_status == "frozen").count() as i64
            + roles.iter().filter(|m| m.sync_status == "frozen").count() as i64,
        held_deletions: held.len() as i64,
        intents_ok: intents_ok(&bridge, c.gid).await?,
        reconcile_available_at: engine
            .reconcile_available_at_ms(c.gid)
            .await
            .map_err(map_err)?,
    }))
}

/// Server Members and Message Content granted at the shard's last READY.
async fn intents_ok(bridge: &BridgeService, guild: i64) -> ApiResult<bool> {
    use sr_bridge::discord_structure::intents;
    let need = intents::GUILD_MEMBERS | intents::MESSAGE_CONTENT;
    let b = sr_bridge::model::get_bridge(&bridge.pool, guild)
        .await
        .map_err(map_err)?;
    Ok(b.is_some_and(|b| u64::try_from(b.intents_granted).unwrap_or(0) & need == need))
}

/// The bot invite link with Administrator, for whole-server mode (design 10).
fn admin_invite(bot_user: i64) -> String {
    format!(
        "https://discord.com/oauth2/authorize?client_id={bot_user}&scope=bot&permissions={}",
        Permissions::ADMINISTRATOR.bits()
    )
}

/// Run the preflight checks now (`MANAGE_GUILD`): reads both servers, writes nothing.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/bridge/preflight",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = PreflightObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn preflight(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Ext(engine): Ext<SyncEngine>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<PreflightObject>> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let b = sr_bridge::model::get_bridge(&bridge.pool, c.gid)
        .await
        .map_err(map_err)?
        .ok_or_else(|| ApiError::from(ErrorCode::SrNotFound))?;
    let (facts, blockers) = engine.preflight(c.gid).await.map_err(map_err)?;
    let blocked = |codes: &[&str]| {
        blockers
            .iter()
            .find(|x| codes.contains(&x.code.as_str()))
            .map(|x| x.message.clone())
    };
    let check = |id: &str, ok: bool, detail: Option<String>| PreflightCheckObject {
        id: id.into(),
        ok,
        blocker: true,
        detail: if ok { None } else { detail },
    };
    let caps = blocked(&["role_cap", "channel_cap"]);
    let intents = blocked(&["intents_missing"]);
    Ok(Json(PreflightObject {
        checks: vec![
            check("bot_admin", facts.bot_administrator, None),
            check("bot_top_role", facts.bot_is_top_role, None),
            check("intents", intents.is_none(), intents),
            check("caps", caps.is_none(), caps),
            check("unique_guild", !facts.discord_guild_taken, None),
        ],
        invite_url: admin_invite(b.bot_user_id),
    }))
}

/// Which roles and channels of the server are mirrored (any member of the server: the badges). 404 while the
/// bridge is off or not connected.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/bridge/mirrors",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = MirrorsObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn mirrors(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<MirrorsObject>> {
    // Being a member of the server is the whole gate (as for the bridged member list).
    let (gid, _user, _acc) = super::bridge::who(&p, &deps, &guild_id).await?;
    let g = super::bridge::raw(gid.get());
    super::bridge::switches(&bridge, g).await?;
    let st = store::get_sync(&bridge.pool, g)
        .await
        .map_err(map_err)?
        .ok_or_else(|| ApiError::from(ErrorCode::SrNotFound))?;
    let held: std::collections::HashSet<i64> = holds::list_held(&bridge.pool, g)
        .await
        .map_err(map_err)?
        .into_iter()
        .map(|h| h.channel_id)
        .collect();
    let roles = store::list_role_maps(&bridge.pool, g)
        .await
        .map_err(map_err)?
        .into_iter()
        .filter(|m| !m.is_everyone)
        .map(|m| MirrorRoleObject {
            role_id: s(m.role_id),
            discord_role_id: s(m.discord_role_id),
            discord_only_bits: (0..64u32)
                .filter(|b| m.discord_only_bits & (1u64 << b) != 0)
                .collect(),
            state: m.sync_status,
        })
        .collect();
    let channels = store::list_structure_maps(&bridge.pool, g)
        .await
        .map_err(map_err)?
        .into_iter()
        .map(|m| MirrorChannelObject {
            channel_id: s(m.channel_id),
            discord_channel_id: s(m.discord_channel_id),
            state: if held.contains(&m.channel_id) {
                "held".into()
            } else {
                m.sync_status
            },
            degraded_reason: m.degraded_reason,
        })
        .collect();
    Ok(Json(MirrorsObject {
        mode: st.mode.as_str().into(),
        roles,
        channels,
    }))
}

/// Queue a reconcile now (`MANAGE_GUILD`; at most once per five minutes).
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/bridge/reconcile",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 202), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn reconcile_now(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Ext(engine): Ext<SyncEngine>,
    Path(guild_id): Path<String>,
) -> ApiResult<StatusCode> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    engine.reconcile_now(c.gid).await.map_err(map_err)?;
    Ok(StatusCode::ACCEPTED)
}

/// Pause the sync (`MANAGE_GUILD`): pushes, reverts and applies stop; SIDE ROOM is unaffected.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/bridge/pause",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn pause(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Ext(engine): Ext<SyncEngine>,
    Path(guild_id): Path<String>,
) -> ApiResult<StatusCode> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    engine.pause(c.gid).await.map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Resume the sync (`MANAGE_GUILD`).
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/bridge/resume",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn resume(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Ext(engine): Ext<SyncEngine>,
    Path(guild_id): Path<String>,
) -> ApiResult<StatusCode> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    engine.resume(c.gid).await.map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- held deletions ---------------------------------------------------------------------------------

/// Channels, categories and threads deleted on Discord and held here (`MANAGE_GUILD`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/bridge/holds",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<BridgeHoldObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_holds(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<Vec<BridgeHoldObject>>> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let list = holds::list_held(&bridge.pool, c.gid)
        .await
        .map_err(map_err)?;
    Ok(Json(
        list.into_iter()
            .map(|h| BridgeHoldObject {
                channel_id: s(h.channel_id),
                discord_channel_id: s(h.discord_channel_id),
                discord_actor_id: h.discord_actor_id.map(s),
                held_at: h.held_at_ms,
                delete_after: h.delete_after_ms,
            })
            .collect(),
    ))
}

/// Restore a held channel (`MANAGE_GUILD` and `MANAGE_CHANNELS`): it is recreated on Discord from SIDE ROOM's
/// state and re-mapped; the old Discord messages are gone.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/bridge/holds/{channel_id}/restore",
    params(("guild_id" = String, Path, description = "Guild snowflake"), ("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = BridgeRestoredObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn restore_hold(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Ext(engine): Ext<SyncEngine>,
    Path((guild_id, channel_id)): Path<(String, String)>,
) -> ApiResult<Json<BridgeRestoredObject>> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    needs_manage_channels(&c)?;
    let channel: i64 = id("channel_id", &channel_id)?;
    ensure_held_here(&bridge, c.gid, channel).await?;
    let d = engine.restore_held(c.gid, channel).await.map_err(map_err)?;
    bridge
        .resolve_open_for(
            c.gid,
            ConflictKind::DeletionHeld,
            channel,
            c.user,
            Resolution::Restored,
        )
        .await
        .map_err(map_err)?;
    Ok(Json(BridgeRestoredObject {
        discord_channel_id: s(d),
    }))
}

/// Delete a held channel now (`MANAGE_GUILD` and `MANAGE_CHANNELS`).
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/bridge/holds/{channel_id}/delete",
    params(("guild_id" = String, Path, description = "Guild snowflake"), ("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_hold(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Ext(engine): Ext<SyncEngine>,
    Path((guild_id, channel_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    needs_manage_channels(&c)?;
    let channel: i64 = id("channel_id", &channel_id)?;
    ensure_held_here(&bridge, c.gid, channel).await?;
    engine
        .delete_held_now(c.gid, channel, c.user)
        .await
        .map_err(map_err)?;
    bridge
        .resolve_open_for(
            c.gid,
            ConflictKind::DeletionHeld,
            channel,
            c.user,
            Resolution::DeletedNow,
        )
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// A hold of another server is a 404, never a hint that it exists.
async fn ensure_held_here(bridge: &BridgeService, guild: i64, channel: i64) -> ApiResult<()> {
    let held = holds::list_held(&bridge.pool, guild)
        .await
        .map_err(map_err)?;
    if held.iter().any(|h| h.channel_id == channel) {
        Ok(())
    } else {
        Err(ErrorCode::SrNotFound.into())
    }
}

// ---- teardown with options (guild_owner) -------------------------------------------------------------

/// What the teardown dialog shows before the owner chooses (the server owner alone).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/bridge/teardown-preview",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = TeardownPreviewObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn teardown_preview(
    SessionOnly(p): SessionOnly,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<TeardownPreviewObject>> {
    let c = guild_owner(&p, &gate, &deps, &guild_id).await?;
    super::bridge::switches(&bridge, c.gid).await?;
    let t: TeardownPreview = bridge.teardown_preview(c.gid).await.map_err(map_err)?;
    Ok(Json(TeardownPreviewObject {
        whole_guild: t.whole_guild,
        created_roles: t.created_roles,
        created_categories: t.created_categories,
        created_channels: t.created_channels,
        created_threads: t.created_threads,
        created_with_discord_messages: t.created_with_discord_messages,
        restorable_roles: t.restorable_roles,
        restorable_channels: t.restorable_channels,
    }))
}

/// Switch the bridge off with the whole_guild options (the server owner alone, second-factor step). The default
/// leaves Discord as it is; the bot never deletes a Discord object it did not create.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/bridge/teardown",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = BridgeTeardownBody,
    responses((status = 200, body = TeardownResultObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn teardown(
    SessionOnly(p): SessionOnly,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<BridgeTeardownBody>,
) -> ApiResult<Json<TeardownResultObject>> {
    let c = guild_owner(&p, &gate, &deps, &guild_id).await?;
    super::bridge::switches(&bridge, c.gid).await?;
    let r = bridge
        .teardown_with(
            c.gid,
            c.user,
            TeardownReason::Owner,
            TeardownOptions {
                remove_created: body.remove_created,
                restore_permissions: body.restore_permissions,
            },
        )
        .await
        .map_err(map_err)?;
    Ok(Json(TeardownResultObject {
        maps_removed: r.maps_removed,
        webhooks_deleted: r.webhooks_deleted,
        roles_removed: r.removal.roles,
        channels_removed: r.removal.channels,
        roles_restored: r.removal.restored_roles,
        channels_restored: r.removal.restored_channels,
        failed_ids: r.removal.failed.iter().map(|f| s(f.id)).collect(),
        leftover_webhooks: r.leftovers.iter().map(|l| s(l.webhook_id)).collect(),
    }))
}

/// OpenAPI fragment of the whole-guild routes.
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        create_plan,
        get_plan,
        edit_plan,
        set_mode,
        sync_status,
        reconcile_now,
        pause,
        resume,
        list_holds,
        restore_hold,
        delete_hold,
        teardown_preview,
        teardown,
        preflight,
        mirrors,
    ),
    components(schemas(
        ErrorBody,
        CreatePlanBody,
        PlanCreated,
        PlanItemObject,
        NotMirroredObject,
        BlockerObject,
        PlanObject,
        PlanEditBody,
        BridgeModeBody,
        BridgeModeObject,
        SyncStatusObject,
        BridgeHoldObject,
        BridgeRestoredObject,
        TeardownPreviewObject,
        BridgeTeardownBody,
        TeardownResultObject,
        PreflightCheckObject,
        PreflightObject,
        MirrorRoleObject,
        MirrorChannelObject,
        MirrorsObject,
    ))
)]
pub struct BridgeSyncApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    <BridgeSyncApiDoc as utoipa::OpenApi>::openapi()
}
