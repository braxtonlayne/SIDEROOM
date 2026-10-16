//! Guild scheduled events (Discord parity; web: Events row and Create Event).
//!
//! * `GET|POST /guilds/{guild_id}/scheduled-events`
//! * `GET|PATCH|DELETE /guilds/{guild_id}/scheduled-events/{event_id}`
//! * `GET /guilds/{guild_id}/scheduled-events/{event_id}/users`: who is interested
//! * `PUT|DELETE /guilds/{guild_id}/scheduled-events/{event_id}/users/@me`: mark or clear
//!   your own interest (RSVP)
//!
//! Gateway: `GUILD_SCHEDULED_EVENT_CREATE/UPDATE/DELETE` (the event object) and
//! `GUILD_SCHEDULED_EVENT_USER_ADD/REMOVE` (`{guild_scheduled_event_id, user_id, guild_id}`).
//! Status moves with `PATCH {status}`: 2 starts a scheduled event, 3 ends a running one, 4
//! cancels a scheduled one. The server also starts and ends events by itself when their time
//! comes (see `sr-server`'s `events_jobs`).

use super::deps::{SettingsDeps, actor, id, invalid, re, se};
use super::events::{ScheduledEventDispatch, SettingsEvent};
use crate::routes::guilds::dto::{MemberObject, UserRef, iso8601, nullable, parse_iso8601};
use crate::routes::media::MediaDeps;
use crate::routes::messages::error::{ApiJson, ApiQuery};
use crate::routes::{Ext, MaybeExt};
use crate::{Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{Json, Router, extract::Path, http::StatusCode, routing::get};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sr_chat::ChannelType;
use sr_chat::settings::scheduled_events::{
    ChannelKind, EntityType, EventPatch, EventStatus, NewEvent, ScheduledEvent,
};
use sr_chat::settings::service::{EventChange, EventStats};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode, GuildId, UserId};
use sr_storage::media::MediaKind;
use std::collections::HashMap;
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/guilds/{guild_id}/scheduled-events",
            axum::routing::post(create_event)
                .layer(crate::routes::media::body_limit())
                .get(list_events),
        )
        .route(
            "/guilds/{guild_id}/scheduled-events/{event_id}",
            axum::routing::patch(modify_event)
                .layer(crate::routes::media::body_limit())
                .get(get_event)
                .delete(delete_event),
        )
        .route(
            "/guilds/{guild_id}/scheduled-events/{event_id}/users",
            get(list_interested),
        )
        .route(
            "/guilds/{guild_id}/scheduled-events/{event_id}/users/@me",
            axum::routing::put(mark_interested).delete(clear_interested),
        )
}

// ------------------------------------------------------------------ objects

/// `entity_metadata` of an event.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EventMetadata {
    /// Where an external event happens (at most 100 characters).
    #[serde(default)]
    pub location: Option<String>,
}

/// Discord's guild scheduled event object.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ScheduledEventObject {
    pub id: String,
    pub guild_id: String,
    pub channel_id: Option<String>,
    pub creator_id: String,
    pub creator: UserRef,
    pub name: String,
    pub description: Option<String>,
    pub scheduled_start_time: String,
    pub scheduled_end_time: Option<String>,
    /// Always 2 (server members only).
    pub privacy_level: u8,
    /// 1 scheduled, 2 active, 3 completed, 4 canceled.
    pub status: u8,
    /// 1 stage, 2 voice, 3 external.
    pub entity_type: u8,
    /// The stage instance (the stage channel id) of a running stage event.
    pub entity_id: Option<String>,
    pub entity_metadata: Option<EventMetadata>,
    /// Members who marked themselves interested; with `with_user_count=true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_count: Option<u32>,
    /// The caller is interested (SIDE ROOM extension; absent on gateway dispatches).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_rsvp: Option<bool>,
    /// Cover image hash: `/cdn/guild-events/{id}/{hash}.png` (SIDE ROOM serves JPEG, PNG and GIF).
    pub image: Option<String>,
}

