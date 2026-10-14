//! Handlers of the Watch Party API. Each is a thin translation to `PartyService`.

use super::{Ctx, WpResult, id};
use crate::openapi::ErrorBody;
use axum::{Json, extract::Path, http::StatusCode};
use serde::Serialize;
use sr_watchparty::model::*;
use utoipa::ToSchema;

/// The 409 body of a stale `expected_version`: the usual error object plus the current playback.
#[derive(Debug, Serialize, ToSchema)]
pub struct VersionConflictBody {
    pub code: i32,
    pub message: String,
    pub playback: PlaybackView,
}

const PARTY: &str = "Watch party snowflake";

/// The server clock. Clients sample it (with their own round-trip time) to estimate their clock offset
/// before and while they are in a party.
#[utoipa::path(
    get, path = "/watch-parties/time", operation_id = "watch_party_time",
    responses((status = 200, body = TimeView), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn time(ctx: Ctx) -> Json<TimeView> {
    Json(ctx.svc().time())
}

/// The party the caller is in now.
#[utoipa::path(
    get, path = "/watch-parties/me", operation_id = "watch_party_mine",
    responses((status = 200, body = PartyView), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody, description = "Not in a party.")),
    security(("session" = []))
)]
pub async fn mine(ctx: Ctx) -> WpResult<Json<PartyView>> {
    Ok(Json(ctx.svc().mine(ctx.viewer).await?))
}

/// Start a party: bound to a channel or DM the caller may start one in (`START_WATCH_PARTY`), or private
/// (shareable link). The title must be published and playable; a restricted one needs the caller's age
/// confirmation (403, code 900011). The caller becomes the host and leaves any other party.
#[utoipa::path(
    post, path = "/watch-parties", operation_id = "watch_party_create", request_body = CreateInput,
    responses((status = 201, body = PartyView), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "No START_WATCH_PARTY, or age confirmation required (900011)."),
              (status = 404, body = ErrorBody, description = "Unknown channel or title."),
              (status = 409, body = ErrorBody, description = "The channel already has a party, or the server is at its party limit."),
              (status = 429, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create(
    ctx: Ctx,
    Json(body): Json<CreateInput>,
) -> WpResult<(StatusCode, Json<PartyView>)> {
    Ok((
        StatusCode::CREATED,
        Json(ctx.svc().create(ctx.viewer, body).await?),
    ))
}

/// A party. Members, and people who can see the bound channel, may read it; restricted content is masked for
/// viewers without age confirmation.
#[utoipa::path(
    get, path = "/watch-parties/{party_id}", operation_id = "watch_party_get",
    params(("party_id" = String, Path, description = PARTY)),
    responses((status = 200, body = PartyView), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn get_party(ctx: Ctx, Path(party_id): Path<String>) -> WpResult<Json<PartyView>> {
    Ok(Json(
        ctx.svc()
            .get(ctx.viewer, id(&party_id, "watch party")?)
            .await?,
    ))
}

/// The live party of a channel or DM.
#[utoipa::path(
    get, path = "/channels/{channel_id}/watch-party", operation_id = "watch_party_in_channel",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = PartyView), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody, description = "No party, or the channel is not visible.")),
    security(("session" = []))
)]
pub async fn in_channel(ctx: Ctx, Path(channel_id): Path<String>) -> WpResult<Json<PartyView>> {
    Ok(Json(
        ctx.svc()
            .in_channel(ctx.viewer, id(&channel_id, "channel")?)
            .await?,
    ))
}

