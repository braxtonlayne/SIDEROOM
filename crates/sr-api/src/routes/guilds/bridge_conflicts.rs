//! The whole-guild bridge's conflict log (BRG-SYN-002, wave 35b, migration 1754).
//!
//! Class `bridge_admin` (`MANAGE_GUILD` once the owner switched the bridge on; 404 while it is off; session only).
//! The log is also the proposal list: nothing here ever applies a Discord value to a SIDE ROOM permission by
//! itself. The two actions that change SIDE ROOM run as the person who clicks and pass the rule of the edit they
//! stand for:
//!
//! * `resolve` ("Apply on SIDE ROOM"): the editor was opened prefilled and the person saved an ordinary SIDE ROOM
//!   edit through the normal role or channel routes; this call only closes the row. It needs `MANAGE_ROLES`
//!   (roles and overwrites) or `MANAGE_CHANNELS` (channels), so a manager without them is 403;
//! * `take-discord` ("Take Discord's"): applies the Discord value of a `structure_conflict` row (name, colour,
//!   hoist, mentionable of a role; name, topic of a channel) through the same service the role and channel routes
//!   use, as the clicking person, so that service's own checks (hierarchy, `MANAGE_ROLES`, `MANAGE_CHANNELS`)
//!   apply. Permission fields are never accepted here.

use super::bridge::{bridge_admin, map_err};
use super::util::{Api, actor, ce, id as parse_id};
use crate::auth::SessionOnly;
use crate::error::ApiResult;
use crate::openapi::ErrorBody;
use crate::routes::Ext;
use crate::routes::messages::error::ApiJson;
use crate::routes::settings::deps::{SettingsDeps, invalid};
use crate::state::AppState;
use axum::{
    Json, Router,
    extract::{Path, Query},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sr_bridge::sync::conflicts::{Conflict, ConflictFilter, ConflictKind, ObjectKind, Resolution};
use sr_bridge::BridgeService;
use sr_chat::admin_rules::ChannelMove;
use sr_chat::channel_rules::ChannelPatch;
use sr_chat::role_rules::RolePatch;
use sr_core::permissions::Permissions;
use sr_core::{ApiError, ChannelId, ErrorCode, GuildId, RoleId};
use utoipa::{IntoParams, ToSchema};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/guilds/{guild_id}/bridge/conflicts", get(list_conflicts))
        .route(
            "/guilds/{guild_id}/bridge/conflicts/{conflict_id}",
            get(get_conflict),
        )
        .route(
            "/guilds/{guild_id}/bridge/conflicts/{conflict_id}/dismiss",
            post(dismiss_conflict),
        )
        .route(
            "/guilds/{guild_id}/bridge/conflicts/{conflict_id}/resolve",
            post(resolve_conflict),
        )
        .route(
            "/guilds/{guild_id}/bridge/conflicts/{conflict_id}/take-discord",
            post(take_discord),
        )
}

/// One row of the conflict log.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConflictObject {
    pub id: String,
    /// `permission_reverted`, `overwrite_trimmed`, `structure_conflict`, `role_deletion_refused`,
    /// `deletion_held`, `frozen`, `dead_op` or `cap`.
    pub kind: String,
    /// `role`, `channel`, `category`, `thread`, `overwrite`, `op` or `guild`.
    pub object_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sr_object_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord_object_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    pub field: String,
    /// What Discord has or attempted (permission words are decimal strings, shared bits only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord_value: Option<Value>,
    /// What SIDE ROOM has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sr_value: Option<Value>,
    /// The Discord user who did it, when Discord's audit log said so.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord_actor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord_actor_name: Option<String>,
    /// How many times it happened inside the coalescing window.
    pub count: i32,
    /// `open`, `resolved`, `dismissed` or `superseded`.
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<i64>,
    pub first_at: i64,
    pub last_at: i64,
}

impl From<&Conflict> for ConflictObject {
    fn from(c: &Conflict) -> Self {
        let s = |v: i64| v.to_string();
        Self {
            id: s(c.id),
            kind: c.kind.clone(),
            object_kind: c.object_kind.clone(),
            sr_object_id: c.sr_object_id.map(s),
            discord_object_id: c.discord_object_id.map(s),
            target_id: c.target_id.map(s),
            field: c.field.clone(),
            discord_value: c.discord_value.clone(),
            sr_value: c.sr_value.clone(),
            discord_actor_id: c.discord_actor_id.map(s),
            discord_actor_name: c.discord_actor_name.clone(),
            count: c.count,
            status: c.status.clone(),
            resolution: c.resolution.clone(),
            resolved_by: c.resolved_by.map(s),
            resolved_at: c.resolved_at_ms,
            first_at: c.first_at_ms,
            last_at: c.last_at_ms,
        }
    }
}

/// A page of the conflict log, newest first.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConflictList {
    pub conflicts: Vec<ConflictObject>,
    /// Conflicts still open (all pages).
    pub open: i64,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ConflictQuery {
    /// Filter by kind.
    pub kind: Option<String>,
    /// `open` (default), `resolved`, `dismissed`, `superseded` or `all`.
    pub status: Option<String>,
    pub object_kind: Option<String>,
    /// Page: rows older than this conflict id.
    pub before: Option<String>,
    /// 1 to 100, default 50.
    pub limit: Option<i64>,
}

