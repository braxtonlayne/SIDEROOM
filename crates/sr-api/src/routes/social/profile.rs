//! User profiles and the caller's own profile edit (LNG-PRF-*, Q118).

use super::deps::SocialDeps;
use super::error::{ApiJson, MsgResult};
use super::events::SocialEvent;
use super::model::{
    ConnectionObject, CustomStatusObject, EditSelfBody, ProfileObject, SelfObject,
    UserProfileFields,
};
use super::util::{actor, custom_status, user_id};
use crate::{Principal, openapi::ErrorBody, routes::Ext};
use axum::{Json, extract::Path};
use serde_json::Value;
use sr_chat::social::profile::ProfileEdit;
use sr_core::UserId;

/// A user's profile: bio, pronouns, banner, accent colour and the connections they
/// made visible.
#[utoipa::path(
    get,
    path = "/users/{user_id}/profile",
    params(("user_id" = String, Path, description = "User snowflake")),
    responses((status = 200, body = ProfileObject), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_profile(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Path(target): Path<String>,
) -> MsgResult<Json<ProfileObject>> {
    let me = actor(&p)?;
    render_profile(&deps, me, user_id(&target)?).await
}

/// The caller's own profile.
#[utoipa::path(
    get,
    path = "/users/@me/profile",
    responses((status = 200, body = ProfileObject)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_own_profile(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
) -> MsgResult<Json<ProfileObject>> {
    let me = actor(&p)?;
    render_profile(&deps, me, me).await
}

async fn render_profile(
    deps: &SocialDeps,
    viewer: UserId,
    user: UserId,
) -> MsgResult<Json<ProfileObject>> {
    let profile = deps.service.get_profile(viewer, user).await?;
    Ok(Json(ProfileObject {
        user: deps.user_object(user).await?,
        user_profile: UserProfileFields::from(&profile),
        connected_accounts: profile
            .connections
            .iter()
            .map(ConnectionObject::from)
            .collect(),
    }))
}

/// Edit the caller's profile: `bio` (190), `pronouns` (40), `banner`, `accent_color`
/// and `custom_status`. Absent fields stay, `null` clears.
#[utoipa::path(
    patch,
    path = "/users/@me",
    request_body = EditSelfBody,
    responses((status = 200, body = SelfObject), (status = 400, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn edit_self(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    ApiJson(body): ApiJson<EditSelfBody>,
) -> MsgResult<Json<SelfObject>> {
    let me = actor(&p)?;
    // Convert before writing so a bad custom status cannot leave a half-applied edit.
    let custom = match &body.custom_status {
        Some(Some(c)) => Some(custom_status(c)?),
        Some(None) => Some(Default::default()),
        None => None,
    };
    let edit = ProfileEdit {
        banner_key: body.banner,
        accent_color: body.accent_color,
        bio: body.bio,
        pronouns: body.pronouns,
    };
    let profile = deps.service.edit_profile(me, &edit).await?;
    let prefs = match custom {
        Some(c) => deps.service.set_custom_status(me, c).await?,
        None => deps.service.presence(me).await?,
    };
    let out = SelfObject {
        user: deps.user_object(me).await?,
        bio: profile.bio.clone(),
        pronouns: profile.pronouns.clone(),
        banner: profile.banner_key.clone(),
        accent_color: profile.accent_color,
        custom_status: CustomStatusObject::new(&prefs.custom),
    };
    // TODO(social-events): also fan USER_UPDATE out to friends and shared-guild members.
    deps.emit(SocialEvent::new(
        "USER_UPDATE",
        vec![me],
        serde_json::to_value(&out).unwrap_or(Value::Null),
    ))
    .await;
    Ok(Json(out))
}
