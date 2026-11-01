//! Admin > Storage (BYOS-LEG-001, wave 28): every storage mount, its owner and status, and the whole-mount delist.
//! Staff with `MANAGE_TAKEDOWNS` only. The list never carries an endpoint or a secret, only the key hint; delisting
//! never touches the member's bucket (the platform neither may nor can delete a member's property). Single objects
//! are delisted through the takedown register (`POST /moderation/takedowns/{id}/items`, kind `storage_object`).

use axum::{
    Json,
    extract::{Path, Query},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use sr_moderation::model::Who;
use sr_moderation::{AdminMountRow, MountLegal};
use utoipa::{IntoParams, ToSchema};

use super::ctx::{ReasonBody, StaffCtx, reason as reason_from};
use crate::{
    error::ApiResult,
    openapi::ErrorBody,
    routes::{
        Ext,
        moderation::{ModerationDeps, id, map_err, staff_result},
    },
};

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct MountListParams {
    /// Only one owner kind: `user`, `guild` or `platform`.
    pub owner_kind: Option<String>,
    /// With `owner_kind`: the owner's snowflake.
    pub owner_id: Option<String>,
    /// Only one status: `active`, `failing`, `disabled` or `delisted`.
    pub status: Option<String>,
    /// 1 to 500, default 100.
    pub limit: Option<i64>,
    /// Page: only mounts with a smaller id than this one.
    pub before: Option<String>,
}

/// One mount as staff see it.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminStorageMount {
    pub id: String,
    pub owner_kind: String,
    pub owner_id: String,
    /// The owner's username, or the owning server's name; empty for the instance bucket.
    pub owner_name: String,
    pub label: String,
    pub provider: String,
    pub bucket: String,
    /// `active`, `failing`, `disabled` or `delisted`.
    pub status: String,
    /// The last four characters of the access key id.
    pub key_hint: String,
    pub created_at_ms: i64,
    pub delisted_at_ms: Option<i64>,
    pub delist_reason: String,
    /// Objects of this mount delisted by an upheld takedown notice.
    pub delisted_objects: i64,
}

impl From<AdminMountRow> for AdminStorageMount {
    fn from(r: AdminMountRow) -> Self {
        Self {
            id: r.id.to_string(),
            owner_kind: r.owner_kind,
            owner_id: r.owner_id.to_string(),
            owner_name: r.owner_name,
            label: r.label,
            provider: r.provider,
            bucket: r.bucket,
            status: r.status,
            key_hint: r.key_hint,
            created_at_ms: r.created_at_ms,
            delisted_at_ms: r.delisted_at_ms,
            delist_reason: r.delist_reason,
            delisted_objects: r.delisted_objects,
        }
    }
}

/// The answer of a delist or relist.
#[derive(Debug, Serialize, ToSchema)]
pub struct StorageMountLegal {
    pub id: String,
    pub owner_kind: String,
    pub owner_id: String,
    pub label: String,
    pub status: String,
    pub delist_reason: String,
}

impl From<MountLegal> for StorageMountLegal {
    fn from(m: MountLegal) -> Self {
        Self {
            id: m.id.to_string(),
            owner_kind: m.owner_kind,
            owner_id: m.owner_id.to_string(),
            label: m.label,
            status: m.status,
            delist_reason: m.delist_reason,
        }
    }
}

/// Every live storage mount, newest first (`MANAGE_TAKEDOWNS`).
#[utoipa::path(
    get, path = "/admin/storage/mounts", params(MountListParams),
    responses((status = 200, body = Vec<AdminStorageMount>), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    Query(p): Query<MountListParams>,
) -> ApiResult<Json<Vec<AdminStorageMount>>> {
    let owner = match (p.owner_kind.as_deref(), p.owner_id.as_deref()) {
        (None | Some(""), None | Some("")) => None,
        (Some(k @ ("user" | "guild")), Some(i)) => Some((k, id(i, "owner")?)),
        (Some("platform"), _) => Some(("platform", 0)),
        _ => {
            return Err(map_err(sr_moderation::ModError::Invalid(
                "owner_kind is user, guild or platform; user and guild need owner_id".into(),
            )));
        }
    };
    let status = match p.status.as_deref() {
        None | Some("") => None,
        Some(s @ ("active" | "failing" | "disabled" | "delisted")) => Some(s),
        Some(_) => {
            return Err(map_err(sr_moderation::ModError::Invalid(
                "status is not a mount status".into(),
            )));
        }
    };
    let before = match p.before.as_deref() {
        None | Some("") => None,
        Some(b) => Some(id(b, "mount")?),
    };
    let r = deps
        .svc
        .list_mounts(
            &Who::Staff(&ctx.caller),
            owner,
            status,
            p.limit.unwrap_or(100),
            before,
        )
        .await;
    staff_result(&ctx, "storage.mount.list", None, r)
        .await
        .map(|rows| Json(rows.into_iter().map(Into::into).collect()))
}