fn path_ids(guild_id: &str, conflict_id: &str) -> ApiResult<i64> {
    let _ = guild_id;
    parse_id::<i64>("conflict_id", conflict_id)
}

/// The conflicts of the bridge (`MANAGE_GUILD`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/bridge/conflicts",
    params(("guild_id" = String, Path, description = "Guild snowflake"), ConflictQuery),
    responses((status = 200, body = ConflictList), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_conflicts(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path(guild_id): Path<String>,
    Query(q): Query<ConflictQuery>,
) -> ApiResult<Json<ConflictList>> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let kind = match q.kind.as_deref() {
        None => None,
        Some(k) => Some(ConflictKind::parse(k).ok_or_else(|| {
            invalid("kind", "ENUM_TYPE_INVALID", "Unknown conflict kind.")
        })?),
    };
    let object_kind = match q.object_kind.as_deref() {
        None => None,
        Some(k) => Some(ObjectKind::parse(k).ok_or_else(|| {
            invalid("object_kind", "ENUM_TYPE_INVALID", "Unknown object kind.")
        })?),
    };
    let status = match q.status.as_deref() {
        None => Some("open".to_owned()),
        Some("all") => None,
        Some(s @ ("open" | "resolved" | "dismissed" | "superseded")) => Some(s.to_owned()),
        Some(_) => {
            return Err(invalid(
                "status",
                "ENUM_TYPE_INVALID",
                "Status must be open, resolved, dismissed, superseded or all.",
            ));
        }
    };
    let before = match q.before.as_deref() {
        Some(b) => Some(parse_id::<i64>("before", b)?),
        None => None,
    };
    let filter = ConflictFilter {
        kind,
        status,
        object_kind,
        sr_object_id: None,
        before,
        limit: q.limit.unwrap_or(50).clamp(1, 100),
    };
    let rows = bridge.list_conflicts(c.gid, &filter).await.map_err(map_err)?;
    let open = bridge.open_conflict_count(c.gid).await.map_err(map_err)?;
    Ok(Json(ConflictList {
        conflicts: rows.iter().map(ConflictObject::from).collect(),
        open,
    }))
}