impl ScheduledEventObject {
    pub fn build(
        e: &ScheduledEvent,
        users: &HashMap<UserId, sr_chat::repo::UserSummary>,
        stats: Option<&EventStats>,
        with_count: bool,
    ) -> Self {
        Self {
            id: e.id.to_string(),
            guild_id: e.guild_id.to_string(),
            channel_id: e.channel_id.map(|c| c.to_string()),
            creator_id: e.creator_id.to_string(),
            creator: UserRef::of(e.creator_id, users),
            name: e.name.clone(),
            description: e.description.clone(),
            scheduled_start_time: iso8601(e.start_ms),
            scheduled_end_time: e.end_ms.map(iso8601),
            privacy_level: 2,
            status: e.status.code(),
            entity_type: e.entity_type.code(),
            entity_id: e.entity_id.map(|id| id.to_string()),
            entity_metadata: (e.entity_type == EntityType::External).then(|| EventMetadata {
                location: e.location.clone(),
            }),
            user_count: with_count
                .then(|| stats.map_or(0, |s| s.interested.get(&e.id).copied().unwrap_or(0))),
            user_rsvp: stats.map(|s| s.mine.contains(&e.id)),
            image: e.image_key.clone(),
        }
    }
}

/// Body of `POST /guilds/{id}/scheduled-events`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateEventBody {
    pub name: String,
    pub description: Option<String>,
    /// The voice or stage channel; omit for an external event.
    pub channel_id: Option<String>,
    /// 1 stage, 2 voice, 3 external.
    pub entity_type: u8,
    pub entity_metadata: Option<EventMetadata>,
    /// ISO 8601; must be in the future.
    pub scheduled_start_time: String,
    /// ISO 8601; required for external events.
    pub scheduled_end_time: Option<String>,
    /// Only 2 (server members) exists.
    pub privacy_level: Option<u8>,
    /// Cover image as a data URI (JPEG, PNG or GIF, 2 MiB at most).
    pub image: Option<String>,
}

/// Body of `PATCH /guilds/{id}/scheduled-events/{event_id}`.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifyEventBody {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub channel_id: Option<Option<String>>,
    pub entity_type: Option<u8>,
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<EventMetadata>)]
    pub entity_metadata: Option<Option<EventMetadata>>,
    pub scheduled_start_time: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub scheduled_end_time: Option<Option<String>>,
    /// 2 starts a scheduled event, 3 ends a running one, 4 cancels a scheduled one.
    pub status: Option<u8>,
    pub privacy_level: Option<u8>,
    /// Cover image data URI; `null` removes the cover.
    #[serde(default, deserialize_with = "nullable")]
    #[schema(value_type = Option<String>)]
    pub image: Option<Option<String>>,
}

/// Query of the list and get routes.
#[derive(Debug, Default, Deserialize)]
pub struct EventQuery {
    pub with_user_count: Option<bool>,
}

