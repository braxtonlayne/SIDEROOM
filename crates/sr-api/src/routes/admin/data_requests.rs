//! The data-subject request queue (M-25 step c, MOD-DSR; D-295, spec section 6).
//!
//! `MANAGE_TAKEDOWNS` (D-210). The due date is computed from
//! `compliance.data_request_deadline_days` when a request is entered; one extension is allowed
//! (`POST /admin/data-requests/{id}/extend`). Every change is written to the staff access log by
//! [`sr_admin::AdminService`]; these handlers translate HTTP and, after an erasure, finish the
//! media clean-up and live notices exactly as the self-service deletion does.

use super::ctx::{ReasonBody, StaffCtx, id, map_err, reason};
use crate::{
    error::ApiResult,
    openapi::ErrorBody,
    routes::{MaybeExt, media::MediaDeps, social::SocialDeps},
};
use axum::{
    Json,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use sr_admin::{DataRequest, DataRequestKind, ErasureReport, RETAINED, RetainedRecord};
use sr_features::{COMPLIANCE_ID, ComplianceSettings, ScopeRef};
use utoipa::ToSchema;

/// The queue with the settings the page needs.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminDataRequestList)]
pub struct DataRequestList {
    pub items: Vec<DataRequest>,
    /// `compliance.data_request_deadline_days` today (new requests use it).
    pub deadline_days: u16,
    /// The records an erasure keeps, so the page can name them (G-66, D-295).
    pub retained_catalog: Vec<RetainedRecord>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Only requests that still need staff (default true).
    pub active_only: Option<bool>,
    pub limit: Option<i64>,
}

/// Body of `POST /admin/data-requests`.
#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = AdminCreateDataRequestBody)]
pub struct CreateBody {
    pub kind: DataRequestKind,
    /// The account the request is about (decimal snowflake).
    pub account_id: String,
    /// When it arrived, Unix milliseconds; defaults to now.
    pub received_at_ms: Option<i64>,
    /// Free text: how it arrived, what was asked (at most 1000 characters).
    pub note: Option<String>,
}

/// Body of `POST /admin/data-requests/{id}/extend`.
#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = AdminExtendDataRequestBody)]
pub struct ExtendBody {
    /// 1 to 60 days added to the original due date. Allowed once.
    pub days: i64,
    /// Why the request is complex or numerous (required, kept in the access log).
    pub reason: Option<String>,
}

async fn deadline_days(ctx: &StaffCtx) -> u16 {
    let Some(f) = &ctx.deps.features else {
        return ComplianceSettings::default().data_request_deadline_days;
    };
    let values = f
        .effective_settings(COMPLIANCE_ID, &ScopeRef::Platform)
        .await
        .unwrap_or_default();
    ComplianceSettings::from_values(&values).data_request_deadline_days
}

/// The queue, active requests by due date first (`MANAGE_TAKEDOWNS`).
#[utoipa::path(
    get, path = "/admin/data-requests",
    params(
        ("active_only" = Option<bool>, Query, description = "Only requests that still need staff (default true)"),
        ("limit" = Option<i64>, Query, description = "At most 200"),
    ),
    responses((status = 200, body = DataRequestList), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(ctx: StaffCtx, Query(q): Query<ListQuery>) -> ApiResult<Json<DataRequestList>> {
    let items = ctx
        .svc()
        .list_data_requests(&ctx.caller, q.active_only.unwrap_or(true), q.limit)
        .await
        .map_err(map_err)?;
    Ok(Json(DataRequestList {
        items,
        deadline_days: deadline_days(&ctx).await,
        retained_catalog: RETAINED.to_vec(),
    }))
}

/// Enter a request that arrived by email or form; the due date is computed now.
#[utoipa::path(
    post, path = "/admin/data-requests", request_body = CreateBody,
    responses((status = 201, body = DataRequest), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create(
    ctx: StaffCtx,
    Json(b): Json<CreateBody>,
) -> ApiResult<(StatusCode, Json<DataRequest>)> {
    let days = i64::from(deadline_days(&ctx).await);
    let r = ctx
        .svc()
        .create_data_request(
            &ctx.caller,
            b.kind,
            id(&b.account_id)?,
            b.received_at_ms,
            b.note.as_deref(),
            days,
        )
        .await
        .map_err(map_err)?;
    Ok((StatusCode::CREATED, Json(r)))
}

/// One request (the read is logged).
#[utoipa::path(
    get, path = "/admin/data-requests/{request_id}",
    params(("request_id" = String, Path, description = "Data request snowflake")),
    responses((status = 200, body = DataRequest), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_one(ctx: StaffCtx, Path(request_id): Path<String>) -> ApiResult<Json<DataRequest>> {
    ctx.svc()
        .get_data_request(&ctx.caller, id(&request_id)?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Record that staff checked who is asking; the reason says how.
#[utoipa::path(
    post, path = "/admin/data-requests/{request_id}/verify", request_body = ReasonBody,
    params(("request_id" = String, Path, description = "Data request snowflake")),
    responses((status = 200, body = DataRequest), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn verify(
    ctx: StaffCtx,
    Path(request_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<ReasonBody>,
) -> ApiResult<Json<DataRequest>> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .verify_data_request(&ctx.caller, id(&request_id)?, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

/// Refuse a request with a stated reason.
#[utoipa::path(
    post, path = "/admin/data-requests/{request_id}/decline", request_body = ReasonBody,
    params(("request_id" = String, Path, description = "Data request snowflake")),
    responses((status = 200, body = DataRequest), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn decline(
    ctx: StaffCtx,
    Path(request_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<ReasonBody>,
) -> ApiResult<Json<DataRequest>> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .decline_data_request(&ctx.caller, id(&request_id)?, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

/// Extend the deadline by `days`, once, with a reason (GDPR article 12(3)).
#[utoipa::path(
    post, path = "/admin/data-requests/{request_id}/extend", request_body = ExtendBody,
    params(("request_id" = String, Path, description = "Data request snowflake")),
    responses((status = 200, body = DataRequest), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn extend(
    ctx: StaffCtx,
    Path(request_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<ExtendBody>,
) -> ApiResult<Json<DataRequest>> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .extend_data_request(&ctx.caller, id(&request_id)?, b.days, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

/// Run a verified request. An export queues the self-service data package for the person (they
/// download it from Settings for 7 days); an erasure deletes the account through account
/// deletion and completes with an outcome that names the records that stay (D-124, G-66).
#[utoipa::path(
    post, path = "/admin/data-requests/{request_id}/run",
    params(("request_id" = String, Path, description = "Data request snowflake")),
    responses((status = 200, body = DataRequest), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn run(
    ctx: StaffCtx,
    MaybeExt(social): MaybeExt<SocialDeps>,
    MaybeExt(media): MaybeExt<MediaDeps>,
    Path(request_id): Path<String>,
) -> ApiResult<Json<DataRequest>> {
    let rid = id(&request_id)?;
    let existing = ctx
        .svc()
        .get_data_request(&ctx.caller, rid)
        .await
        .map_err(map_err)?;
    match existing.kind {
        DataRequestKind::Export => {
            ctx.require_reauth().await?;
            let r = ctx
                .svc()
                .run_data_export(&ctx.caller, rid)
                .await
                .map_err(map_err)?;
            ctx.svc()
                .system_dm(
                    r.account_id,
                    "Staff are preparing the copy of your data you asked for. When it is ready you can download it from Settings, Your data, for 7 days.",
                )
                .await;
            Ok(Json(r))
        }
        DataRequestKind::Erasure => {
            ctx.require_reauth().await?;
            let run = ctx
                .svc()
                .run_data_erasure(&ctx.caller, rid)
                .await
                .map_err(map_err)?;
            finish_erasure(media, social, run.request.account_id, &run.report).await;
            Ok(Json(run.request))
        }
    }
}

/// Media clean-up and live notices after account deletion, the same as the self-service route.
async fn finish_erasure(
    media: Option<std::sync::Arc<MediaDeps>>,
    social: Option<std::sync::Arc<SocialDeps>>,
    account: i64,
    report: &ErasureReport,
) {
    if report.already_deleted {
        return;
    }
    if let Some(media) = media
        && let Ok(uid) = u64::try_from(account)
    {
        for (kind, key) in [
            (sr_storage::media::MediaKind::UserAvatar, &report.avatar_key),
            (sr_storage::media::MediaKind::UserBanner, &report.banner_key),
        ] {
            if let Some(key) = key {
                media.remove(kind, uid, key).await;
            }
        }
        for key in &report.profile_media_keys {
            if let Some(hash) = key.rsplit('/').next() {
                media
                    .remove(sr_storage::media::MediaKind::ProfileMedia, uid, hash)
                    .await;
            }
        }
    }
    if let Some(social) = social {
        let done = sr_auth::store::DeletedAccount {
            guilds_left: report.guilds_left.clone(),
            friends: report.friends.clone(),
            other_relations: report.other_relations.clone(),
            credits_forfeited: report.credits_forfeited,
            invites_revoked: report.invites_revoked,
            avatar_key: report.avatar_key.clone(),
            banner_key: report.banner_key.clone(),
            profile_media_keys: report.profile_media_keys.clone(),
        };
        crate::routes::social::announce_account_deleted(&social, account, &done).await;
    }
}
