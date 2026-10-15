//! Webhooks (LNG-SSET-044/045, open question 137): management by users with
//! `MANAGE_WEBHOOKS`, and token-authenticated get/modify/delete/execute.
//!
//! The token is 68 base64url characters, returned once at creation (and again by
//! `POST /webhooks/{id}/reset-token`, which replaces it); only its
//! SHA-256 is stored. `POST /webhooks/{id}/{token}` supports `?wait`.
//!
//! DEV-BOT-006: an executed message is shown as the webhook, or as the `username` and
//! `avatar_url` of the call (`message_webhook_authors`, rendered by `rich_model::decorate`);
//! `messages.author_id` stays the creator, whose channel permissions applied when sending.
//! `authorType:webhook` search is not built.

use super::deps::{SettingsDeps, actor, id, invalid, re, se};
use super::events::SettingsEvent;
use crate::routes::guilds::dto::{UserRef, nullable};
use crate::routes::media::MediaDeps;
use crate::routes::messages::MessagesDeps;
use crate::routes::messages::error::{ApiJson, ApiQuery, MsgError, MsgResult};
use crate::routes::messages::model::MessageObject;
use crate::routes::messages::render::{event_payload, render_one};
use crate::routes::{Ext, MaybeExt};
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sr_chat::SendMessage;
use sr_chat::settings::SettingsError;
use sr_chat::settings::service::WebhookPatch;
use sr_chat::settings::webhooks::Webhook;
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode, GuildId, WebhookId};
use sr_events::DomainEvent;
use sr_storage::media::MediaKind;
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/channels/{channel_id}/webhooks",
            get(list_channel_webhooks).post(create_webhook),
        )
        .route("/guilds/{guild_id}/webhooks", get(list_guild_webhooks))
        .route(
            "/webhooks/{webhook_id}",
            get(get_webhook)
                .patch(modify_webhook)
                .delete(delete_webhook),
        )
        .route(
            "/webhooks/{webhook_id}/reset-token",
            post(reset_webhook_token),
        )
        .route(
            "/webhooks/{webhook_id}/{token}",
            get(get_webhook_with_token)
                .patch(modify_webhook_with_token)
                .delete(delete_webhook_with_token)
                .post(execute_webhook),
        )
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WebhookObject {
    pub id: String,
    /// 1 incoming, 2 channel follower, 3 application.
    #[serde(rename = "type")]
    pub kind: i16,
    pub guild_id: String,
    pub channel_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<UserRef>,
    pub name: String,
    pub avatar: Option<String>,
    /// Only in the creation response; it cannot be recovered afterwards.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Execution path, only when the token is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl WebhookObject {
    fn of(
        w: &Webhook,
        users: &std::collections::HashMap<sr_core::UserId, sr_chat::repo::UserSummary>,
        token: Option<&str>,
    ) -> Self {
        Self {
            id: w.id.to_string(),
            kind: w.kind.code(),
            guild_id: w.guild_id.to_string(),
            channel_id: w.channel_id.to_string(),
            user: w.creator_id.map(|u| UserRef::of(u, users)),
            name: w.name.clone(),
            avatar: w.avatar_key.clone(),
            token: token.map(str::to_owned),
            url: token.map(|t| format!("/api/v1/webhooks/{}/{t}", w.id)),
        }
    }
}

