//! Notification settings per guild (or the DM scope) and per channel (LNG-NTF-001,
//! Q116). `guild_id` `@me` addresses the DM scope, as Discord does.

use super::deps::SocialDeps;
use super::error::{ApiJson, MsgResult, invalid};
use super::events::SocialEvent;
use super::model::{ChannelOverrideBody, GuildSettingsBody, MuteConfigIn, UserGuildSettingsObject};
use super::util::{actor, channel_id};
use crate::routes::messages::MessagesDeps;
use crate::{Principal, openapi::ErrorBody, routes::Ext};
use axum::{Json, extract::Path};
use serde_json::Value;
use sr_chat::ChatError;
use sr_chat::social::notifications::{ChannelOverride, MuteDuration, NotificationLevel};
use sr_chat::social::repo::NotificationRepository;
use sr_chat::social::service::ScopeUpdate;
use sr_core::{ErrorCode, GuildId, UserId};

/// Settings of one guild (`@me` for DMs) with the caller's channel overrides in it.
#[utoipa::path(
    get,
    path = "/users/@me/guilds/{guild_id}/settings",
    params(("guild_id" = String, Path, description = "Guild snowflake, or `@me` for DMs")),
    responses((status = 200, body = UserGuildSettingsObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_settings(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Path(guild): Path<String>,
) -> MsgResult<Json<UserGuildSettingsObject>> {
    let me = actor(&p)?;
    let scope = resolve_scope(&messages, me, &guild).await?;
    Ok(Json(render(&deps, &messages, me, scope).await?))
}

/// Change notification level, mutes and suppression toggles of a guild, plus
/// overrides for channels in it. A mute is `muted: true` with an optional
/// `mute_config.selected_time_window` (seconds, or -1 until turned off).
#[utoipa::path(
    patch,
    path = "/users/@me/guilds/{guild_id}/settings",
    params(("guild_id" = String, Path, description = "Guild snowflake, or `@me` for DMs")),
    request_body = GuildSettingsBody,
    responses((status = 200, body = UserGuildSettingsObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn patch_settings(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Path(guild): Path<String>,
    ApiJson(body): ApiJson<GuildSettingsBody>,
) -> MsgResult<Json<UserGuildSettingsObject>> {
    let me = actor(&p)?;
    let scope = resolve_scope(&messages, me, &guild).await?;
    // Validate everything before writing anything.
    let level = match body.message_notifications {
        None => None,
        Some(l) => Some(level_from(l, "message_notifications")?),
    };
    let mute = mute_change(body.muted, body.mute_config.as_ref(), "mute_config")?;
    let mut overrides = Vec::new();
    for (raw, ov) in body.channel_overrides.iter().flatten() {
        let cid = channel_id(raw)?;
        let acc = messages.access(me, cid).await?;
        acc.require_view()?;
        if acc.channel.guild_id != scope {
            return Err(invalid(
                "channel_overrides",
                "CHANNEL_NOT_IN_GUILD",
                "Channel does not belong to this guild.",
            ));
        }
        overrides.push((cid, override_change(ov)?));
    }

    let update = ScopeUpdate {
        level,
        suppress_everyone: body.suppress_everyone,
        suppress_roles: body.suppress_roles,
        suppress_highlights: body.suppress_highlights,
        mute_scheduled_events: body.mute_scheduled_events,
        mobile_push: body.mobile_push,
    };
    deps.service.update_scope(me, scope, &update).await?;
    if let Some(m) = mute {
        deps.service.mute_scope(me, scope, m).await?;
    }
    for (cid, (level, mute)) in overrides {
        deps.service.update_override(me, cid, level, mute).await?;
    }
    let out = render(&deps, &messages, me, scope).await?;
    deps.emit(SocialEvent::new(
        "USER_GUILD_SETTINGS_UPDATE",
        vec![me],
        serde_json::to_value(&out).unwrap_or(Value::Null),
    ))
    .await;
    Ok(Json(out))
}

/// `@me` is the DM scope; anything else must be a guild the caller belongs to.
async fn resolve_scope(
    messages: &MessagesDeps,
    me: UserId,
    raw: &str,
) -> MsgResult<Option<GuildId>> {
    if raw == "@me" {
        return Ok(None);
    }
    let gid: GuildId = raw.parse().map_err(|_| ErrorCode::UnknownGuild)?;
    messages
        .repo
        .get_guild(gid)
        .await?
        .ok_or(ChatError::UnknownGuild)?;
    messages
        .repo
        .get_member(gid, me)
        .await?
        .ok_or(ChatError::MissingAccess)?;
    Ok(Some(gid))
}

/// Scope settings plus the overrides whose channel lives in that scope.
async fn render(
    deps: &SocialDeps,
    messages: &MessagesDeps,
    me: UserId,
    scope: Option<GuildId>,
) -> MsgResult<UserGuildSettingsObject> {
    let settings = deps.service.scope_settings(me, scope).await?;
    let mut mine: Vec<ChannelOverride> = Vec::new();
    for o in deps.service.repo().list_overrides(me).await? {
        if let Some(ch) = messages.repo.get_channel(o.channel).await?
            && ch.guild_id == scope
        {
            mine.push(o);
        }
    }
    Ok(UserGuildSettingsObject::new(&settings, &mine))
}

fn level_from(code: Option<i16>, field: &str) -> MsgResult<Option<NotificationLevel>> {
    match code {
        None => Ok(None),
        Some(c) => NotificationLevel::from_code(c).map(Some).ok_or_else(|| {
            invalid(
                field,
                "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
                "Must be 0 (all), 1 (only mentions) or 2 (nothing).",
            )
        }),
    }
}

/// `Some(None)` unmute, `Some(Some(d))` mute, `None` leave alone.
fn mute_change(
    muted: Option<bool>,
    config: Option<&MuteConfigIn>,
    field: &str,
) -> MsgResult<Option<Option<MuteDuration>>> {
    let duration = |c: Option<&MuteConfigIn>| -> MsgResult<MuteDuration> {
        match c {
            None => Ok(MuteDuration::UntilTurnedOff),
            Some(c) => c.duration().ok_or_else(|| {
                invalid(
                    field,
                    "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
                    "selected_time_window must be 900, 3600, 10800, 28800, 86400 or -1.",
                )
            }),
        }
    };
    Ok(match (muted, config) {
        (Some(false), _) => Some(None),
        (Some(true), c) | (None, c @ Some(_)) => Some(Some(duration(c)?)),
        (None, None) => None,
    })
}

type OverrideChange = (
    Option<Option<NotificationLevel>>,
    Option<Option<MuteDuration>>,
);

fn override_change(o: &ChannelOverrideBody) -> MsgResult<OverrideChange> {
    let level = match o.message_notifications {
        None => None,
        Some(l) => Some(level_from(l, "message_notifications")?),
    };
    let mute = mute_change(o.muted, o.mute_config.as_ref(), "mute_config")?;
    Ok((level, mute))
}