/// Query of `GET .../users`.
#[derive(Debug, Default, Deserialize)]
pub struct InterestedQuery {
    /// 1 to 100, default 100.
    pub limit: Option<usize>,
    pub with_member: Option<bool>,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// One interested member.
#[derive(Debug, Serialize, ToSchema)]
pub struct EventUserObject {
    pub guild_scheduled_event_id: String,
    pub user: UserRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<MemberObject>,
}

// ------------------------------------------------------------------ helpers

fn parse_time(field: &str, raw: &str) -> ApiResult<i64> {
    parse_iso8601(raw).ok_or_else(|| {
        invalid(
            field,
            "DATE_TYPE_PARSE",
            "Not a valid ISO 8601 date and time.",
        )
    })
}

fn entity_of(raw: u8) -> ApiResult<EntityType> {
    EntityType::from_code(raw).ok_or_else(|| {
        invalid(
            "entity_type",
            "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
            "Must be 1 (stage), 2 (voice) or 3 (external).",
        )
    })
}

fn check_privacy(level: Option<u8>) -> ApiResult<()> {
    match level {
        None | Some(2) => Ok(()),
        Some(_) => Err(invalid(
            "privacy_level",
            "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
            "Must be 2 (server members only).",
        )),
    }
}

/// What kind of channel `channel` is in `guild`, plus the caller's permissions there.
async fn channel_kind(
    deps: &SettingsDeps,
    user: UserId,
    guild: GuildId,
    channel: Option<ChannelId>,
) -> ApiResult<(ChannelKind, Option<Permissions>)> {
    let Some(cid) = channel else {
        return Ok((ChannelKind::Missing, None));
    };
    let Some(ch) = deps.chat.get_channel(cid).await.map_err(re)? else {
        return Ok((ChannelKind::Missing, None));
    };
    if ch.guild_id != Some(guild) {
        return Ok((ChannelKind::Missing, None));
    }
    let kind = match ch.kind {
        ChannelType::Voice => ChannelKind::Voice,
        ChannelType::Stage => ChannelKind::Stage,
        _ => ChannelKind::Other,
    };
    // Voice and stage events need to see and join the channel.
    let perms = deps
        .channel_access(user, cid)
        .await
        .map(|a| a.perms)
        .unwrap_or_else(|_| Permissions::empty());
    Ok((kind, Some(perms)))
}

fn need_channel_access(perms: Option<Permissions>) -> ApiResult<()> {
    match perms {
        Some(p) if !(p.has(Permissions::VIEW_CHANNEL) && p.has(Permissions::CONNECT)) => {
            Err(ErrorCode::MissingPermissions.into())
        }
        _ => Ok(()),
    }
}

/// The event as sent on the gateway: no viewer-specific fields.
async fn dispatch_value(deps: &SettingsDeps, e: &ScheduledEvent, count: u32) -> Value {
    let users = deps.users([e.creator_id]).await.unwrap_or_default();
    let mut obj = ScheduledEventObject::build(e, &users, None, true);
    obj.user_count = Some(count);
    serde_json::to_value(obj).unwrap_or(Value::Null)
}

/// Announce an event that changed (also used by the scheduler).
pub async fn announce(deps: &SettingsDeps, kind: ScheduledEventDispatch, e: &ScheduledEvent) {
    let count = deps
        .service
        .event_stats(UserId::new(0), &[e.id])
        .await
        .map(|s| s.interested.get(&e.id).copied().unwrap_or(0))
        .unwrap_or(0);
    let payload = dispatch_value(deps, e, count).await;
    deps.publish(SettingsEvent::ScheduledEvent {
        guild: e.guild_id,
        kind,
        payload,
    })
    .await;
}

/// Announce a status change: `GUILD_SCHEDULED_EVENT_UPDATE`, and when the event just started,
/// the start notification to every interested member (gateway dispatch and Web Push; G-93).
pub async fn announce_change(deps: &SettingsDeps, change: &EventChange) {
    announce(deps, ScheduledEventDispatch::Update, &change.event).await;
    if change.started() {
        notify_started(deps, &change.event).await;
    }
}

async fn notify_started(deps: &SettingsDeps, ev: &ScheduledEvent) {
    match deps.service.all_interested(ev.id).await {
        Ok(interested) if !interested.is_empty() => {
            deps.event_notifier().event_started(ev, &interested).await;
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "could not list members interested in an event"),
    }
}

/// The clock's turn (the leased `scheduled_events.tick` job in `sr-server`, every 30 s):
/// start external events whose time has come, end running ones at their end time, after 15
/// minutes of an empty channel or when their stage instance is gone, cancel voice and stage
/// events nobody started within a day, and wind up orphans. Each change is audited and
/// announced. Idempotent, so a repeated or concurrent turn changes nothing twice.
pub async fn advance(deps: &SettingsDeps) {
    match deps.service.advance_events(deps.event_world()).await {
        Ok(changed) => {
            for change in &changed {
                announce_change(deps, change).await;
            }
        }
        Err(e) => tracing::warn!(error = %e, "advancing scheduled events failed"),
    }
}

/// A member entered `channel` of `guild` (`VoiceJoined` on the bus): a voice or stage event
/// waiting on the channel starts when its start time has come (D-222).
pub async fn on_voice_joined(deps: &SettingsDeps, guild: GuildId, channel: ChannelId) {
    match deps
        .service
        .start_on_voice_join(guild, channel, deps.event_world())
        .await
    {
        Ok(Some(change)) => announce_change(deps, &change).await,
        Ok(None) => {}
        Err(e) => tracing::warn!(error = %e, "starting an event on a voice join failed"),
    }
}

/// Store a cover under the event and return its hash. The image field of the request names
/// the error tree.
async fn store_cover(
    media: &Option<std::sync::Arc<MediaDeps>>,
    event: u64,
    uri: String,
) -> ApiResult<String> {
    let Some(m) = media else {
        tracing::error!("event covers need MediaDeps registered on AppState");
        return Err(ErrorCode::SrInternal.into());
    };
    m.store(MediaKind::EventCover, event, "image", uri).await
}

async fn drop_cover(media: &Option<std::sync::Arc<MediaDeps>>, event: u64, hash: &str) {
    if let Some(m) = media {
        m.remove(MediaKind::EventCover, event, hash).await;
    }
}

async fn render(
    deps: &SettingsDeps,
    viewer: UserId,
    events: &[ScheduledEvent],
    with_count: bool,
) -> ApiResult<Vec<ScheduledEventObject>> {
    let ids: Vec<u64> = events.iter().map(|e| e.id).collect();
    let stats = deps.service.event_stats(viewer, &ids).await.map_err(se)?;
    let users = deps.users(events.iter().map(|e| e.creator_id)).await?;
    Ok(events
        .iter()
        .map(|e| ScheduledEventObject::build(e, &users, Some(&stats), with_count))
        .collect())
}

// ------------------------------------------------------------------ handlers

/// Open (scheduled or running) events of a server, soonest first (any member).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/scheduled-events",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("with_user_count" = Option<bool>, Query, description = "Include the number of interested members"),
    ),
    responses((status = 200, body = Vec<ScheduledEventObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_events(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiQuery(q): ApiQuery<EventQuery>,
) -> ApiResult<Json<Vec<ScheduledEventObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    deps.guild_access(user, gid).await?;
    let list = deps.service.list_events(gid).await.map_err(se)?;
    Ok(Json(
        render(&deps, user, &list, q.with_user_count.unwrap_or(false)).await?,
    ))
}

/// Create an event (`CREATE_EVENTS` or `MANAGE_EVENTS`). Voice and stage events need
/// `VIEW_CHANNEL` and `CONNECT` in their channel. Emits `GUILD_SCHEDULED_EVENT_CREATE`.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/scheduled-events",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = CreateEventBody,
    responses((status = 200, body = ScheduledEventObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_event(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    MaybeExt(media): MaybeExt<MediaDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<CreateEventBody>,
) -> ApiResult<Json<ScheduledEventObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    check_privacy(body.privacy_level)?;
    let channel_id = body
        .channel_id
        .as_deref()
        .filter(|c| !c.is_empty())
        .map(|c| id::<ChannelId>("channel_id", c))
        .transpose()?;
    let (kind, chan_perms) = channel_kind(&deps, user, gid, channel_id).await?;
    need_channel_access(chan_perms)?;
    if !sr_chat::settings::scheduled_events::can_create(acc.perms) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    // The cover is stored under the id the event is about to get.
    let event_id = deps.service.new_id();
    let image_key = match body.image.filter(|i| !i.is_empty()) {
        Some(uri) => Some(store_cover(&media, event_id, uri).await?),
        None => None,
    };
    let new = NewEvent {
        id: Some(event_id),
        image_key: image_key.clone(),
        channel_id,
        name: body.name,
        description: body.description,
        start_ms: parse_time("scheduled_start_time", &body.scheduled_start_time)?,
        end_ms: body
            .scheduled_end_time
            .as_deref()
            .map(|t| parse_time("scheduled_end_time", t))
            .transpose()?,
        entity_type: entity_of(body.entity_type)?,
        location: body.entity_metadata.and_then(|m| m.location),
    };
    let ev = match deps
        .service
        .create_event(acc.perms, user, gid, new, kind)
        .await
    {
        Ok(ev) => ev,
        Err(e) => {
            if let Some(h) = &image_key {
                drop_cover(&media, event_id, h).await;
            }
            return Err(se(e));
        }
    };
    announce(&deps, ScheduledEventDispatch::Create, &ev).await;
    // D-228: a person who creates an event is marked Interested (and can un-mark themselves);
    // an event created by a bot marks nobody.
    if !matches!(p.kind, crate::AuthKind::Bot { .. }) {
        match deps.service.set_interested(gid, ev.id, user, true).await {
            Ok((_, true)) => announce_interest(&deps, gid, ev.id, user, true).await,
            Ok(_) => {}
            Err(e) => tracing::warn!(error = ?e, event = ev.id, "could not mark the creator interested"),
        }
    }
    let mut out = render(&deps, user, std::slice::from_ref(&ev), true).await?;
    Ok(Json(out.remove(0)))
}

/// Dispatch `GUILD_SCHEDULED_EVENT_USER_ADD` / `_REMOVE` for `user`.
async fn announce_interest(deps: &SettingsDeps, gid: GuildId, eid: u64, user: UserId, added: bool) {
    deps.publish(SettingsEvent::ScheduledEvent {
        guild: gid,
        kind: if added {
            ScheduledEventDispatch::UserAdd
        } else {
            ScheduledEventDispatch::UserRemove
        },
        payload: json!({
            "guild_scheduled_event_id": eid.to_string(),
            "user_id": user.to_string(),
            "guild_id": gid.to_string(),
        }),
    })
    .await;
}

/// One event of the server in any state (any member).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/scheduled-events/{event_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("event_id" = String, Path, description = "Event snowflake"),
        ("with_user_count" = Option<bool>, Query, description = "Include the number of interested members"),
    ),
    responses((status = 200, body = ScheduledEventObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_event(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, event_id)): Path<(String, String)>,
    ApiQuery(q): ApiQuery<EventQuery>,
) -> ApiResult<Json<ScheduledEventObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let eid: u64 = id("event_id", &event_id)?;
    deps.guild_access(user, gid).await?;
    let ev = deps.service.get_event(gid, eid).await.map_err(se)?;
    let mut out = render(
        &deps,
        user,
        std::slice::from_ref(&ev),
        q.with_user_count.unwrap_or(false),
    )
    .await?;
    Ok(Json(out.remove(0)))
}

