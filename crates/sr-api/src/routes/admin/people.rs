//! Staff views and actions on people and servers: an account's sessions and servers, the manual
//! invite freeze, Person notes and links, the suspicion history (ADM-PPL-002/004,
//! ADM-GRAPH-004) and the platform-wide server list with transfer and delete (ADM-COM-002).
//!
//! Server changes go through the same chat stack the owner uses, acting as the current owner,
//! so members hear the usual events; the staff action is written to the access log.

use super::ctx::{ReasonBody, StaffCtx, id, map_err, reason};
use crate::routes::guilds::util::{Api, ce};
use crate::{error::ApiResult, openapi::ErrorBody};
use axum::{
    Json,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sr_admin::{
    AdminGuild, AdminGuildPage, AdminGuildRef, AdminService, AdminSession, SuspicionEvent,
};
use sr_chat::admin_rules::GuildPatch;
use sr_core::{GuildId, UserId};
use utoipa::ToSchema;

fn user(raw: i64) -> UserId {
    UserId::new(u64::try_from(raw).unwrap_or(0))
}

fn guild(raw: i64) -> GuildId {
    GuildId::new(u64::try_from(raw).unwrap_or(0))
}

/// Open sessions of an account: IP, browser and times, never the token (`MANAGE_ACCOUNTS`).
/// The read is logged.
#[utoipa::path(
    get, path = "/admin/accounts/{account_id}/sessions",
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = Vec<AdminSession>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn sessions(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
) -> ApiResult<Json<Vec<AdminSession>>> {
    ctx.svc()
        .account_sessions(&ctx.caller, id(&account_id)?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Servers an account belongs to (`MANAGE_ACCOUNTS`).
#[utoipa::path(
    get, path = "/admin/accounts/{account_id}/guilds",
    params(("account_id" = String, Path, description = "Account snowflake")),
    responses((status = 200, body = Vec<AdminGuildRef>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn account_guilds(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
) -> ApiResult<Json<Vec<AdminGuildRef>>> {
    ctx.svc()
        .account_guilds(&ctx.caller, id(&account_id)?)
        .await
        .map(Json)
        .map_err(map_err)
}

async fn freeze(
    ctx: StaffCtx,
    account_id: String,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
    frozen: bool,
) -> ApiResult<StatusCode> {
    let r = reason(&headers, body.and_then(|Json(b)| b.reason).as_deref());
    ctx.svc()
        .set_invites_frozen(&ctx.caller, id(&account_id)?, frozen, r.as_deref())
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Freeze the account's invite creation by hand (`MANAGE_INVITES`, reason required). The member
/// sees the generic "invites are unavailable" wording.
#[utoipa::path(
    post, path = "/admin/accounts/{account_id}/freeze-invites",
    params(("account_id" = String, Path, description = "Account snowflake")),
    request_body(content = ReasonBody, description = "Reason (or X-Audit-Log-Reason header)"),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn freeze_invites(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<StatusCode> {
    freeze(ctx, account_id, headers, body, true).await
}

/// Lift the manual invite freeze (`MANAGE_INVITES`, reason required). An automatic suspicion
/// freeze stays until the suspicion is cleared.
#[utoipa::path(
    post, path = "/admin/accounts/{account_id}/unfreeze-invites",
    params(("account_id" = String, Path, description = "Account snowflake")),
    request_body(content = ReasonBody, description = "Reason (or X-Audit-Log-Reason header)"),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unfreeze_invites(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<StatusCode> {
    freeze(ctx, account_id, headers, body, false).await
}

/// Body of `PUT /admin/persons/{person_id}/notes`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PersonNotesBody {
    /// The whole note, at most 4 000 characters.
    pub notes: String,
    pub reason: Option<String>,
}

/// Replace a Person's staff notes (`MANAGE_ACCOUNTS`).
#[utoipa::path(
    put, path = "/admin/persons/{person_id}/notes", request_body = PersonNotesBody,
    params(("person_id" = String, Path, description = "Person snowflake")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn put_notes(
    ctx: StaffCtx,
    Path(person_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<PersonNotesBody>,
) -> ApiResult<StatusCode> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .set_person_notes(&ctx.caller, id(&person_id)?, &b.notes, r.as_deref())
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Body of `POST /admin/persons/{person_id}/link`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LinkAccountBody {
    /// The account to move under this Person.
    pub account_id: String,
    pub reason: Option<String>,
}

/// Put an account under this Person (`MANAGE_ACCOUNTS`, reason required). Refused (409) while
/// either Person is banned. A Person left without accounts is merged into this one.
#[utoipa::path(
    post, path = "/admin/persons/{person_id}/link", request_body = LinkAccountBody,
    params(("person_id" = String, Path, description = "Person snowflake")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn link_account(
    ctx: StaffCtx,
    Path(person_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<LinkAccountBody>,
) -> ApiResult<StatusCode> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .link_account(
            &ctx.caller,
            id(&person_id)?,
            id(&b.account_id)?,
            r.as_deref(),
        )
        .await
        .map_err(map_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Body of `POST /admin/persons/{person_id}/identities/{identity_id}/move`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MoveIdentityBody {
    /// The account that should hold the identity. It joins this Person when it is elsewhere.
    pub to_account_id: String,
    /// Why; required, kept in the access log and told to the old account.
    pub reason: Option<String>,
}

/// What a move did.
#[derive(Debug, Serialize, ToSchema)]
pub struct MoveIdentityResult {
    pub to_account_id: String,
    /// The target account was in another Person and was linked into this one.
    pub linked_target: bool,
    /// The old account's verified address was emailed (it always gets a system DM).
    pub emailed: bool,
}

/// Move a Discord identity to another account of the Person (`MANAGE_ACCOUNTS`, fresh second factor,
/// reason required, D-262). The target joins the Person in the same step when it is elsewhere. 409 when
/// the Person or either account is banned or suspended. The old row is kept as history; the old account
/// gets a system DM, and an email when its address is verified.
#[utoipa::path(
    post, path = "/admin/persons/{person_id}/identities/{identity_id}/move", request_body = MoveIdentityBody,
    params(("person_id" = String, Path, description = "Person snowflake"),
           ("identity_id" = String, Path, description = "Identity row snowflake")),
    responses((status = 200, body = MoveIdentityResult), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn move_identity(
    ctx: StaffCtx,
    Path((person_id, identity_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(b): Json<MoveIdentityBody>,
) -> ApiResult<Json<MoveIdentityResult>> {
    let r = reason(&headers, b.reason.as_deref());
    ctx.svc()
        .authorize(&ctx.caller, sr_core::PlatformPermissions::MANAGE_ACCOUNTS, "person.identity_move")
        .await
        .map_err(map_err)?;
    ctx.require_reauth().await?;
    let to = id(&b.to_account_id)?;
    let moved = ctx
        .svc()
        .move_identity(
            &ctx.caller,
            id(&person_id)?,
            id(&identity_id)?,
            to,
            r.as_deref(),
        )
        .await
        .map_err(map_err)?;
    let why = r.as_deref().unwrap_or("").trim().to_owned();
    let emailed = ctx
        .notify_by_mail(
            moved.from_account_id,
            "Your linked Discord account was moved",
            &format!(
                "A staff member moved the Discord account linked to your SIDE ROOM account to the account @{}. Reason: {why}. If you did not expect this, contact support.",
                moved.to_username
            ),
        )
        .await;
    Ok(Json(MoveIdentityResult {
        to_account_id: to.to_string(),
        linked_target: moved.linked_target,
        emailed,
    }))
}

/// The Person created by an unlink.
#[derive(Debug, Serialize, ToSchema)]
pub struct UnlinkResult {
    pub person_id: String,
}

/// Split an account off into a Person of its own (`MANAGE_ACCOUNTS`, reason required). 409
/// when it is the only account of its Person or the Person is banned.
#[utoipa::path(
    post, path = "/admin/accounts/{account_id}/unlink",
    params(("account_id" = String, Path, description = "Account snowflake")),
    request_body(content = ReasonBody, description = "Reason (or X-Audit-Log-Reason header)"),
    responses((status = 200, body = UnlinkResult), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn unlink_account(
    ctx: StaffCtx,
    Path(account_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<Json<UnlinkResult>> {
    let r = reason(&headers, body.and_then(|Json(b)| b.reason).as_deref());
    let person = ctx
        .svc()
        .unlink_account(&ctx.caller, id(&account_id)?, r.as_deref())
        .await
        .map_err(map_err)?;
    Ok(Json(UnlinkResult {
        person_id: person.to_string(),
    }))
}

/// Query of the suspicion log.
#[derive(Debug, Default, Deserialize)]
pub struct LogParams {
    /// 1 to 200, default 50.
    pub limit: Option<i64>,
}

/// A Person's suspicion history, newest first (`MANAGE_INVITES`).
#[utoipa::path(
    get, path = "/admin/persons/{person_id}/suspicion-log",
    params(("person_id" = String, Path, description = "Person snowflake"),
           ("limit" = Option<i64>, Query, description = "1-200, default 50")),
    responses((status = 200, body = Vec<SuspicionEvent>), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn suspicion_log(
    ctx: StaffCtx,
    Path(person_id): Path<String>,
    Query(p): Query<LogParams>,
) -> ApiResult<Json<Vec<SuspicionEvent>>> {
    ctx.svc()
        .suspicion_log(&ctx.caller, id(&person_id)?, p.limit)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Query of the server list.
#[derive(Debug, Default, Deserialize)]
pub struct GuildSearch {
    /// Name fragment or exact server id.
    pub q: Option<String>,
    /// Cursor: the `next_after` of the previous page.
    pub after: Option<String>,
    /// 1 to 100, default 50.
    pub limit: Option<i64>,
}

/// Every server on the platform, by id (`MANAGE_GUILDS_GLOBAL`).
#[utoipa::path(
    get, path = "/admin/guilds",
    params(("q" = Option<String>, Query, description = "Name fragment or exact id"),
           ("after" = Option<String>, Query, description = "Cursor"),
           ("limit" = Option<i64>, Query, description = "1-100, default 50")),
    responses((status = 200, body = AdminGuildPage), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn search_guilds(
    ctx: StaffCtx,
    Query(p): Query<GuildSearch>,
) -> ApiResult<Json<AdminGuildPage>> {
    ctx.svc()
        .search_guilds(
            &ctx.caller,
            p.q.as_deref(),
            p.after.as_deref().map(id).transpose()?,
            p.limit,
        )
        .await
        .map(Json)
        .map_err(map_err)
}

/// One server's metadata (`MANAGE_GUILDS_GLOBAL`).
#[utoipa::path(
    get, path = "/admin/guilds/{guild_id}",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = AdminGuild), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_guild(ctx: StaffCtx, Path(guild_id): Path<String>) -> ApiResult<Json<AdminGuild>> {
    ctx.svc()
        .get_guild(&ctx.caller, id(&guild_id)?)
        .await
        .map(Json)
        .map_err(map_err)
}

/// Body of `POST /admin/guilds/{guild_id}/transfer-ownership`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TransferBody {
    /// A current member of the server.
    pub new_owner_id: String,
    pub reason: Option<String>,
}

/// Make another member the owner (`MANAGE_GUILDS_GLOBAL`, reason required). Applied as the
/// current owner, so the server hears the usual `GUILD_UPDATE`. The Staff Server is refused.
#[utoipa::path(
    post, path = "/admin/guilds/{guild_id}/transfer-ownership", request_body = TransferBody,
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = AdminGuild), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn transfer_ownership(
    ctx: StaffCtx,
    Api(api): Api,
    Path(guild_id): Path<String>,
    headers: HeaderMap,
    Json(b): Json<TransferBody>,
) -> ApiResult<Json<AdminGuild>> {
    let gid = id(&guild_id)?;
    let new_owner: i64 = id(&b.new_owner_id)?;
    let r = reason(&headers, b.reason.as_deref());
    let svc: &AdminService = ctx.svc();
    let (g, reason) = svc
        .authorize_guild_change(&ctx.caller, gid, "guilds.transfer", r.as_deref())
        .await
        .map_err(map_err)?;
    api.edit_guild(
        user(g.owner_id),
        guild(gid),
        GuildPatch {
            owner_id: Some(user(new_owner)),
            ..GuildPatch::default()
        },
    )
    .await
    .map_err(ce)?;
    if let Err(e) = svc
        .log_guild_action(
            &ctx.caller,
            "guilds.transfer",
            gid,
            &reason,
            json!({ "from": g.owner_id.to_string(), "to": new_owner.to_string() }),
        )
        .await
    {
        tracing::warn!(error = %e, "server transfer was not written to the access log");
    }
    // H-36: both owners hear about it from the system user, without the staff reason.
    let name = quoted(&g.name);
    let new_name = match svc.username_of(new_owner).await {
        Some(u) => quoted(&u),
        None => "the new owner".to_owned(),
    };
    svc.system_dm(
        g.owner_id,
        &format!("Staff transferred ownership of {name} to {new_name}. {CONTACT_STAFF}"),
    )
    .await;
    svc.system_dm(
        new_owner,
        &format!("You are now the owner of {name}. {CONTACT_STAFF}"),
    )
    .await;
    svc.get_guild(&ctx.caller, gid)
        .await
        .map(Json)
        .map_err(map_err)
}

/// The standing line of the system DMs about staff actions on a server.
const CONTACT_STAFF: &str = "If this looks wrong, contact staff.";

/// A name inside backticks, so a server called `@everyone` mentions nobody.
fn quoted(name: &str) -> String {
    format!("`{}`", name.replace('`', "'"))
}

/// Delete a server (`MANAGE_GUILDS_GLOBAL`, reason required). Applied as the owner: the same soft
/// delete the owner gets, with the 30-day purge window and the usual `GUILD_DELETE`. The Staff
/// Server is refused.
#[utoipa::path(
    delete, path = "/admin/guilds/{guild_id}",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body(content = ReasonBody, description = "Reason (or X-Audit-Log-Reason header)"),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_guild(
    ctx: StaffCtx,
    Api(api): Api,
    Path(guild_id): Path<String>,
    headers: HeaderMap,
    body: Option<Json<ReasonBody>>,
) -> ApiResult<StatusCode> {
    let gid = id(&guild_id)?;
    let r = reason(&headers, body.and_then(|Json(b)| b.reason).as_deref());
    let svc: &AdminService = ctx.svc();
    let (g, reason) = svc
        .authorize_guild_change(&ctx.caller, gid, "guilds.delete", r.as_deref())
        .await
        .map_err(map_err)?;
    api.delete_guild(user(g.owner_id), guild(gid))
        .await
        .map_err(ce)?;
    if let Err(e) = svc
        .log_guild_action(
            &ctx.caller,
            "guilds.delete",
            gid,
            &reason,
            json!({ "name": g.name, "owner": g.owner_id.to_string(), "members": g.member_count }),
        )
        .await
    {
        tracing::warn!(error = %e, "server delete was not written to the access log");
    }
    svc.system_dm(
        g.owner_id,
        &format!("Staff deleted {}. {CONTACT_STAFF}", quoted(&g.name)),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}
