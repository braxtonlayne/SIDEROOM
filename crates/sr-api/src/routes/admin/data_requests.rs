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
use sr_admin::{
    DataRequest, DataRequestKind, ErasureOptions, ErasureReport, RetainedRecord, retained,
};
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

async fn compliance(ctx: &StaffCtx) -> ComplianceSettings {
    let Some(f) = &ctx.deps.features else {
        return ComplianceSettings::default();
    };
    let values = f
        .effective_settings(COMPLIANCE_ID, &ScopeRef::Platform)
        .await
        .unwrap_or_default();
    ComplianceSettings::from_values(&values)
}

async fn deadline_days(ctx: &StaffCtx) -> u16 {
    compliance(ctx).await.data_request_deadline_days
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
        retained_catalog: retained(compliance(&ctx).await.invoice_retention_years),
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
pub async fn get_one(
    ctx: StaffCtx,
    Path(request_id): Path<String>,
) -> ApiResult<Json<DataRequest>> {
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

/// Body of `POST /admin/data-requests/{id}/run` (optional).
#[derive(Debug, Default, Deserialize, ToSchema)]
#[schema(as = AdminRunDataRequestBody)]
pub struct RunBody {
    /// Erasure only: the member also asked for their messages to be deleted (N-06, D-324). The
    /// member's message self-delete is queued with servers and direct messages, no cut-off.
    #[serde(default)]
    pub delete_messages: bool,
}

/// Run a verified request. An export queues the self-service data package for the person (they
/// download it from Settings for 7 days); an erasure deletes the account through account
/// deletion and completes with an outcome that names the records that stay (D-124, G-66).
#[utoipa::path(
    post, path = "/admin/data-requests/{request_id}/run",
    params(("request_id" = String, Path, description = "Data request snowflake")),
    request_body(content = RunBody, description = "Optional"),
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
    body: Option<Json<RunBody>>,
) -> ApiResult<Json<DataRequest>> {
    let rid = id(&request_id)?;
    let delete_messages = body.is_some_and(|b| b.0.delete_messages);
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
                .run_data_erasure(
                    &ctx.caller,
                    rid,
                    ErasureOptions {
                        delete_messages,
                        invoice_years: compliance(&ctx).await.invoice_retention_years,
                    },
                )
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

/// The original username, email and display name of a deleted account, from the staff-only archive
/// (`MANAGE_ACCOUNTS`, D-326). Each view is written to the access log.
#[utoipa::path(
    get, path = "/admin/accounts/{account_id}/deleted-archive",
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = sr_admin::DeletedArchive), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn deleted_archive(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
) -> ApiResult<Json<sr_admin::DeletedArchive>> {
    ctx.svc()
        .view_deleted_archive(&ctx.caller, id(&account_id)?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Body of `POST /admin/data-requests/{id}/restriction`.
#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = AdminRestrictDataRequestBody)]
pub struct RestrictBody {
    /// `true` hides the account's shared activity from discovery while the request is reviewed; `false` shows it
    /// again.
    pub hide: bool,
    /// Why (required, kept in the access log).
    pub reason: Option<String>,
}

/// Restriction or objection (GDPR articles 18 and 21, MOD-DSR-006): hide the requester's shared activity (cinema,
/// library sharing, watching presence, recommendations) from discovery while the request is reviewed, or
/// show it again. Nothing is deleted. It also ends when the request is completed or declined. 409 when the request
/// is closed or already in that state.
#[utoipa::path(
    post, path = "/admin/data-requests/{request_id}/restriction", request_body = RestrictBody,
    params(("request_id" = String, Path, description = "Data request snowflake")),
    responses((status = 200, body = DataRequest), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn restriction(
    ctx: StaffCtx,
    Path(request_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<RestrictBody>,
) -> ApiResult<Json<DataRequest>> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .set_data_request_restriction(&ctx.caller, id(&request_id)?, b.hide, r.as_deref())
        .await
        .map(Json)
        .map_err(map_err)
}

/// Body of `POST /admin/data-requests/{id}/rectify`.
#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = AdminRectifyDataRequestBody)]
pub struct RectifyBody {
    /// Put the requester's account under this Person (decimal snowflake); absent splits the account off into a
    /// Person of its own.
    pub person_id: Option<String>,
    /// Why the link was wrong (required, kept in the access log).
    pub reason: Option<String>,
}

/// The Person the account is under after a rectification.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AdminRectifyDataRequestResult)]
pub struct RectifyResult {
    pub person_id: String,
}

/// Rectification of a Person link (MOD-DSR-007, spec section 6): correct which Person the requester's account is
/// linked to, with a logged reason. Needs `MANAGE_TAKEDOWNS` and `MANAGE_ACCOUNTS`; refused while a ban involves
/// either Person.
#[utoipa::path(
    post, path = "/admin/data-requests/{request_id}/rectify", request_body = RectifyBody,
    params(("request_id" = String, Path, description = "Data request snowflake")),
    responses((status = 200, body = RectifyResult), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn rectify(
    ctx: StaffCtx,
    Path(request_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<RectifyBody>,
) -> ApiResult<Json<RectifyResult>> {
    let r = reason(&headers, b.reason.as_deref());
    let to = b.person_id.as_deref().map(id).transpose()?;
    ctx.svc()
        .rectify_person_link(&ctx.caller, id(&request_id)?, to, r.as_deref())
        .await
        .map(|p| {
            Json(RectifyResult {
                person_id: p.to_string(),
            })
        })
        .map_err(map_err)
}