/// Delist a whole mount: nothing of it is served to anyone, the owner is told why, the bucket is untouched. The
/// reason is required (body or `X-Audit-Log-Reason`).
#[utoipa::path(
    post, path = "/admin/storage/mounts/{mount_id}/delist",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    request_body = ReasonBody,
    responses((status = 200, body = StorageMountLegal), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delist(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    headers: HeaderMap,
    Path(mount_id): Path<String>,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<Json<StorageMountLegal>> {
    let mid = id(&mount_id, "storage mount")?;
    let reason = reason_from(&headers, body.as_ref().and_then(|b| b.0.reason.as_deref()));
    let r = deps
        .svc
        .delist_mount(&Who::Staff(&ctx.caller), mid, reason.as_deref())
        .await;
    staff_result(
        &ctx,
        "storage.mount.delist",
        Some(("storage_mount", mid)),
        r,
    )
    .await
    .map(|m| Json(m.into()))
}

/// Relist a delisted mount (it returns to the status it had). The reason is required.
#[utoipa::path(
    post, path = "/admin/storage/mounts/{mount_id}/relist",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    request_body = ReasonBody,
    responses((status = 200, body = StorageMountLegal), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn relist(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    headers: HeaderMap,
    Path(mount_id): Path<String>,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<Json<StorageMountLegal>> {
    let mid = id(&mount_id, "storage mount")?;
    let reason = reason_from(&headers, body.as_ref().and_then(|b| b.0.reason.as_deref()));
    let r = deps
        .svc
        .relist_mount(&Who::Staff(&ctx.caller), mid, reason.as_deref())
        .await;
    staff_result(
        &ctx,
        "storage.mount.relist",
        Some(("storage_mount", mid)),
        r,
    )
    .await
    .map(|m| Json(m.into()))
}

/// Body of `POST /admin/shelves/delist`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ShelfDelistBody {
    /// The shelf (storage mount snowflake).
    pub shelf_id: String,
    /// The object's hash (64 lowercase hex characters).
    pub object_hash: String,
    /// The takedown notice this belongs to, when there is one.
    #[serde(default)]
    pub notice_id: Option<String>,
    /// Why. Required (or the `X-Audit-Log-Reason` header).
    #[serde(default)]
    pub reason: Option<String>,
}

/// The answer of a shelf-object delist or relist.
#[derive(Debug, Serialize, ToSchema)]
pub struct ShelfObjectLegal {
    pub shelf_id: String,
    pub object_hash: String,
    /// The strike row (snowflake) the delisting was logged under.
    pub strike_id: String,
    pub path: String,
}

/// Delist one object of a shelf by `object_hash` (`MANAGE_TAKEDOWNS`): it vanishes from every catalog response and
/// can no longer be minted a URL, and a strike is logged against the shelf's owner. The bucket is untouched.
#[utoipa::path(
    post, path = "/admin/shelves/delist",
    request_body = ShelfDelistBody,
    responses((status = 200, body = ShelfObjectLegal), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delist_object(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    headers: HeaderMap,
    Json(body): Json<ShelfDelistBody>,
) -> ApiResult<Json<ShelfObjectLegal>> {
    let sid = id(&body.shelf_id, "shelf")?;
    let notice = match body.notice_id.as_deref() {
        None | Some("") => None,
        Some(n) => Some(id(n, "takedown notice")?),
    };
    let reason = reason_from(&headers, body.reason.as_deref());
    let r = deps
        .svc
        .delist_shelf_object(
            &Who::Staff(&ctx.caller),
            sid,
            &body.object_hash,
            notice,
            reason.as_deref(),
        )
        .await;
    staff_result(
        &ctx,
        "storage.shelf.object.delist",
        Some(("storage_mount", sid)),
        r,
    )
    .await
    .map(|d| {
        Json(ShelfObjectLegal {
            shelf_id: body.shelf_id.clone(),
            object_hash: body.object_hash.clone(),
            strike_id: d.strike_id.to_string(),
            path: d.path,
        })
    })
}

/// Put a delisted shelf object back (`MANAGE_TAKEDOWNS`; the strike row stays, marked restored).
#[utoipa::path(
    post, path = "/admin/shelves/relist",
    request_body = ShelfDelistBody,
    responses((status = 200, body = ShelfObjectLegal), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn relist_object(
    ctx: StaffCtx,
    Ext(deps): Ext<ModerationDeps>,
    headers: HeaderMap,
    Json(body): Json<ShelfDelistBody>,
) -> ApiResult<Json<ShelfObjectLegal>> {
    let sid = id(&body.shelf_id, "shelf")?;
    let reason = reason_from(&headers, body.reason.as_deref());
    let r = deps
        .svc
        .relist_shelf_object(
            &Who::Staff(&ctx.caller),
            sid,
            &body.object_hash,
            reason.as_deref(),
        )
        .await;
    staff_result(
        &ctx,
        "storage.shelf.object.relist",
        Some(("storage_mount", sid)),
        r,
    )
    .await
    .map(|d| {
        Json(ShelfObjectLegal {
            shelf_id: body.shelf_id.clone(),
            object_hash: body.object_hash.clone(),
            strike_id: d.strike_id.to_string(),
            path: d.path,
        })
    })
}
