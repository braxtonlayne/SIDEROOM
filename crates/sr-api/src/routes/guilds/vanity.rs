//! Vanity invite URL (LNG-INV-004): a short code of the server's own choosing that works
//! like an invite that never expires. It is free (Discord asks for a boost level).
//!
//! * `GET /guilds/{id}/vanity-url`: the code and how many members joined through it.
//! * `PATCH /guilds/{id}/vanity-url`: claim, change or release it (`MANAGE_GUILD`).
//!
//! Resolving and accepting the code is `GET`/`POST /invites/{code}` (see the settings
//! invites module), which falls back to the vanity code when no invite has that text.

use super::util::{Api, actor, ce, id};
use crate::routes::Ext;
use crate::routes::messages::error::ApiJson;
use crate::routes::settings::deps::{SettingsDeps, re};
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{Json, Router, extract::Path, routing::get};
use serde::{Deserialize, Serialize};
use sr_chat::settings::repo::InviteRepository;
use sr_core::{ApiError, GuildId};
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/guilds/{guild_id}/vanity-url",
        get(get_vanity).patch(set_vanity),
    )
}

/// The vanity URL of a server.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct VanityObject {
    /// The code; `null` while the server has none.
    pub code: Option<String>,
    /// Members who joined through it.
    pub uses: u32,
}

/// Body of `PATCH /guilds/{id}/vanity-url`.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct VanityBody {
    /// 3 to 32 letters, digits or hyphens; `null` or an empty string releases the code.
    #[serde(default, deserialize_with = "super::dto::nullable")]
    #[schema(value_type = Option<String>)]
    pub code: Option<Option<String>>,
}

/// The vanity code of a server and its use count (`MANAGE_GUILD`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/vanity-url",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = VanityObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_vanity(
    p: Principal,
    Api(api): Api,
    Ext(settings): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<VanityObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let code = api.vanity_code(user, gid).await.map_err(ce)?;
    let uses = settings.service.repo().vanity_uses(gid).await.map_err(re)?;
    Ok(Json(VanityObject { code, uses }))
}

/// Claim, change or release the vanity code (`MANAGE_GUILD`). The code is unique across
/// servers and may not shadow an existing invite code or a reserved word; a taken code
/// answers 400 `VANITY_URL_TAKEN`. Emits `GUILD_UPDATE`.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/vanity-url",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = VanityBody,
    responses((status = 200, body = VanityObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn set_vanity(
    p: Principal,
    Api(api): Api,
    Ext(settings): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<VanityBody>,
) -> ApiResult<Json<VanityObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let Some(code) = body.code else {
        return Err(HttpError(ApiError::invalid_form().field(
            "code",
            "BASE_TYPE_REQUIRED",
            "This field is required.",
        )));
    };
    // A real invite with the same text would win over the vanity code, making it
    // unreachable.
    if let Some(c) = code.as_deref().map(str::trim).filter(|c| !c.is_empty())
        && settings
            .service
            .repo()
            .get_invite(c)
            .await
            .map_err(re)?
            .is_some()
    {
        return Err(HttpError(ApiError::invalid_form().field(
            "code",
            "VANITY_URL_TAKEN",
            "This vanity URL is already taken.",
        )));
    }
    let g = api.set_vanity_code(user, gid, code).await.map_err(ce)?;
    let uses = settings.service.repo().vanity_uses(gid).await.map_err(re)?;
    Ok(Json(VanityObject {
        code: g.vanity_code,
        uses,
    }))
}