/// Change the settings (host or channel manager).
#[utoipa::path(
    patch, path = "/watch-parties/{party_id}", operation_id = "watch_party_update", request_body = SettingsInput,
    params(("party_id" = String, Path, description = PARTY)),
    responses((status = 200, body = PartyView), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update_settings(
    ctx: Ctx,
    Path(party_id): Path<String>,
    Json(body): Json<SettingsInput>,
) -> WpResult<Json<PartyView>> {
    Ok(Json(
        ctx.svc()
            .update_settings(ctx.viewer, id(&party_id, "watch party")?, body)
            .await?,
    ))
}

/// End the party for everybody (host or channel manager).
#[utoipa::path(
    delete, path = "/watch-parties/{party_id}", operation_id = "watch_party_end",
    params(("party_id" = String, Path, description = PARTY)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn end(ctx: Ctx, Path(party_id): Path<String>) -> WpResult<StatusCode> {
    ctx.svc()
        .end(ctx.viewer, id(&party_id, "watch party")?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Join. A private party needs `invite_code`; a bound party needs access to its channel; a restricted title
/// needs the caller's age confirmation (403, 900011). Joining leaves the caller's other party.
#[utoipa::path(
    post, path = "/watch-parties/{party_id}/join", operation_id = "watch_party_join", request_body = JoinInput,
    params(("party_id" = String, Path, description = PARTY)),
    responses((status = 200, body = PartyView), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "The party is full."), (status = 429, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn join(
    ctx: Ctx,
    Path(party_id): Path<String>,
    body: Option<Json<JoinInput>>,
) -> WpResult<Json<PartyView>> {
    let body = body.map(|Json(b)| b).unwrap_or_default();
    Ok(Json(
        ctx.svc()
            .join(ctx.viewer, id(&party_id, "watch party")?, body)
            .await?,
    ))
}

/// Leave. The host role passes to the next person; an empty party ends after 5 minutes unless persistent.
#[utoipa::path(
    post, path = "/watch-parties/{party_id}/leave", operation_id = "watch_party_leave",
    params(("party_id" = String, Path, description = PARTY)),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn leave(ctx: Ctx, Path(party_id): Path<String>) -> WpResult<StatusCode> {
    ctx.svc()
        .leave(ctx.viewer, id(&party_id, "watch party")?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Hand the host role to another participant (host or channel manager).
#[utoipa::path(
    post, path = "/watch-parties/{party_id}/host", operation_id = "watch_party_transfer_host", request_body = TransferInput,
    params(("party_id" = String, Path, description = PARTY)),
    responses((status = 200, body = PartyView), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn transfer_host(
    ctx: Ctx,
    Path(party_id): Path<String>,
    Json(body): Json<TransferInput>,
) -> WpResult<Json<PartyView>> {
    Ok(Json(
        ctx.svc()
            .transfer_host(ctx.viewer, id(&party_id, "watch party")?, body.user_id)
            .await?,
    ))
}

/// Name or unname a controller (host or channel manager).
#[utoipa::path(
    put, path = "/watch-parties/{party_id}/controllers/{user_id}", operation_id = "watch_party_set_controller",
    request_body = ControllerInput,
    params(("party_id" = String, Path, description = PARTY), ("user_id" = String, Path, description = "User snowflake")),
    responses((status = 200, body = PartyView), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn set_controller(
    ctx: Ctx,
    Path((party_id, user_id)): Path<(String, String)>,
    Json(body): Json<ControllerInput>,
) -> WpResult<Json<PartyView>> {
    Ok(Json(
        ctx.svc()
            .set_controller(
                ctx.viewer,
                id(&party_id, "watch party")?,
                id(&user_id, "participant")?,
                body.controller,
            )
            .await?,
    ))
}

/// Remove a participant and bar them from rejoining (host or channel manager).
#[utoipa::path(
    delete, path = "/watch-parties/{party_id}/members/{user_id}", operation_id = "watch_party_kick",
    params(("party_id" = String, Path, description = PARTY), ("user_id" = String, Path, description = "User snowflake")),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn kick(
    ctx: Ctx,
    Path((party_id, user_id)): Path<(String, String)>,
) -> WpResult<StatusCode> {
    ctx.svc()
        .kick(
            ctx.viewer,
            id(&party_id, "watch party")?,
            id(&user_id, "participant")?,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A playback command: `play`, `pause`, `seek` (`position_ms`), `rate` (0.5 to 2) or `ended`. Allowed for
/// the host and, per the shared-control setting, controllers or everybody; otherwise 403. A stale
/// `expected_version` answers 409 with the current playback. The answer is the state after the command.
#[utoipa::path(
    post, path = "/watch-parties/{party_id}/playback", operation_id = "watch_party_command", request_body = CommandInput,
    params(("party_id" = String, Path, description = PARTY)),
    responses((status = 200, body = PlaybackView), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody),
              (status = 409, body = VersionConflictBody, description = "`expected_version` is stale."),
              (status = 429, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn command(
    ctx: Ctx,
    Path(party_id): Path<String>,
    Json(body): Json<CommandInput>,
) -> WpResult<Json<PlaybackView>> {
    Ok(Json(
        ctx.svc()
            .command(ctx.viewer, id(&party_id, "watch party")?, body)
            .await?,
    ))
}

/// Change what is playing (controllers). The title must be published and playable. For a restricted title,
/// participants without age confirmation are removed from the party.
#[utoipa::path(
    put, path = "/watch-parties/{party_id}/media", operation_id = "watch_party_change_media", request_body = ChangeMediaInput,
    params(("party_id" = String, Path, description = PARTY)),
    responses((status = 200, body = PartyView), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody, description = "Not a controller, or age confirmation required (900011)."),
              (status = 404, body = ErrorBody), (status = 429, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn change_media(
    ctx: Ctx,
    Path(party_id): Path<String>,
    Json(body): Json<ChangeMediaInput>,
) -> WpResult<Json<PartyView>> {
    Ok(Json(
        ctx.svc()
            .change_media(ctx.viewer, id(&party_id, "watch party")?, body)
            .await?,
    ))
}

/// Add to the queue (per the queue policy).
#[utoipa::path(
    post, path = "/watch-parties/{party_id}/queue", operation_id = "watch_party_queue_add", request_body = QueueAddInput,
    params(("party_id" = String, Path, description = PARTY)),
    responses((status = 200, body = PartyView), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "The queue is full.")),
    security(("session" = []))
)]
pub async fn queue_add(
    ctx: Ctx,
    Path(party_id): Path<String>,
    Json(body): Json<QueueAddInput>,
) -> WpResult<Json<PartyView>> {
    Ok(Json(
        ctx.svc()
            .queue_add(ctx.viewer, id(&party_id, "watch party")?, body)
            .await?,
    ))
}

/// Reorder the queue (controllers): move an entry to `position`.
#[utoipa::path(
    patch, path = "/watch-parties/{party_id}/queue/{item_id}", operation_id = "watch_party_queue_move",
    request_body = QueueMoveInput,
    params(("party_id" = String, Path, description = PARTY), ("item_id" = String, Path, description = "Queue entry snowflake")),
    responses((status = 200, body = PartyView), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn queue_move(
    ctx: Ctx,
    Path((party_id, item_id)): Path<(String, String)>,
    Json(body): Json<QueueMoveInput>,
) -> WpResult<Json<PartyView>> {
    Ok(Json(
        ctx.svc()
            .queue_move(
                ctx.viewer,
                id(&party_id, "watch party")?,
                id(&item_id, "queue item")?,
                body,
            )
            .await?,
    ))
}

/// Remove a queue entry (per the queue policy; the person who added it may always).
#[utoipa::path(
    delete, path = "/watch-parties/{party_id}/queue/{item_id}", operation_id = "watch_party_queue_remove",
    params(("party_id" = String, Path, description = PARTY), ("item_id" = String, Path, description = "Queue entry snowflake")),
    responses((status = 200, body = PartyView), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn queue_remove(
    ctx: Ctx,
    Path((party_id, item_id)): Path<(String, String)>,
) -> WpResult<Json<PartyView>> {
    Ok(Json(
        ctx.svc()
            .queue_remove(
                ctx.viewer,
                id(&party_id, "watch party")?,
                id(&item_id, "queue item")?,
            )
            .await?,
    ))
}

/// Skip to the next queue entry (controllers).
#[utoipa::path(
    post, path = "/watch-parties/{party_id}/next", operation_id = "watch_party_next",
    params(("party_id" = String, Path, description = PARTY)),
    responses((status = 200, body = PartyView), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "The queue is empty.")),
    security(("session" = []))
)]
pub async fn next(ctx: Ctx, Path(party_id): Path<String>) -> WpResult<Json<PartyView>> {
    Ok(Json(
        ctx.svc()
            .next(ctx.viewer, id(&party_id, "watch party")?)
            .await?,
    ))
}

/// The participant's heartbeat (every 15 to 30 s): liveness, round-trip estimate, buffering and ready
/// flags. Answers with the server clock and the current playback. A participant silent for 90 s is dropped.
#[utoipa::path(
    post, path = "/watch-parties/{party_id}/heartbeat", operation_id = "watch_party_heartbeat", request_body = HeartbeatInput,
    params(("party_id" = String, Path, description = PARTY)),
    responses((status = 200, body = HeartbeatResult), (status = 404, body = ErrorBody), (status = 429, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn heartbeat(
    ctx: Ctx,
    Path(party_id): Path<String>,
    body: Option<Json<HeartbeatInput>>,
) -> WpResult<Json<HeartbeatResult>> {
    let body = body.map(|Json(b)| b).unwrap_or_default();
    Ok(Json(
        ctx.svc()
            .heartbeat(ctx.viewer, id(&party_id, "watch party")?, body)
            .await?,
    ))
}

/// Send a floating emoji reaction to the party (rate limited).
#[utoipa::path(
    post, path = "/watch-parties/{party_id}/reactions", operation_id = "watch_party_react", request_body = ReactionInput,
    params(("party_id" = String, Path, description = PARTY)),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody), (status = 429, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn react(
    ctx: Ctx,
    Path(party_id): Path<String>,
    Json(body): Json<ReactionInput>,
) -> WpResult<StatusCode> {
    ctx.svc()
        .react(ctx.viewer, id(&party_id, "watch party")?, body)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Send an ephemeral message to the party's own chat (used by private parties; parties bound to a channel
/// use that channel's text chat). Not stored.
#[utoipa::path(
    post, path = "/watch-parties/{party_id}/messages", operation_id = "watch_party_say", request_body = MessageInput,
    params(("party_id" = String, Path, description = PARTY)),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody), (status = 429, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn say(
    ctx: Ctx,
    Path(party_id): Path<String>,
    Json(body): Json<MessageInput>,
) -> WpResult<StatusCode> {
    ctx.svc()
        .say(ctx.viewer, id(&party_id, "watch party")?, body)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The audit trail of party-level actions, newest first (host or channel manager).
#[utoipa::path(
    get, path = "/watch-parties/{party_id}/audit", operation_id = "watch_party_audit",
    params(("party_id" = String, Path, description = PARTY)),
    responses((status = 200, body = Vec<AuditEntry>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn audit(ctx: Ctx, Path(party_id): Path<String>) -> WpResult<Json<Vec<AuditEntry>>> {
    Ok(Json(
        ctx.svc()
            .audit_log(ctx.viewer, id(&party_id, "watch party")?)
            .await?,
    ))
}