async fn render(
    deps: &SettingsDeps,
    list: &[Webhook],
    token: Option<&str>,
) -> ApiResult<Vec<WebhookObject>> {
    let users = deps.users(list.iter().filter_map(|w| w.creator_id)).await?;
    Ok(list
        .iter()
        .map(|w| WebhookObject::of(w, &users, token))
        .collect())
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWebhookBody {
    /// 1-80 characters, no `@everyone`/`@here`.
    pub name: String,
    /// Avatar image data URI (PNG, JPEG, GIF or WebP, 8 MiB at most; cropped to a square).
    /// Served at `/cdn/avatars/{webhook_id}/{hash}.png`.
    #[serde(default)]
    pub avatar: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifyWebhookBody {
    pub name: Option<String>,
    /// Move to another channel of the same server (management endpoint only).
    pub channel_id: Option<String>,
    /// Avatar image data URI; `null` removes it.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub avatar: Option<Option<String>>,
}

fn webhook_id(raw: &str) -> ApiResult<WebhookId> {
    raw.parse()
        .map_err(|_| HttpError::from(ErrorCode::UnknownWebhook))
}

/// Create a webhook in a channel (`MANAGE_WEBHOOKS`; 15 per channel). The response
/// carries the token once.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/webhooks",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    request_body = CreateWebhookBody,
    responses((status = 200, body = WebhookObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_webhook(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    MaybeExt(media): MaybeExt<MediaDeps>,
    Path(channel_id): Path<String>,
    ApiJson(body): ApiJson<CreateWebhookBody>,
) -> ApiResult<Json<WebhookObject>> {
    let user = actor(&p)?;
    let cid: ChannelId = id("channel_id", &channel_id)?;
    let acc = deps.channel_access(user, cid).await?;
    let gid = acc.guild.guild.id;
    let wid = deps.service.new_webhook_id();
    // The permission check and the name validation come first so a refused request
    // never leaves an object in storage.
    let avatar_key = match (body.avatar, &media) {
        (None, _) => None,
        (Some(_), None) => {
            tracing::error!("webhook avatar needs MediaDeps registered on AppState");
            return Err(ErrorCode::SrInternal.into());
        }
        (Some(uri), Some(m)) => {
            if !acc.perms.has(Permissions::MANAGE_WEBHOOKS) {
                return Err(se(SettingsError::MissingPermissions));
            }
            sr_chat::settings::webhooks::validate_name(&body.name).map_err(se)?;
            Some(
                m.store(MediaKind::WebhookAvatar, wid.get(), "avatar", uri)
                    .await?,
            )
        }
    };
    let created = deps
        .service
        .create_webhook(
            acc.perms,
            user,
            gid,
            cid,
            &body.name,
            wid,
            avatar_key.clone(),
        )
        .await;
    let (w, token) = match created {
        Ok(v) => v,
        Err(e) => {
            // Nothing references the stored image; do not leave it behind.
            if let (Some(m), Some(h)) = (&media, &avatar_key) {
                m.remove(MediaKind::WebhookAvatar, wid.get(), h).await;
            }
            return Err(se(e));
        }
    };
    deps.publish(SettingsEvent::WebhooksUpdated {
        guild: gid,
        channel: cid,
    })
    .await;
    let users = deps.users([user]).await?;
    Ok(Json(WebhookObject::of(&w, &users, Some(&token))))
}

/// Webhooks of a channel (`MANAGE_WEBHOOKS`).
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/webhooks",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = Vec<WebhookObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_channel_webhooks(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(channel_id): Path<String>,
) -> ApiResult<Json<Vec<WebhookObject>>> {
    let user = actor(&p)?;
    let cid: ChannelId = id("channel_id", &channel_id)?;
    let acc = deps.channel_access(user, cid).await?;
    let all = deps
        .service
        .list_guild_webhooks(acc.perms, acc.guild.guild.id)
        .await
        .map_err(se)?;
    let mine: Vec<_> = all.into_iter().filter(|w| w.channel_id == cid).collect();
    Ok(Json(render(&deps, &mine, None).await?))
}

/// Webhooks of a guild (`MANAGE_WEBHOOKS`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/webhooks",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<WebhookObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_guild_webhooks(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<Vec<WebhookObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let list = deps
        .service
        .list_guild_webhooks(acc.perms, gid)
        .await
        .map_err(se)?;
    Ok(Json(render(&deps, &list, None).await?))
}

/// A webhook the caller manages: resolves its guild, then checks the permission.
async fn managed(
    deps: &SettingsDeps,
    user: sr_core::UserId,
    raw: &str,
) -> ApiResult<(Webhook, Permissions)> {
    use sr_chat::settings::repo::WebhookRepository;
    let wid = webhook_id(raw)?;
    let w = deps
        .service
        .repo()
        .get_webhook(wid)
        .await
        .map_err(re)?
        .ok_or(ErrorCode::UnknownWebhook)?;
    let acc = deps.guild_access(user, w.guild_id).await?;
    Ok((w, acc.perms))
}

/// Get a webhook (`MANAGE_WEBHOOKS`).
#[utoipa::path(
    get,
    path = "/webhooks/{webhook_id}",
    params(("webhook_id" = String, Path, description = "Webhook snowflake")),
    responses((status = 200, body = WebhookObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_webhook(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(webhook_id): Path<String>,
) -> ApiResult<Json<WebhookObject>> {
    let user = actor(&p)?;
    let (w, perms) = managed(&deps, user, &webhook_id).await?;
    let w = deps
        .service
        .guild_webhook(perms, w.guild_id, w.id)
        .await
        .map_err(se)?;
    Ok(Json(render(&deps, &[w], None).await?.remove(0)))
}

/// Issue a new execution token for an incoming webhook and invalidate the old one
/// (`MANAGE_WEBHOOKS`). The token is stored only as a hash, so an existing URL cannot be
/// shown again; this is the supported way to get a copyable URL later. The response is the
/// only place the new `token` and `url` appear.
#[utoipa::path(
    post,
    path = "/webhooks/{webhook_id}/reset-token",
    params(("webhook_id" = String, Path, description = "Webhook snowflake")),
    responses((status = 200, body = WebhookObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn reset_webhook_token(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(webhook_id): Path<String>,
) -> ApiResult<Json<WebhookObject>> {
    let user = actor(&p)?;
    let (w, perms) = managed(&deps, user, &webhook_id).await?;
    let (w, token) = deps
        .service
        .reset_webhook_token(perms, user, w.guild_id, w.id)
        .await
        .map_err(se)?;
    deps.publish(SettingsEvent::WebhooksUpdated {
        guild: w.guild_id,
        channel: w.channel_id,
    })
    .await;
    let users = deps.users(w.creator_id).await?;
    Ok(Json(WebhookObject::of(&w, &users, Some(&token))))
}

async fn modify_inner(
    deps: &SettingsDeps,
    media: Option<&MediaDeps>,
    actor_id: sr_core::UserId,
    perms: Permissions,
    w: &Webhook,
    body: ModifyWebhookBody,
    allow_move: bool,
) -> ApiResult<WebhookObject> {
    let channel_id = match (&body.channel_id, allow_move) {
        (Some(c), true) => {
            let cid: ChannelId = id("channel_id", c)?;
            let ok = deps
                .chat
                .get_channel(cid)
                .await
                .map_err(re)?
                .is_some_and(|ch| ch.guild_id == Some(w.guild_id));
            if !ok {
                return Err(invalid("channel_id", "UNKNOWN_CHANNEL", "Unknown channel."));
            }
            Some(cid)
        }
        _ => None,
    };
    if !perms.has(Permissions::MANAGE_WEBHOOKS) {
        return Err(se(SettingsError::MissingPermissions));
    }
    let new_avatar = match (body.avatar, media) {
        (None, _) => None,
        (Some(None), _) => Some(None),
        (Some(Some(_)), None) => {
            tracing::error!("webhook avatar needs MediaDeps registered on AppState");
            return Err(ErrorCode::SrInternal.into());
        }
        (Some(Some(uri)), Some(m)) => Some(Some(
            m.store(MediaKind::WebhookAvatar, w.id.get(), "avatar", uri)
                .await?,
        )),
    };
    let stored_new = new_avatar.clone().flatten();
    let result = deps
        .service
        .modify_webhook(
            perms,
            actor_id,
            w.guild_id,
            w.id,
            WebhookPatch {
                name: body.name,
                channel_id,
                avatar_key: new_avatar,
            },
        )
        .await;
    let updated = match result {
        Ok(u) => u,
        Err(e) => {
            if let (Some(m), Some(h)) = (media, &stored_new) {
                m.remove(MediaKind::WebhookAvatar, w.id.get(), h).await;
            }
            return Err(se(e));
        }
    };
    // Committed: drop the image this edit replaced or cleared.
    if let (Some(m), Some(old)) = (media, &w.avatar_key)
        && updated.avatar_key.as_ref() != Some(old)
    {
        m.remove(MediaKind::WebhookAvatar, w.id.get(), old).await;
    }
    deps.publish(SettingsEvent::WebhooksUpdated {
        guild: updated.guild_id,
        channel: updated.channel_id,
    })
    .await;
    Ok(render(deps, &[updated], None).await?.remove(0))
}

/// Modify a webhook's name or channel (`MANAGE_WEBHOOKS`).
#[utoipa::path(
    patch,
    path = "/webhooks/{webhook_id}",
    params(("webhook_id" = String, Path, description = "Webhook snowflake")),
    request_body = ModifyWebhookBody,
    responses((status = 200, body = WebhookObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_webhook(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    MaybeExt(media): MaybeExt<MediaDeps>,
    Path(webhook_id): Path<String>,
    ApiJson(body): ApiJson<ModifyWebhookBody>,
) -> ApiResult<Json<WebhookObject>> {
    let user = actor(&p)?;
    let (w, perms) = managed(&deps, user, &webhook_id).await?;
    Ok(Json(
        modify_inner(&deps, media.as_deref(), user, perms, &w, body, true).await?,
    ))
}

/// Delete a webhook (`MANAGE_WEBHOOKS`).
#[utoipa::path(
    delete,
    path = "/webhooks/{webhook_id}",
    params(("webhook_id" = String, Path, description = "Webhook snowflake")),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_webhook(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    MaybeExt(media): MaybeExt<MediaDeps>,
    Path(webhook_id): Path<String>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let (w, perms) = managed(&deps, user, &webhook_id).await?;
    deps.service
        .delete_webhook(perms, user, w.guild_id, w.id)
        .await
        .map_err(se)?;
    if let (Some(m), Some(h)) = (media.as_deref(), &w.avatar_key) {
        m.remove(MediaKind::WebhookAvatar, w.id.get(), h).await;
    }
    deps.publish(SettingsEvent::WebhooksUpdated {
        guild: w.guild_id,
        channel: w.channel_id,
    })
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn authed(deps: &SettingsDeps, wid: &str, token: &str) -> ApiResult<Webhook> {
    deps.service
        .authenticate_webhook(webhook_id(wid)?, token)
        .await
        .map_err(se)
}

/// Get a webhook by id and token (no other authentication).
#[utoipa::path(
    get,
    path = "/webhooks/{webhook_id}/{token}",
    params(
        ("webhook_id" = String, Path, description = "Webhook snowflake"),
        ("token" = String, Path, description = "Execution token"),
    ),
    responses((status = 200, body = WebhookObject), (status = 404, body = ErrorBody))
)]
pub async fn get_webhook_with_token(
    Ext(deps): Ext<SettingsDeps>,
    Path((webhook_id, token)): Path<(String, String)>,
) -> ApiResult<Json<WebhookObject>> {
    let w = authed(&deps, &webhook_id, &token).await?;
    let mut obj = render(&deps, &[w], None).await?.remove(0);
    // Same shape Discord returns for token-authenticated reads: no user object.
    obj.user = None;
    Ok(Json(obj))
}

/// Modify a webhook's name by id and token.
#[utoipa::path(
    patch,
    path = "/webhooks/{webhook_id}/{token}",
    params(
        ("webhook_id" = String, Path, description = "Webhook snowflake"),
        ("token" = String, Path, description = "Execution token"),
    ),
    request_body = ModifyWebhookBody,
    responses((status = 200, body = WebhookObject), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn modify_webhook_with_token(
    Ext(deps): Ext<SettingsDeps>,
    MaybeExt(media): MaybeExt<MediaDeps>,
    Path((webhook_id, token)): Path<(String, String)>,
    ApiJson(body): ApiJson<ModifyWebhookBody>,
) -> ApiResult<Json<WebhookObject>> {
    let w = authed(&deps, &webhook_id, &token).await?;
    // The token holder acts as the webhook's creator for the audit entry.
    let actor_id = w.creator_id.unwrap_or_default();
    let mut obj = modify_inner(
        &deps,
        media.as_deref(),
        actor_id,
        Permissions::MANAGE_WEBHOOKS,
        &w,
        body,
        false,
    )
    .await?;
    obj.user = None;
    Ok(Json(obj))
}

/// Delete a webhook by id and token.
#[utoipa::path(
    delete,
    path = "/webhooks/{webhook_id}/{token}",
    params(
        ("webhook_id" = String, Path, description = "Webhook snowflake"),
        ("token" = String, Path, description = "Execution token"),
    ),
    responses((status = 204), (status = 404, body = ErrorBody))
)]
pub async fn delete_webhook_with_token(
    Ext(deps): Ext<SettingsDeps>,
    MaybeExt(media): MaybeExt<MediaDeps>,
    Path((webhook_id, token)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let w = authed(&deps, &webhook_id, &token).await?;
    deps.service
        .delete_webhook(
            Permissions::MANAGE_WEBHOOKS,
            w.creator_id.unwrap_or_default(),
            w.guild_id,
            w.id,
        )
        .await
        .map_err(se)?;
    if let (Some(m), Some(h)) = (media.as_deref(), &w.avatar_key) {
        m.remove(MediaKind::WebhookAvatar, w.id.get(), h).await;
    }
    deps.publish(SettingsEvent::WebhooksUpdated {
        guild: w.guild_id,
        channel: w.channel_id,
    })
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Default, Deserialize)]
pub struct ExecuteQuery {
    /// Return the created message (200) instead of 204.
    pub wait: Option<bool>,
    /// Threads are not supported yet.
    pub thread_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ExecuteWebhookBody {
    pub content: Option<String>,
    /// Shown instead of the webhook name for this message (1 to 80 characters).
    pub username: Option<String>,
    pub avatar_url: Option<String>,
    pub tts: Option<bool>,
    #[schema(value_type = Option<Vec<Object>>)]
    pub embeds: Option<Vec<sr_chat::message::Embed>>,
    #[schema(value_type = Option<Vec<Object>>)]
    pub components: Option<Vec<sr_chat::message::Component>>,
    pub flags: Option<u32>,
}

/// Execute a webhook: post a message into its channel. `?wait=true` returns the
/// message. A wrong token and an unknown id both answer 404.
#[utoipa::path(
    post,
    path = "/webhooks/{webhook_id}/{token}",
    params(
        ("webhook_id" = String, Path, description = "Webhook snowflake"),
        ("token" = String, Path, description = "Execution token"),
        ("wait" = Option<bool>, Query, description = "Return the created message"),
        ("thread_id" = Option<String>, Query, description = "Not supported yet"),
    ),
    request_body = ExecuteWebhookBody,
    responses(
        (status = 200, body = MessageObject, description = "With `wait=true`"),
        (status = 204),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
    )
)]
pub async fn execute_webhook(
    Ext(deps): Ext<SettingsDeps>,
    Ext(messages): Ext<MessagesDeps>,
    MaybeExt(dev): MaybeExt<crate::routes::developer::DevDeps>,
    Path((webhook_id, token)): Path<(String, String)>,
    ApiQuery(q): ApiQuery<ExecuteQuery>,
    ApiJson(body): ApiJson<ExecuteWebhookBody>,
) -> MsgResult<axum::response::Response> {
    // `POST /webhooks/{application_id}/{interaction token}` is an interaction follow-up.
    if let Some(dev) = &dev
        && let Some(resp) = crate::routes::developer::try_followup(
            dev,
            &messages,
            &webhook_id,
            &token,
            q.wait == Some(true),
            &body,
        )
        .await?
    {
        return Ok(resp);
    }
    let w = authed(&deps, &webhook_id, &token).await?;
    if q.thread_id.is_some() {
        return Err(MsgError::from(HttpError(
            sr_core::ApiError::invalid_form().field(
                "thread_id",
                "NOT_SUPPORTED",
                "Threads are not available yet.",
            ),
        )));
    }
    let shown_as = webhook_display(
        &w.name,
        body.username.as_deref(),
        body.avatar_url.as_deref(),
    )?;
    let embeds = body.embeds.unwrap_or_default();
    let content = body.content.unwrap_or_default();
    if content.trim().is_empty() && embeds.is_empty() {
        return Err(MsgError::from(invalid(
            "content",
            "BASE_TYPE_REQUIRED",
            "Cannot send an empty message.",
        )));
    }
    if embeds.len() > sr_chat::message::MAX_EMBEDS {
        return Err(MsgError::from(invalid(
            "embeds",
            "BASE_TYPE_MAX_LENGTH",
            "Must be 10 or fewer in length.",
        )));
    }
    let author = w.creator_id.ok_or_else(|| {
        tracing::error!(webhook = w.id.get(), "incoming webhook without a creator");
        MsgError::from(ErrorCode::SrInternal)
    })?;
    let mut flags = body.flags.unwrap_or(0);
    if body.tts == Some(true) {
        flags |= sr_chat::message_flags::TTS;
    }
    let msg = messages
        .send(
            author,
            w.channel_id,
            SendMessage {
                content,
                embeds,
                components: body.components.unwrap_or_default(),
                flags,
                ..Default::default()
            },
        )
        .await?;
    if let Some(store) = &messages.rich.store {
        let author = sr_chat::rich_repo::WebhookAuthor {
            webhook_id: w.id.get(),
            username: shown_as.0,
            avatar_url: shown_as.1,
        };
        store
            .set_webhook_author(msg.id, &author)
            .await
            .map_err(|e| {
                tracing::error!(error = %e.0, "recording the webhook author failed");
                MsgError::from(ErrorCode::SrInternal)
            })?;
    }
    let data = event_payload(&messages, &msg).await?;
    messages
        .publish(DomainEvent::MessageSent {
            guild: msg.guild_id,
            channel: w.channel_id,
            message_id: msg.id.get(),
            author,
            data,
        })
        .await;
    if q.wait == Some(true) {
        let out = render_one(&messages, author, &msg).await?;
        Ok(Json(out).into_response())
    } else {
        Ok(StatusCode::NO_CONTENT.into_response())
    }
}

/// Longest `username` a webhook execution may carry (Discord: 80).
const MAX_WEBHOOK_USERNAME: usize = 80;
/// Longest `avatar_url`.
const MAX_WEBHOOK_AVATAR_URL: usize = 512;

/// The name and avatar a webhook message is shown with: the per-call override when given,
/// else the webhook's own name. The avatar must be an absolute `https` URL; viewers load it
/// through the image proxy, never directly (DEV-BOT-006).
fn webhook_display(
    webhook_name: &str,
    username: Option<&str>,
    avatar_url: Option<&str>,
) -> MsgResult<(String, Option<String>)> {
    let name = match username.map(str::trim) {
        None | Some("") => webhook_name.to_owned(),
        Some(n) if n.chars().count() > MAX_WEBHOOK_USERNAME => {
            return Err(MsgError::from(invalid(
                "username",
                "BASE_TYPE_MAX_LENGTH",
                "Must be 80 or fewer in length.",
            )));
        }
        Some(n) => n.to_owned(),
    };
    let avatar = match avatar_url.map(str::trim) {
        None | Some("") => None,
        Some(u)
            if u.len() > MAX_WEBHOOK_AVATAR_URL
                || !u
                    .get(..8)
                    .is_some_and(|p| p.eq_ignore_ascii_case("https://"))
                || u.chars().any(|c| c.is_control() || c.is_whitespace()) =>
        {
            return Err(MsgError::from(invalid(
                "avatar_url",
                "URL_TYPE_INVALID_URL",
                "Must be an https URL of at most 512 characters.",
            )));
        }
        Some(u) => Some(u.to_owned()),
    };
    Ok((name, avatar))
}

#[cfg(test)]
mod display_tests {
    use super::webhook_display;

    #[test]
    fn username_defaults_to_the_webhook_name_and_is_length_checked() {
        assert_eq!(
            webhook_display("Hook", None, None).unwrap(),
            ("Hook".into(), None)
        );
        assert_eq!(webhook_display("Hook", Some("  "), None).unwrap().0, "Hook");
        assert_eq!(
            webhook_display("Hook", Some(" Bot9 "), None).unwrap().0,
            "Bot9"
        );
        assert!(webhook_display("Hook", Some(&"x".repeat(81)), None).is_err());
        assert!(webhook_display("Hook", Some(&"x".repeat(80)), None).is_ok());
    }

    #[test]
    fn avatar_must_be_a_short_https_url_without_whitespace() {
        let ok = webhook_display("H", None, Some("https://cdn.example/a.png")).unwrap();
        assert_eq!(ok.1.as_deref(), Some("https://cdn.example/a.png"));
        for bad in [
            "http://cdn.example/a.png",
            "javascript:alert(1)",
            "https://x.example/a b.png",
            "//cdn.example/a.png",
        ] {
            assert!(webhook_display("H", None, Some(bad)).is_err(), "{bad}");
        }
        let long = format!("https://x.example/{}", "a".repeat(600));
        assert!(webhook_display("H", None, Some(&long)).is_err());
    }
}
