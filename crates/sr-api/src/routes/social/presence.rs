//! The caller's chosen presence (Q119): status, optional DND/idle expiry, custom
//! status. Live per-session merging happens in the gateway.

use super::deps::SocialDeps;
use super::error::{ApiJson, MsgResult, invalid};
use super::events::SocialEvent;
use super::model::{PresenceBody, PresenceObject};
use super::util::{actor, custom_status};
use crate::{Principal, openapi::ErrorBody, routes::Ext};
use axum::Json;
use serde_json::Value;
use sr_chat::social::presence::Status;

/// The stored presence preference with expiries applied.
#[utoipa::path(
    get,
    path = "/users/@me/presence",
    responses((status = 200, body = PresenceObject)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_presence(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
) -> MsgResult<Json<PresenceObject>> {
    let me = actor(&p)?;
    let prefs = deps.service.presence(me).await?;
    Ok(Json(PresenceObject::from(&prefs)))
}

/// Choose `online`, `idle`, `dnd` or `invisible` (optionally for
/// `duration_seconds`) and set or clear the custom status.
#[utoipa::path(
    patch,
    path = "/users/@me/presence",
    request_body = PresenceBody,
    responses((status = 200, body = PresenceObject), (status = 400, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn patch_presence(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    ApiJson(body): ApiJson<PresenceBody>,
) -> MsgResult<Json<PresenceObject>> {
    let me = actor(&p)?;
    let status = match body.status.as_deref() {
        None => None,
        Some("online") => Some(Status::Online),
        Some("idle") => Some(Status::Idle),
        Some("dnd") => Some(Status::Dnd),
        Some("invisible") => Some(Status::Invisible),
        Some(_) => {
            return Err(invalid(
                "status",
                "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
                "Must be online, idle, dnd or invisible.",
            ));
        }
    };
    let custom = match &body.custom_status {
        Some(Some(c)) => Some(custom_status(c)?),
        Some(None) => Some(Default::default()),
        None => None,
    };
    let duration_ms = body.duration_seconds.map(|s| s.saturating_mul(1000));
    let mut prefs = deps.service.presence(me).await?;
    if let Some(s) = status {
        prefs = deps.service.set_status(me, s, duration_ms).await?;
    }
    if let Some(c) = custom {
        prefs = deps.service.set_custom_status(me, c).await?;
    }
    let out = PresenceObject::from(&prefs);
    // TODO(social-events): PRESENCE_UPDATE to friends and guild members is raised by
    // the gateway's presence map once it reads these preferences.
    deps.emit(SocialEvent::new(
        "USER_SETTINGS_UPDATE",
        vec![me],
        serde_json::to_value(&out).unwrap_or(Value::Null),
    ))
    .await;
    Ok(Json(out))
}
