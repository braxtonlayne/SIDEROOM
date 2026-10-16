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
use sr_chat::social::safety::{MediaFilter, SpamFilter};
use sr_chat::social::user_settings::{PrivacyUpdate, UserSettings};
use std::collections::BTreeMap;
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
    /// How to show sensitive media in DMs, per source (LNG-USET-012).
    pub sensitive_media: SensitiveMediaObject,
    /// Flag likely spam into Message Requests (LNG-USET-013). Stored; no detector reads it yet.
    /// `off`, `non_friends` (default) or `all` (D-230).
    pub dm_spam_filter: String,
    /// Per-server override of `allow_server_member_dms`: server id to allowed.
    pub server_dm_overrides: BTreeMap<String, bool>,
}

/// Sensitive-media handling per source; each is `show`, `blur` or `block`.
#[derive(Debug, Serialize, ToSchema)]
pub struct SensitiveMediaObject {
    /// DMs from friends (default `show`).
    pub friends: String,
    /// DMs from people who are not friends (default `blur`).
    pub others: String,
    /// Messages in servers (default `show`).
    pub servers: String,
}

impl From<Privacy> for PrivacyObject {
    fn from(p: Privacy) -> Self {
        Self {
            friend_requests_everyone: p.friend_requests.everyone,
            friend_requests_friends_of_friends: p.friend_requests.friends_of_friends,
            friend_requests_server_members: p.friend_requests.server_members,
            allow_server_member_dms: p.dms.allow_server_member_dms,
            message_requests: p.dms.message_requests,
            sensitive_media: SensitiveMediaObject {
                friends: p.safety.sensitive_media.friends.as_str().to_owned(),
                others: p.safety.sensitive_media.others.as_str().to_owned(),
                servers: p.safety.sensitive_media.servers.as_str().to_owned(),
            },
            dm_spam_filter: p.safety.dm_spam_filter.as_str().to_owned(),
            server_dm_overrides: p
                .safety
                .server_dm_overrides
                .iter()
                .map(|(g, v)| (g.to_string(), *v))
                .collect(),
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
    /// Any of `friends`, `others`, `servers` (each `show`, `blur` or `block`); absent ones stay.
    pub sensitive_media: Option<PatchSensitiveMedia>,
    pub dm_spam_filter: Option<String>,
    /// Server id to `true`/`false` (set) or `null` (back to the default). At most 200 servers.
    pub server_dm_overrides: Option<BTreeMap<String, Option<bool>>>,
}

/// Partial `sensitive_media`.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct PatchSensitiveMedia {
    pub friends: Option<String>,
    pub others: Option<String>,
    pub servers: Option<String>,
}

fn media_filter(field: &str, v: Option<&String>) -> MsgResult<Option<MediaFilter>> {
    v.map(|s| {
        MediaFilter::parse(s).ok_or_else(|| {
            invalid(
                field,
                "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
                "Must be show, blur or block.",
            )
        })
    })
    .transpose()
}

type OverridePatch = BTreeMap<u64, Option<bool>>;

fn parse_overrides(
    raw: Option<BTreeMap<String, Option<bool>>>,
) -> MsgResult<Option<OverridePatch>> {
    raw.map(|m| {
        m.into_iter()
            .map(|(k, v)| {
                let id = k.parse::<u64>().ok().filter(|id| *id > 0).ok_or_else(|| {
                    invalid(
                        "server_dm_overrides",
                        "SNOWFLAKE_TYPE_INVALID",
                        "Keys must be server ids.",
                    )
                })?;
                Ok((id, v))
            })
            .collect()
    })
    .transpose()
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
    let sm = body.sensitive_media.unwrap_or_default();
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
                media_friends: media_filter("sensitive_media.friends", sm.friends.as_ref())?,
                media_others: media_filter("sensitive_media.others", sm.others.as_ref())?,
                media_servers: media_filter("sensitive_media.servers", sm.servers.as_ref())?,
                dm_spam_filter: body
                    .dm_spam_filter
                    .as_deref()
                    .map(|s| {
                        SpamFilter::parse(s).ok_or_else(|| {
                            invalid(
                                "dm_spam_filter",
                                "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
                                "Must be off, non_friends or all.",
                            )
                        })
                    })
                    .transpose()?,
                server_dm_overrides: parse_overrides(body.server_dm_overrides)?,
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

/// The caller's highlight keywords.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NotificationKeywordsObject {
    /// Words or phrases (at most 50, 100 characters each). A message containing one as a whole
    /// word (any case) notifies like a mention, unless the server or channel suppresses highlights.
    pub keywords: Vec<String>,
}

/// The caller's keyword highlight list (empty by default).
#[utoipa::path(
    get,
    path = "/users/@me/notification-keywords",
    responses((status = 200, body = NotificationKeywordsObject)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_notification_keywords(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
) -> MsgResult<Json<NotificationKeywordsObject>> {
    let me = actor(&p)?;
    Ok(Json(NotificationKeywordsObject {
        keywords: deps.service.notification_keywords(me).await?,
    }))
}

/// Replace the keyword list. Entries are trimmed; blanks and duplicates (any case) are dropped.
/// Publishes `SR_NOTIFICATION_KEYWORDS_UPDATE` to the caller's sessions.
#[utoipa::path(
    put,
    path = "/users/@me/notification-keywords",
    request_body = NotificationKeywordsObject,
    responses((status = 200, body = NotificationKeywordsObject), (status = 400, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn put_notification_keywords(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    ApiJson(body): ApiJson<NotificationKeywordsObject>,
) -> MsgResult<Json<NotificationKeywordsObject>> {
    let me = actor(&p)?;
    let out = NotificationKeywordsObject {
        keywords: deps
            .service
            .set_notification_keywords(me, &body.keywords)
            .await?,
    };
    deps.emit(SocialEvent::new(
        "SR_NOTIFICATION_KEYWORDS_UPDATE",
        vec![me],
        serde_json::to_value(&out).unwrap_or(Value::Null),
    ))
    .await;
    Ok(Json(out))
}
