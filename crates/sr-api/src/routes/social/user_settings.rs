//! Per-user settings: the synced settings blob, the global default notification
//! level, and DM / friend-request privacy (LNG-USET-002, LNG-USET-013/014, LNG-NTF-001).

use super::deps::SocialDeps;
use super::error::{ApiJson, MsgResult, invalid};
use super::events::SocialEvent;
use super::model::nullable;
use super::util::actor;
use crate::{Principal, openapi::ErrorBody, routes::Ext};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sr_chat::social::notifications::NotificationLevel;
use sr_chat::social::repo::Privacy;
use sr_chat::social::user_settings::{PrivacyUpdate, UserSettings};
use utoipa::ToSchema;

/// The synced settings blob.
#[derive(Debug, Serialize, ToSchema)]
pub struct UserSettingsObject {
    /// Bumps on every save; 0 when nothing was ever saved.
    pub version: i64,
    /// Client-owned JSON object.
    #[schema(value_type = Object)]
    pub settings: Value,
}

impl From<UserSettings> for UserSettingsObject {
    fn from(s: UserSettings) -> Self {
        Self {
            version: s.version,
            settings: s.data,
        }
    }
}

/// Body of `PATCH /users/@me/settings`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchUserSettingsBody {
    /// A JSON Merge Patch (RFC 7386): objects merge key by key, `null` deletes a key,
    /// other values replace. The merged blob may not exceed 64 KiB, 8 levels of nesting
    /// or 256 top-level keys.
    #[schema(value_type = Object)]
    pub settings: Value,
}