/// Edit, start (`status` 2), end (3) or cancel (4) an event (`MANAGE_EVENTS`, or the creator
/// with `CREATE_EVENTS`). Emits `GUILD_SCHEDULED_EVENT_UPDATE`.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/scheduled-events/{event_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("event_id" = String, Path, description = "Event snowflake"),
    ),
    request_body = ModifyEventBody,
    responses((status = 200, body = ScheduledEventObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_event(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    MaybeExt(media): MaybeExt<MediaDeps>,
    Path((guild_id, event_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<ModifyEventBody>,
) -> ApiResult<Json<ScheduledEventObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let eid: u64 = id("event_id", &event_id)?;
    let acc = deps.guild_access(user, gid).await?;
    check_privacy(body.privacy_level)?;
    let current = deps.service.get_event(gid, eid).await.map_err(se)?;
    if !sr_chat::settings::scheduled_events::can_manage(acc.perms, &current, user) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let channel = match &body.channel_id {
        None => None,
        Some(None) => Some(None),
        Some(Some(c)) if c.is_empty() => Some(None),
        Some(Some(c)) => Some(Some(id::<ChannelId>("channel_id", c)?)),
    };
    let target_channel = channel.unwrap_or(current.channel_id);
    let (kind, chan_perms) = channel_kind(&deps, user, gid, target_channel).await?;
    if channel.is_some() {
        need_channel_access(chan_perms)?;
    }
    // A new cover is stored first and the replaced one dropped once the edit committed.
    let mut stored: Option<String> = None;
    let image_key = match body.image {
        None => None,
        Some(None) => Some(None),
        Some(Some(uri)) if uri.is_empty() => Some(None),
        Some(Some(uri)) => {
            let h = store_cover(&media, eid, uri).await?;
            stored = Some(h.clone());
            Some(Some(h))
        }
    };
    let patch = EventPatch {
        image_key,
        channel_id: channel,
        name: body.name,
        description: body.description,
        start_ms: body
            .scheduled_start_time
            .as_deref()
            .map(|t| parse_time("scheduled_start_time", t))
            .transpose()?,
        end_ms: match &body.scheduled_end_time {
            None => None,
            Some(None) => Some(None),
            Some(Some(t)) => Some(Some(parse_time("scheduled_end_time", t)?)),
        },
        entity_type: body.entity_type.map(entity_of).transpose()?,
        location: body.entity_metadata.map(|m| m.and_then(|m| m.location)),
        status: body
            .status
            .map(|s| {
                EventStatus::from_code(s).ok_or_else(|| {
                    invalid("status", "ENUM_TYPE_BASE_TYPE_BAD_VALUE", "Must be 1 to 4.")
                })
            })
            .transpose()?,
    };
    let ev = match deps
        .service
        .update_event(acc.perms, user, gid, eid, patch, kind, deps.event_world())
        .await
    {
        Ok(ev) => ev,
        Err(e) => {
            if let Some(h) = &stored {
                drop_cover(&media, eid, h).await;
            }
            return Err(se(e));
        }
    };
    if let Some(old) = &current.image_key
        && ev.image_key.as_ref() != Some(old)
    {
        drop_cover(&media, eid, old).await;
    }
    if ev != current {
        announce_change(
            &deps,
            &EventChange {
                before: current.status,
                event: ev.clone(),
            },
        )
        .await;
    }
    let mut out = render(&deps, user, std::slice::from_ref(&ev), true).await?;
    Ok(Json(out.remove(0)))
}

/// Delete an event for good (`MANAGE_EVENTS`, or the creator with `CREATE_EVENTS`). Emits
/// `GUILD_SCHEDULED_EVENT_DELETE`.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/scheduled-events/{event_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("event_id" = String, Path, description = "Event snowflake"),
    ),
    responses((status = 204, description = "Deleted"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_event(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    MaybeExt(media): MaybeExt<MediaDeps>,
    Path((guild_id, event_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let eid: u64 = id("event_id", &event_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let ev = deps
        .service
        .delete_event(acc.perms, user, gid, eid, deps.event_world())
        .await
        .map_err(se)?;
    if let Some(h) = &ev.image_key {
        drop_cover(&media, eid, h).await;
    }
    announce(&deps, ScheduledEventDispatch::Delete, &ev).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Members interested in an event, ascending by user id (any member).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/scheduled-events/{event_id}/users",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("event_id" = String, Path, description = "Event snowflake"),
        ("limit" = Option<usize>, Query, description = "1 to 100, default 100"),
        ("with_member" = Option<bool>, Query, description = "Include the member object"),
        ("before" = Option<String>, Query, description = "Only users below this id"),
        ("after" = Option<String>, Query, description = "Only users above this id"),
    ),
    responses((status = 200, body = Vec<EventUserObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_interested(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, event_id)): Path<(String, String)>,
    ApiQuery(q): ApiQuery<InterestedQuery>,
) -> ApiResult<Json<Vec<EventUserObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let eid: u64 = id("event_id", &event_id)?;
    deps.guild_access(user, gid).await?;
    let before = q
        .before
        .as_deref()
        .map(|b| id::<UserId>("before", b))
        .transpose()?;
    let after = q
        .after
        .as_deref()
        .map(|a| id::<UserId>("after", a))
        .transpose()?;
    let ids = deps
        .service
        .list_interested(gid, eid, before, after, q.limit.unwrap_or(100))
        .await
        .map_err(se)?;
    let users = deps.users(ids.iter().copied()).await?;
    let mut out = Vec::with_capacity(ids.len());
    for uid in ids {
        let member = if q.with_member.unwrap_or(false) {
            deps.chat
                .get_member(gid, uid)
                .await
                .map_err(re)?
                .map(|m| MemberObject::new(&m, false, &users))
        } else {
            None
        };
        out.push(EventUserObject {
            guild_scheduled_event_id: eid.to_string(),
            user: UserRef::of(uid, &users),
            member,
        });
    }
    Ok(Json(out))
}

async fn rsvp(
    deps: &SettingsDeps,
    p: &Principal,
    guild_id: &str,
    event_id: &str,
    interested: bool,
) -> ApiResult<StatusCode> {
    let user = actor(p)?;
    let gid: GuildId = id("guild_id", guild_id)?;
    let eid: u64 = id("event_id", event_id)?;
    deps.guild_access(user, gid).await?;
    let (ev, changed) = deps
        .service
        .set_interested(gid, eid, user, interested)
        .await
        .map_err(se)?;
    if changed {
        announce_interest(deps, gid, ev.id, user, interested).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Mark yourself interested in an open event. Emits `GUILD_SCHEDULED_EVENT_USER_ADD`.
#[utoipa::path(
    put,
    path = "/guilds/{guild_id}/scheduled-events/{event_id}/users/@me",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("event_id" = String, Path, description = "Event snowflake"),
    ),
    responses((status = 204, description = "Marked"), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn mark_interested(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, event_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    rsvp(&deps, &p, &guild_id, &event_id, true).await
}

/// Clear your interest. Emits `GUILD_SCHEDULED_EVENT_USER_REMOVE`.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/scheduled-events/{event_id}/users/@me",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("event_id" = String, Path, description = "Event snowflake"),
    ),
    responses((status = 204, description = "Cleared"), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn clear_interested(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, event_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    rsvp(&deps, &p, &guild_id, &event_id, false).await
}