/// One conflict (`MANAGE_GUILD`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/bridge/conflicts/{conflict_id}",
    params(("guild_id" = String, Path, description = "Guild snowflake"), ("conflict_id" = String, Path, description = "Conflict snowflake")),
    responses((status = 200, body = ConflictObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_conflict(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path((guild_id, conflict_id)): Path<(String, String)>,
) -> ApiResult<Json<ConflictObject>> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let cid = path_ids(&guild_id, &conflict_id)?;
    let row = bridge.get_conflict(c.gid, cid).await.map_err(map_err)?;
    Ok(Json(ConflictObject::from(&row)))
}

/// Dismiss a conflict (`MANAGE_GUILD`): it stays in the log as dismissed. Nothing on either side changes.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/bridge/conflicts/{conflict_id}/dismiss",
    params(("guild_id" = String, Path, description = "Guild snowflake"), ("conflict_id" = String, Path, description = "Conflict snowflake")),
    responses((status = 200, body = ConflictObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn dismiss_conflict(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path((guild_id, conflict_id)): Path<(String, String)>,
) -> ApiResult<Json<ConflictObject>> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let cid = path_ids(&guild_id, &conflict_id)?;
    let row = bridge
        .resolve_conflict(c.gid, cid, c.user, Resolution::Dismissed)
        .await
        .map_err(map_err)?;
    Ok(Json(ConflictObject::from(&row)))
}

/// The SIDE ROOM permission the edit behind a row needs.
fn edit_permission(c: &Conflict) -> Permissions {
    match c.object_kind.as_str() {
        "channel" | "category" | "thread" => Permissions::MANAGE_CHANNELS,
        _ => Permissions::MANAGE_ROLES,
    }
}

/// Close a conflict after "Apply on SIDE ROOM" (`MANAGE_GUILD`, and `MANAGE_ROLES` or `MANAGE_CHANNELS` for the
/// edit the row stands for). The edit itself was an ordinary SIDE ROOM edit by this person; this only records it.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/bridge/conflicts/{conflict_id}/resolve",
    params(("guild_id" = String, Path, description = "Guild snowflake"), ("conflict_id" = String, Path, description = "Conflict snowflake")),
    responses((status = 200, body = ConflictObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn resolve_conflict(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path((guild_id, conflict_id)): Path<(String, String)>,
) -> ApiResult<Json<ConflictObject>> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let cid = path_ids(&guild_id, &conflict_id)?;
    let row = bridge.get_conflict(c.gid, cid).await.map_err(map_err)?;
    if !c.acc.perms.has(edit_permission(&row)) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let row = bridge
        .resolve_conflict(c.gid, cid, c.user, Resolution::AppliedOnSideRoom)
        .await
        .map_err(map_err)?;
    Ok(Json(ConflictObject::from(&row)))
}

/// What "Take Discord's" applies.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TakeDiscordBody {}

/// Apply Discord's value of a `structure_conflict` row here, as the person who clicks (`MANAGE_GUILD`, plus the
/// SIDE ROOM rule of the edit: `MANAGE_ROLES` and hierarchy for a role, `MANAGE_CHANNELS` for a channel).
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/bridge/conflicts/{conflict_id}/take-discord",
    params(("guild_id" = String, Path, description = "Guild snowflake"), ("conflict_id" = String, Path, description = "Conflict snowflake")),
    request_body = TakeDiscordBody,
    responses((status = 200, body = ConflictObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn take_discord(
    SessionOnly(p): SessionOnly,
    Api(api): Api,
    Ext(deps): Ext<SettingsDeps>,
    Ext(bridge): Ext<BridgeService>,
    Path((guild_id, conflict_id)): Path<(String, String)>,
    ApiJson(_body): ApiJson<TakeDiscordBody>,
) -> ApiResult<Json<ConflictObject>> {
    let c = bridge_admin(&p, &deps, &bridge, &guild_id).await?;
    let cid = path_ids(&guild_id, &conflict_id)?;
    let row = bridge.get_conflict(c.gid, cid).await.map_err(map_err)?;
    if row.kind != ConflictKind::StructureConflict.as_str() || row.status != "open" {
        return Err(ApiError::with_message(
            ErrorCode::SrConflict,
            "Only an open structure conflict can take Discord's value.",
        )
        .into());
    }
    if !c.acc.perms.has(edit_permission(&row)) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let user = actor(&p)?;
    let gid: GuildId = parse_id("guild_id", &guild_id)?;
    let target = row
        .sr_object_id
        .and_then(|v| u64::try_from(v).ok())
        .ok_or_else(|| invalid("conflict", "BRIDGE_INVALID", "The conflict names no SIDE ROOM object."))?;
    let value = row.discord_value.clone().unwrap_or(Value::Null);
    let bad = || {
        invalid(
            "field",
            "BRIDGE_INVALID",
            "Discord's value of this field cannot be applied from here.",
        )
    };
    match (row.object_kind.as_str(), row.field.as_str()) {
        ("role", f @ ("name" | "color" | "hoist" | "mentionable")) => {
            let mut patch = RolePatch::default();
            match f {
                "name" => patch.name = Some(value.as_str().ok_or_else(bad)?.to_owned()),
                "color" => {
                    patch.color = Some(
                        u32::try_from(value.as_u64().ok_or_else(bad)?).map_err(|_| bad())?,
                    )
                }
                "hoist" => patch.hoist = Some(value.as_bool().ok_or_else(bad)?),
                _ => patch.mentionable = Some(value.as_bool().ok_or_else(bad)?),
            }
            api.edit_role(user, gid, RoleId::new(target), patch)
                .await
                .map_err(ce)?;
        }
        ("channel" | "category", f @ ("name" | "topic")) => {
            let mut patch = ChannelPatch::default();
            match f {
                "name" => patch.name = Some(value.as_str().ok_or_else(bad)?.to_owned()),
                _ => patch.topic = Some(value.as_str().map(str::to_owned)),
            }
            api.edit_channel(user, ChannelId::new(target), patch, ChannelMove::default())
                .await
                .map_err(ce)?;
        }
        _ => return Err(bad()),
    }
    let row = bridge
        .resolve_conflict(c.gid, cid, c.user, Resolution::TookDiscord)
        .await
        .map_err(map_err)?;
    Ok(Json(ConflictObject::from(&row)))
}

/// OpenAPI fragment of the conflict routes.
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        list_conflicts,
        get_conflict,
        dismiss_conflict,
        resolve_conflict,
        take_discord,
    ),
    components(schemas(ErrorBody, ConflictObject, ConflictList, TakeDiscordBody))
)]
pub struct BridgeConflictsApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    <BridgeConflictsApiDoc as utoipa::OpenApi>::openapi()
}

#[cfg(test)]
mod tests {
    use sr_bridge::sync::conflicts as c;
    use sr_chat::settings::audit::AuditAction;

    /// The bridge crate writes audit rows by number; the numbers are the `AuditAction` ones.
    #[test]
    fn the_bridge_audit_numbers_are_the_audit_actions() {
        assert_eq!(c::AUDIT_MODE_UPDATE, AuditAction::BridgeModeUpdate.code());
        assert_eq!(c::AUDIT_SYNC_RUN, AuditAction::BridgeSyncRun.code());
        assert_eq!(
            c::AUDIT_PERMISSION_REVERTED,
            AuditAction::BridgePermissionReverted.code()
        );
        assert_eq!(c::AUDIT_CONFLICT, AuditAction::BridgeConflict.code());
        assert_eq!(
            c::AUDIT_OBJECT_REMOVED,
            AuditAction::BridgeObjectRemoved.code()
        );
    }
}