/// The caller's synced settings blob (empty at version 0 until the first save).
#[utoipa::path(
    get,
    path = "/users/@me/settings",
    responses((status = 200, body = UserSettingsObject)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_settings(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
) -> MsgResult<Json<UserSettingsObject>> {
    let me = actor(&p)?;
    Ok(Json(deps.service.user_settings(me).await?.into()))
}

/// Merge a partial update into the settings blob. Publishes `SR_USER_SETTINGS_UPDATE`
/// to the caller's other sessions.
#[utoipa::path(
    patch,
    path = "/users/@me/settings",
    request_body = PatchUserSettingsBody,
    responses((status = 200, body = UserSettingsObject), (status = 400, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn patch_settings(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    ApiJson(body): ApiJson<PatchUserSettingsBody>,
) -> MsgResult<Json<UserSettingsObject>> {
    let me = actor(&p)?;
    let saved = deps.service.patch_user_settings(me, &body.settings).await?;
    let out = UserSettingsObject::from(saved);
    deps.emit(SocialEvent::new(
        "SR_USER_SETTINGS_UPDATE",
        vec![me],
        serde_json::to_value(&out).unwrap_or(Value::Null),
    ))
    .await;
    Ok(Json(out))
}

/// The caller's own default notification level.
#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationDefaultsObject {
    /// 0 all messages, 1 only @mentions, 2 nothing; `null` follows each server's default.
    pub message_notifications: Option<u8>,
}

/// Body of `PATCH /users/@me/notification-defaults`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchNotificationDefaultsBody {
    /// 0, 1 or 2; `null` goes back to following each server's default.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<u8>)]
    pub message_notifications: Option<Option<i16>>,
}

fn defaults_object(level: Option<NotificationLevel>) -> NotificationDefaultsObject {
    NotificationDefaultsObject {
        message_notifications: level.map(|l| u8::try_from(l.code()).unwrap_or(0)),
    }
}

/// The caller's global default notification level. It applies to servers where
/// neither the server nor a channel has an explicit choice, and takes precedence
/// over the server owner's suggested default.
#[utoipa::path(
    get,
    path = "/users/@me/notification-defaults",
    responses((status = 200, body = NotificationDefaultsObject)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_notification_defaults(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
) -> MsgResult<Json<NotificationDefaultsObject>> {
    let me = actor(&p)?;
    Ok(Json(defaults_object(
        deps.service.default_notification_level(me).await?,
    )))
}

/// Set (or clear with `null`) the global default notification level. Publishes
/// `SR_NOTIFICATION_DEFAULTS_UPDATE` to the caller's sessions.
#[utoipa::path(
    patch,
    path = "/users/@me/notification-defaults",
    request_body = PatchNotificationDefaultsBody,
    responses((status = 200, body = NotificationDefaultsObject), (status = 400, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn patch_notification_defaults(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    ApiJson(body): ApiJson<PatchNotificationDefaultsBody>,
) -> MsgResult<Json<NotificationDefaultsObject>> {
    let me = actor(&p)?;
    let Some(change) = body.message_notifications else {
        return get_notification_defaults(p, Ext(deps)).await;
    };
    let level = match change {
        None => None,
        Some(code) => Some(NotificationLevel::from_code(code).ok_or_else(|| {
            invalid(
                "message_notifications",
                "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
                "Must be 0 (all), 1 (only mentions) or 2 (nothing).",
            )
        })?),
    };
    deps.service
        .set_default_notification_level(me, level)
        .await?;
    let out = defaults_object(level);
    deps.emit(SocialEvent::new(
        "SR_NOTIFICATION_DEFAULTS_UPDATE",
        vec![me],
        serde_json::to_value(&out).unwrap_or(Value::Null),
    ))
    .await;
    Ok(Json(out))
}

/// Who may send the caller friend requests and DMs.
#[derive(Debug, Serialize, ToSchema)]
pub struct PrivacyObject {
    /// Friend requests from anyone.
    pub friend_requests_everyone: bool,
    /// Friend requests from friends of friends.
    pub friend_requests_friends_of_friends: bool,
    /// Friend requests from members of a shared server.
    pub friend_requests_server_members: bool,
    /// DMs from members of shared servers (default on).
    pub allow_server_member_dms: bool,
    /// DMs from non-friends land in Message Requests (default on).
    pub message_requests: bool,
}

impl From<Privacy> for PrivacyObject {
    fn from(p: Privacy) -> Self {
        Self {
            friend_requests_everyone: p.friend_requests.everyone,
            friend_requests_friends_of_friends: p.friend_requests.friends_of_friends,
            friend_requests_server_members: p.friend_requests.server_members,
            allow_server_member_dms: p.dms.allow_server_member_dms,
            message_requests: p.dms.message_requests,
        }
    }
}

/// Body of `PATCH /users/@me/privacy`; absent fields stay.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct PatchPrivacyBody {
    pub friend_requests_everyone: Option<bool>,
    pub friend_requests_friends_of_friends: Option<bool>,
    pub friend_requests_server_members: Option<bool>,
    pub allow_server_member_dms: Option<bool>,
    pub message_requests: Option<bool>,
}

/// The caller's DM and friend-request privacy settings.
#[utoipa::path(
    get,
    path = "/users/@me/privacy",
    responses((status = 200, body = PrivacyObject)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_privacy(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
) -> MsgResult<Json<PrivacyObject>> {
    let me = actor(&p)?;
    Ok(Json(deps.service.privacy(me).await?.into()))
}

/// Change DM and friend-request privacy. Takes effect for new requests and new
/// conversations; existing friendships and open DMs are untouched. Publishes
/// `SR_PRIVACY_UPDATE` to the caller's sessions.
#[utoipa::path(
    patch,
    path = "/users/@me/privacy",
    request_body = PatchPrivacyBody,
    responses((status = 200, body = PrivacyObject)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn patch_privacy(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    ApiJson(body): ApiJson<PatchPrivacyBody>,
) -> MsgResult<Json<PrivacyObject>> {
    let me = actor(&p)?;
    let saved = deps
        .service
        .update_privacy(
            me,
            PrivacyUpdate {
                friend_everyone: body.friend_requests_everyone,
                friend_of_friends: body.friend_requests_friends_of_friends,
                friend_server_members: body.friend_requests_server_members,
                allow_server_member_dms: body.allow_server_member_dms,
                message_requests: body.message_requests,
            },
        )
        .await?;
    let out = PrivacyObject::from(saved);
    deps.emit(SocialEvent::new(
        "SR_PRIVACY_UPDATE",
        vec![me],
        serde_json::to_value(&out).unwrap_or(Value::Null),
    ))
    .await;
    Ok(Json(out))
}
