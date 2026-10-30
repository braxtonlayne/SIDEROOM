//! The voice jukebox routes (MUS-JB-002): a queue per voice channel, vote-skip, a DJ hand-over and synced
//! local playback.
//!
//! * `GET /channels/{id}/jukebox`: the state, settled to now, filtered for the viewer.
//! * `PUT /channels/{id}/jukebox`: pick `stream` or `synced`.
//! * `POST /channels/{id}/jukebox/queue`, `DELETE /channels/{id}/jukebox/queue/{item}`.
//! * `POST|DELETE /channels/{id}/jukebox/skip`: vote to skip, or take the vote back.
//! * `POST /channels/{id}/jukebox/next`: skip outright (the DJ, a moderator, the one who queued it).
//! * `POST /channels/{id}/jukebox/ended`: a client reports that a track reached its end.
//! * `PUT /channels/{id}/jukebox/dj`, `PUT /channels/{id}/jukebox/playback`.
//! * `PUT|DELETE /channels/{id}/jukebox/listening`: join and heartbeat, leave.
//! * `POST /channels/{id}/jukebox/presign`: a presigned `GET` of the current (or next) track, for this
//!   listener only.
//!
//! Who is listening, speaking and moderating comes from the channel permissions (`CONNECT`, `SPEAK`,
//! `MANAGE_CHANNELS`). What a member may *hear* is a different question and is asked at every presign: the
//! stored mount id and key are re-checked against the storage scopes of the listener asking, so a track from
//! a bucket that listener cannot read is never signed for them and shows as "not shared with you". The
//! gateway event `JUKEBOX_UPDATE` carries ids and the version only; the state is read per viewer.

use crate::AppState;
use crate::auth::SessionOnly;
use crate::openapi::ErrorBody;
use crate::routes::admin::AdminDeps;
use crate::routes::messages::error::{ApiJson, MsgError, MsgResult, invalid};
use crate::routes::music::{MusicDeps, reaches, tracks::find_track};
use crate::routes::storage::objects::{PresignBody, PresignView};
use crate::routes::storage::{ScopeDeps, uid};
use crate::routes::{Ext, MaybeExt};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use sr_chat::jukebox::model::{
    DEFAULT_VOTE_PERCENT, Doc, Item, ItemState, JukeboxError, MAX_ADD_BATCH, Mode, NewTrack,
};
use sr_chat::jukebox::ports::{EVENT_JUKEBOX_UPDATE, JukeboxAccess, JukeboxEvents, VoiceAccess};
use sr_chat::jukebox::service::{Actor, JukeboxService, Snapshot};
use sr_core::{ApiError, ErrorCode};
use std::collections::HashSet;
use std::sync::Arc;
use utoipa::ToSchema;

/// Collaborators of the jukebox routes.
pub struct JukeboxDeps {
    /// Rules over the store.
    pub service: JukeboxService,
    /// Channel standing and the audience of a change.
    pub access: Arc<dyn JukeboxAccess>,
    /// Gateway dispatch.
    pub events: Arc<dyn JukeboxEvents>,
    /// The listening-room flag (`routes::voice::rooms`).
    pub rooms: Arc<dyn sr_chat::jukebox::ports::ListeningRooms>,
}

/// The routes, relative to the API prefix.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/channels/{channel_id}/jukebox", get(state).put(set_mode))
        .route("/channels/{channel_id}/jukebox/queue", post(enqueue))
        .route(
            "/channels/{channel_id}/jukebox/queue/{item_id}",
            axum::routing::delete(remove),
        )
        .route(
            "/channels/{channel_id}/jukebox/skip",
            post(vote_skip).delete(withdraw_vote),
        )
        .route("/channels/{channel_id}/jukebox/next", post(skip_now))
        .route("/channels/{channel_id}/jukebox/ended", post(ended))
        .route("/channels/{channel_id}/jukebox/dj", put(set_dj))
        .route("/channels/{channel_id}/jukebox/playback", put(set_playback))
        .route(
            "/channels/{channel_id}/jukebox/listening",
            put(listen).delete(leave),
        )
        .route("/channels/{channel_id}/jukebox/presign", post(presign))
}

// ---- wire types ------------------------------------------------------------------------------------

/// One queue row as the viewer may see it.
#[derive(Debug, Serialize, ToSchema)]
pub struct JukeboxItemView {
    /// Queue row snowflake.
    pub id: String,
    /// `playing`, `queued`, `played` or `skipped`.
    pub state: String,
    /// Who queued it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_by: Option<String>,
    /// The viewer can read the track's bucket now. `false` means "not shared with you": silence for them.
    pub playable: bool,
    /// Title; absent when the viewer cannot read the bucket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Artist; absent when the viewer cannot read the bucket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    /// Length in milliseconds, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
}

/// The jukebox of a voice channel, for one viewer.
#[derive(Debug, Serialize, ToSchema)]
pub struct JukeboxStateView {
    /// The voice channel.
    pub channel_id: String,
    /// `false` while nobody has queued anything in this channel.
    pub active: bool,
    /// `stream` (a DJ sends the audio into the call) or `synced` (everyone plays from a bucket they read).
    pub mode: String,
    /// The DJ.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dj_id: Option<String>,
    /// Changes with every change; clients refetch when it moves.
    pub version: i64,
    /// The server clock when this was read, unix ms. Compare with the local clock once and keep the offset.
    pub server_now_ms: i64,
    /// Position in the current track at `server_now_ms`.
    pub position_ms: i64,
    /// The server clock at which the current track was at position 0 (`server_now_ms - position_ms`) while it
    /// plays; absent while paused or idle. Listeners align their `currentTime` to `now - started_at_ms`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<i64>,
    /// The DJ paused the track.
    pub paused: bool,
    /// The playing track.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<JukeboxItemView>,
    /// Waiting tracks in play order.
    pub queue: Vec<JukeboxItemView>,
    /// Skip votes standing for the current track.
    pub votes: i64,
    /// Skip votes needed.
    pub votes_needed: i64,
    /// The viewer has voted.
    pub my_vote: bool,
    /// Members listening now.
    pub listeners: i64,
    /// The viewer holds the decks.
    pub i_am_dj: bool,
    /// The viewer may queue tracks and pick the mode.
    pub can_queue: bool,
    /// The viewer may pause, skip outright, remove any track and hand the decks over.
    pub can_control: bool,
}

/// Body of `PUT /channels/{id}/jukebox`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ModeBody {
    /// `stream` or `synced`.
    pub mode: String,
}

/// Body of `POST /channels/{id}/jukebox/queue`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct EnqueueBody {
    /// Index track snowflakes (`GET /music/library`), at most 50.
    pub track_ids: Vec<String>,
}

/// Body of `POST /channels/{id}/jukebox/ended`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct EndedBody {
    /// The queue row that reached its end.
    pub item_id: String,
}

/// Body of `PUT /channels/{id}/jukebox/dj`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct DjBody {
    /// The new DJ, a member who is listening; `null` lets go of the decks.
    #[serde(default)]
    pub user_id: Option<String>,
}

/// Body of `PUT /channels/{id}/jukebox/playback`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PlaybackBody {
    /// `true` pauses, `false` resumes.
    pub paused: bool,
}

/// Body of `POST /channels/{id}/jukebox/presign`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct JukeboxPresignBody {
    /// The queue row: the playing one, or the next waiting one (prefetch).
    pub item_id: String,
    /// A request range the client will send, `bytes=a-b`.
    #[serde(default)]
    pub range: Option<String>,
}

/// What a vote did.
#[derive(Debug, Serialize, ToSchema)]
pub struct VoteView {
    /// Votes standing.
    pub votes: i64,
    /// Votes needed.
    pub needed: i64,
    /// The vote skipped the track.
    pub skipped: bool,
}

/// Row ids a queue add made.
#[derive(Debug, Serialize, ToSchema)]
pub struct QueuedView {
    /// New queue row snowflakes, in request order.
    pub item_ids: Vec<String>,
}

// ---- helpers ---------------------------------------------------------------------------------------

fn not_found() -> MsgError {
    MsgError::Api(ApiError::with_message(
        ErrorCode::SrNotFound,
        "Unknown channel, track or queue entry",
    ))
}

fn conflict(code: &str, message: &str) -> MsgError {
    MsgError::Api(
        ApiError::with_message(ErrorCode::SrConflict, message).field("jukebox", code, message),
    )
}

fn failed(e: JukeboxError) -> MsgError {
    match e {
        JukeboxError::NotFound => not_found(),
        JukeboxError::QueueFull => conflict("QUEUE_FULL", "The queue is full."),
        JukeboxError::MemberQueueFull => {
            conflict("MEMBER_QUEUE_FULL", "You already have many tracks waiting.")
        }
        JukeboxError::NothingPlaying => conflict("NOTHING_PLAYING", "Nothing is playing."),
        JukeboxError::NotAllowed => ErrorCode::MissingPermissions.into(),
        JukeboxError::NotListening => {
            conflict("NOT_LISTENING", "That member is not listening right now.")
        }
        JukeboxError::Store(m) => {
            tracing::error!(error = %m, "jukebox store failed");
            ErrorCode::SrInternal.into()
        }
    }
}

fn row(raw: &str) -> MsgResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(not_found)
}

/// The caller's standing, or 404 (cannot see the channel or it has no jukebox) / 403 (may see it, not listen).
pub(super) async fn standing(j: &JukeboxDeps, user: i64, channel: i64) -> MsgResult<VoiceAccess> {
    let a = j.access.access(user, channel).await.map_err(|e| {
        tracing::error!(error = %e, "jukebox channel access failed");
        MsgError::from(ErrorCode::SrInternal)
    })?;
    match a {
        None => Err(not_found()),
        Some(a) if !a.listen => Err(ErrorCode::MissingPermissions.into()),
        Some(a) => Ok(a),
    }
}

fn need_speak(a: &VoiceAccess) -> MsgResult<()> {
    if a.speak {
        Ok(())
    } else {
        Err(ErrorCode::MissingPermissions.into())
    }
}

/// `jukebox.vote_skip_percent`, clamped; the default when the settings service is not wired.
async fn vote_percent(admin: Option<&AdminDeps>) -> i64 {
    match admin {
        Some(a) => a
            .service
            .setting_value_i64(sr_admin::settings::JUKEBOX_VOTE_SKIP_PERCENT)
            .await
            .ok()
            .filter(|v| (1..=100).contains(v))
            .unwrap_or(DEFAULT_VOTE_PERCENT),
        None => DEFAULT_VOTE_PERCENT,
    }
}

/// Tell the audience to read the state again.
async fn announce(j: &JukeboxDeps, snap: &Snapshot, changed: bool) {
    if !changed {
        return;
    }
    let channel = snap.doc.session.channel_id;
    let users = match j.access.audience(channel).await {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!(error = %e, "jukebox audience lookup failed");
            return;
        }
    };
    j.events
        .emit(
            users,
            EVENT_JUKEBOX_UPDATE,
            serde_json::json!({
                "channel_id": channel.to_string(),
                "guild_id": snap.doc.session.guild_id.to_string(),
                "version": snap.doc.session.version,
            }),
        )
        .await;
}

/// The mounts the viewer can read right now (a failed lookup admits nothing).
async fn readable(d: &ScopeDeps, m: &MusicDeps, user: i64) -> MsgResult<HashSet<i64>> {
    Ok(reaches(d, m, user)
        .await?
        .into_iter()
        .map(|r| r.mount.id)
        .collect())
}

fn item_view(it: &Item, readable: &HashSet<i64>) -> JukeboxItemView {
    let playable = readable.contains(&it.mount_id);
    JukeboxItemView {
        id: it.id.to_string(),
        state: it.state.as_str().to_owned(),
        added_by: it.added_by.map(|u| u.to_string()),
        playable,
        title: playable.then(|| it.title.clone()),
        artist: playable.then(|| it.artist.clone()),
        duration_ms: it.duration_ms,
    }
}

/// The state for one viewer. `snap` is `None` while the channel never used the jukebox.
fn state_view(
    channel: i64,
    snap: Option<&Snapshot>,
    viewer: i64,
    access: &VoiceAccess,
    readable: &HashSet<i64>,
    now_ms: i64,
) -> JukeboxStateView {
    let idle = Doc::new(channel, access.guild_id);
    let doc = snap.map_or(&idle, |s| &s.doc);
    let s = &doc.session;
    let position = snap.map_or(0, |x| x.position_ms);
    let current = doc.current().map(|i| item_view(i, readable));
    let i_am_dj = s.dj == Some(viewer);
    JukeboxStateView {
        channel_id: channel.to_string(),
        active: snap.is_some(),
        mode: s.mode.as_str().to_owned(),
        dj_id: s.dj.map(|u| u.to_string()),
        version: s.version,
        server_now_ms: snap.map_or(now_ms, |x| x.now_ms),
        position_ms: position,
        started_at_ms: (current.is_some() && !s.paused)
            .then(|| snap.map_or(now_ms, |x| x.now_ms) - position),
        paused: s.paused,
        current,
        queue: doc.queued().map(|i| item_view(i, readable)).collect(),
        votes: snap.map_or(0, |x| i64::try_from(x.votes).unwrap_or(0)),
        votes_needed: snap.map_or(1, |x| i64::try_from(x.needed).unwrap_or(1)),
        my_vote: s.current.is_some_and(|c| doc.votes.contains(&(c, viewer))),
        listeners: snap.map_or(0, |x| i64::try_from(x.listeners).unwrap_or(0)),
        i_am_dj,
        can_queue: access.speak,
        can_control: access.manage || i_am_dj,
    }
}

/// Answer a change with the state for its caller and announce it.
#[allow(clippy::too_many_arguments)]
async fn reply<T>(
    j: &JukeboxDeps,
    d: &ScopeDeps,
    m: &MusicDeps,
    user: i64,
    access: &VoiceAccess,
    done: sr_chat::jukebox::service::Done<T>,
) -> MsgResult<Json<JukeboxStateView>> {
    announce(j, &done.snapshot, done.changed).await;
    let ok = readable(d, m, user).await?;
    Ok(Json(state_view(
        done.snapshot.doc.session.channel_id,
        Some(&done.snapshot),
        user,
        access,
        &ok,
        done.snapshot.now_ms,
    )))
}

fn actor(user: i64, a: &VoiceAccess) -> Actor {
    Actor {
        user,
        manage: a.manage,
    }
}

// ---- handlers --------------------------------------------------------------------------------------

/// The jukebox of a voice channel. Needs `VIEW_CHANNEL` and `CONNECT`. A track in a bucket the caller cannot
/// read is listed without a title and with `playable: false`.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/jukebox",
    params(("channel_id" = String, Path, description = "Voice channel snowflake")),
    responses(
        (status = 200, body = JukeboxStateView),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn state(
    SessionOnly(p): SessionOnly,
    Ext(j): Ext<JukeboxDeps>,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    Path(channel_id): Path<String>,
) -> MsgResult<Json<JukeboxStateView>> {
    let user = uid(&p);
    let channel = row(&channel_id)?;
    let access = standing(&j, user, channel).await?;
    let percent = vote_percent(admin.as_deref()).await;
    let snap = j.service.state(channel, percent).await.map_err(failed)?;
    let ok = readable(&d, &m, user).await?;
    if let Some((s, changed)) = &snap {
        announce(&j, s, *changed).await;
    }
    let now = snap
        .as_ref()
        .map_or_else(|| j.service.now_ms(), |(s, _)| s.now_ms);
    Ok(Json(state_view(
        channel,
        snap.as_ref().map(|(s, _)| s),
        user,
        &access,
        &ok,
        now,
    )))
}

/// Switch between `stream` and `synced`. Needs `SPEAK`; while a DJ holds the decks only the DJ or a moderator
/// may.
#[utoipa::path(
    put,
    path = "/channels/{channel_id}/jukebox",
    params(("channel_id" = String, Path, description = "Voice channel snowflake")),
    request_body = ModeBody,
    responses(
        (status = 200, body = JukeboxStateView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn set_mode(
    SessionOnly(p): SessionOnly,
    Ext(j): Ext<JukeboxDeps>,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    Path(channel_id): Path<String>,
    ApiJson(body): ApiJson<ModeBody>,
) -> MsgResult<Json<JukeboxStateView>> {
    let user = uid(&p);
    let channel = row(&channel_id)?;
    let access = standing(&j, user, channel).await?;
    need_speak(&access)?;
    let mode = Mode::parse(&body.mode)
        .ok_or_else(|| invalid("mode", "ENUM_TYPE_COERCE", "Use stream or synced."))?;
    let percent = vote_percent(admin.as_deref()).await;
    let done = j
        .service
        .set_mode(
            channel,
            access.guild_id,
            actor(user, &access),
            mode,
            percent,
        )
        .await
        .map_err(failed)?;
    reply(&j, &d, &m, user, &access, done).await
}

/// Queue tracks from the caller's reachable library. Needs `SPEAK`. A track the caller cannot read is a 404,
/// the same answer as for one that does not exist.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/jukebox/queue",
    params(("channel_id" = String, Path, description = "Voice channel snowflake")),
    request_body = EnqueueBody,
    responses(
        (status = 200, body = JukeboxStateView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "QUEUE_FULL or MEMBER_QUEUE_FULL"),
    ),
    security(("session" = []))
)]
pub async fn enqueue(
    SessionOnly(p): SessionOnly,
    Ext(j): Ext<JukeboxDeps>,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    Path(channel_id): Path<String>,
    ApiJson(body): ApiJson<EnqueueBody>,
) -> MsgResult<Json<JukeboxStateView>> {
    let user = uid(&p);
    let channel = row(&channel_id)?;
    let access = standing(&j, user, channel).await?;
    need_speak(&access)?;
    if body.track_ids.is_empty() || body.track_ids.len() > MAX_ADD_BATCH {
        return Err(invalid(
            "track_ids",
            "BASE_TYPE_BAD_LENGTH",
            "Add between 1 and 50 tracks.",
        ));
    }
    let mut tracks = Vec::with_capacity(body.track_ids.len());
    for raw in &body.track_ids {
        let t = find_track(&d, &m, user, row(raw)?).await?;
        tracks.push(NewTrack {
            track_id: t.id,
            mount_id: t.mount_id,
            key: t.key,
            title: t.title,
            artist: t.artist,
            duration_ms: t.duration_ms.map(i64::from).filter(|v| *v > 0),
        });
    }
    let percent = vote_percent(admin.as_deref()).await;
    let done = j
        .service
        .enqueue(channel, access.guild_id, user, &tracks, percent)
        .await
        .map_err(failed)?;
    reply(&j, &d, &m, user, &access, done).await
}

/// Remove a queue entry: whoever queued it, the DJ or a moderator. Removing the playing track skips it.
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/jukebox/queue/{item_id}",
    params(
        ("channel_id" = String, Path, description = "Voice channel snowflake"),
        ("item_id" = String, Path, description = "Queue entry snowflake"),
    ),
    responses(
        (status = 200, body = JukeboxStateView),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn remove(
    SessionOnly(p): SessionOnly,
    Ext(j): Ext<JukeboxDeps>,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    Path((channel_id, item_id)): Path<(String, String)>,
) -> MsgResult<Json<JukeboxStateView>> {
    let user = uid(&p);
    let channel = row(&channel_id)?;
    let access = standing(&j, user, channel).await?;
    let percent = vote_percent(admin.as_deref()).await;
    let done = j
        .service
        .remove(
            channel,
            access.guild_id,
            actor(user, &access),
            row(&item_id)?,
            percent,
        )
        .await
        .map_err(failed)?;
    reply(&j, &d, &m, user, &access, done).await
}

/// Vote to skip the playing track. The vote counts against `jukebox.vote_skip_percent` of the members who are
/// listening; reaching it skips.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/jukebox/skip",
    params(("channel_id" = String, Path, description = "Voice channel snowflake")),
    responses(
        (status = 200, body = JukeboxStateView),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "NOTHING_PLAYING"),
    ),
    security(("session" = []))
)]
pub async fn vote_skip(
    SessionOnly(p): SessionOnly,
    Ext(j): Ext<JukeboxDeps>,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    Path(channel_id): Path<String>,
) -> MsgResult<Json<JukeboxStateView>> {
    let user = uid(&p);
    let channel = row(&channel_id)?;
    let access = standing(&j, user, channel).await?;
    let percent = vote_percent(admin.as_deref()).await;
    let done = j
        .service
        .vote_skip(channel, access.guild_id, user, percent)
        .await
        .map_err(failed)?;
    reply(&j, &d, &m, user, &access, done).await
}

/// Take a skip vote back.
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/jukebox/skip",
    params(("channel_id" = String, Path, description = "Voice channel snowflake")),
    responses(
        (status = 200, body = JukeboxStateView),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn withdraw_vote(
    SessionOnly(p): SessionOnly,
    Ext(j): Ext<JukeboxDeps>,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    Path(channel_id): Path<String>,
) -> MsgResult<Json<JukeboxStateView>> {
    let user = uid(&p);
    let channel = row(&channel_id)?;
    let access = standing(&j, user, channel).await?;
    let percent = vote_percent(admin.as_deref()).await;
    let done = j
        .service
        .withdraw_vote(channel, access.guild_id, user, percent)
        .await
        .map_err(failed)?;
    reply(&j, &d, &m, user, &access, done).await
}

/// Skip the playing track outright: the DJ, a moderator, or the member who queued it.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/jukebox/next",
    params(("channel_id" = String, Path, description = "Voice channel snowflake")),
    responses(
        (status = 200, body = JukeboxStateView),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "NOTHING_PLAYING"),
    ),
    security(("session" = []))
)]
pub async fn skip_now(
    SessionOnly(p): SessionOnly,
    Ext(j): Ext<JukeboxDeps>,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    Path(channel_id): Path<String>,
) -> MsgResult<Json<JukeboxStateView>> {
    let user = uid(&p);
    let channel = row(&channel_id)?;
    let access = standing(&j, user, channel).await?;
    let percent = vote_percent(admin.as_deref()).await;
    let done = j
        .service
        .force_skip(channel, access.guild_id, actor(user, &access), percent)
        .await
        .map_err(failed)?;
    reply(&j, &d, &m, user, &access, done).await
}

/// A listener reports that the playing track reached its end. Ignored unless it is the playing track and its
/// known end is near; the server clock decides, so a liar cannot skip.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/jukebox/ended",
    params(("channel_id" = String, Path, description = "Voice channel snowflake")),
    request_body = EndedBody,
    responses(
        (status = 200, body = JukeboxStateView),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn ended(
    SessionOnly(p): SessionOnly,
    Ext(j): Ext<JukeboxDeps>,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    Path(channel_id): Path<String>,
    ApiJson(body): ApiJson<EndedBody>,
) -> MsgResult<Json<JukeboxStateView>> {
    let user = uid(&p);
    let channel = row(&channel_id)?;
    let access = standing(&j, user, channel).await?;
    let percent = vote_percent(admin.as_deref()).await;
    let done = j
        .service
        .ended(channel, access.guild_id, row(&body.item_id)?, percent)
        .await
        .map_err(failed)?;
    reply(&j, &d, &m, user, &access, done).await
}

/// Claim the decks (while nobody holds them), hand them to a listening member, or let go (`null`). The DJ and
/// moderators may hand over at any time.
#[utoipa::path(
    put,
    path = "/channels/{channel_id}/jukebox/dj",
    params(("channel_id" = String, Path, description = "Voice channel snowflake")),
    request_body = DjBody,
    responses(
        (status = 200, body = JukeboxStateView),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "NOT_LISTENING"),
    ),
    security(("session" = []))
)]
pub async fn set_dj(
    SessionOnly(p): SessionOnly,
    Ext(j): Ext<JukeboxDeps>,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    Path(channel_id): Path<String>,
    ApiJson(body): ApiJson<DjBody>,
) -> MsgResult<Json<JukeboxStateView>> {
    let user = uid(&p);
    let channel = row(&channel_id)?;
    let access = standing(&j, user, channel).await?;
    need_speak(&access)?;
    let target = body
        .user_id
        .as_deref()
        .map(|raw| {
            raw.parse::<i64>()
                .ok()
                .filter(|v| *v > 0)
                .ok_or_else(|| invalid("user_id", "SNOWFLAKE_TYPE", "Not a member id."))
        })
        .transpose()?;
    let percent = vote_percent(admin.as_deref()).await;
    let done = j
        .service
        .set_dj(
            channel,
            access.guild_id,
            actor(user, &access),
            target,
            percent,
        )
        .await
        .map_err(failed)?;
    reply(&j, &d, &m, user, &access, done).await
}

/// Pause or resume the playing track: the DJ or a moderator.
#[utoipa::path(
    put,
    path = "/channels/{channel_id}/jukebox/playback",
    params(("channel_id" = String, Path, description = "Voice channel snowflake")),
    request_body = PlaybackBody,
    responses(
        (status = 200, body = JukeboxStateView),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "NOTHING_PLAYING"),
    ),
    security(("session" = []))
)]
pub async fn set_playback(
    SessionOnly(p): SessionOnly,
    Ext(j): Ext<JukeboxDeps>,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    Path(channel_id): Path<String>,
    ApiJson(body): ApiJson<PlaybackBody>,
) -> MsgResult<Json<JukeboxStateView>> {
    let user = uid(&p);
    let channel = row(&channel_id)?;
    let access = standing(&j, user, channel).await?;
    let percent = vote_percent(admin.as_deref()).await;
    let done = j
        .service
        .set_paused(
            channel,
            access.guild_id,
            actor(user, &access),
            body.paused,
            percent,
        )
        .await
        .map_err(failed)?;
    reply(&j, &d, &m, user, &access, done).await
}

/// Say you are listening (on joining the call, then every 30 s). Listeners are who a skip vote is counted
/// against; a listener not heard from for a minute stops counting.
#[utoipa::path(
    put,
    path = "/channels/{channel_id}/jukebox/listening",
    params(("channel_id" = String, Path, description = "Voice channel snowflake")),
    responses(
        (status = 200, body = JukeboxStateView),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn listen(
    SessionOnly(p): SessionOnly,
    Ext(j): Ext<JukeboxDeps>,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    Path(channel_id): Path<String>,
) -> MsgResult<Json<JukeboxStateView>> {
    let user = uid(&p);
    let channel = row(&channel_id)?;
    let access = standing(&j, user, channel).await?;
    let percent = vote_percent(admin.as_deref()).await;
    let done = j
        .service
        .listen(channel, access.guild_id, user, percent)
        .await
        .map_err(failed)?;
    reply(&j, &d, &m, user, &access, done).await
}

/// Stop listening: your skip vote and the decks (if you held them) go.
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/jukebox/listening",
    params(("channel_id" = String, Path, description = "Voice channel snowflake")),
    responses(
        (status = 200, body = JukeboxStateView),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn leave(
    SessionOnly(p): SessionOnly,
    Ext(j): Ext<JukeboxDeps>,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    Path(channel_id): Path<String>,
) -> MsgResult<Json<JukeboxStateView>> {
    let user = uid(&p);
    let channel = row(&channel_id)?;
    let access = standing(&j, user, channel).await?;
    let percent = vote_percent(admin.as_deref()).await;
    let done = j
        .service
        .leave(channel, access.guild_id, user, percent)
        .await
        .map_err(failed)?;
    reply(&j, &d, &m, user, &access, done).await
}

/// A presigned `GET` for the playing track (or the next waiting one, to prefetch), made for the caller alone.
/// The stored mount and key are checked against the caller's own storage scopes: a track in a bucket the
/// caller cannot read is a 404 ("not shared with you") and nothing is signed.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/jukebox/presign",
    params(("channel_id" = String, Path, description = "Voice channel snowflake")),
    request_body = JukeboxPresignBody,
    responses(
        (status = 200, body = PresignView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "CORS_NOT_READY"),
    ),
    security(("session" = []))
)]
#[allow(clippy::too_many_arguments)]
pub async fn presign(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(j): Ext<JukeboxDeps>,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
    ApiJson(body): ApiJson<JukeboxPresignBody>,
) -> MsgResult<Json<PresignView>> {
    let user = uid(&p);
    let channel = row(&channel_id)?;
    standing(&j, user, channel).await?;
    let item_id = row(&body.item_id)?;
    let percent = vote_percent(admin.as_deref()).await;
    let Some((snap, changed)) = j.service.state(channel, percent).await.map_err(failed)? else {
        return Err(not_found());
    };
    announce(&j, &snap, changed).await;
    let ok = readable(&d, &m, user).await?;
    let it = presignable(&snap.doc, item_id, &ok)?;
    crate::routes::storage::objects::presign(
        State(state),
        SessionOnly(p),
        Ext(d),
        MaybeExt(admin),
        headers,
        Path(it.mount_id.to_string()),
        ApiJson(PresignBody {
            key: it.key.clone(),
            method: "GET".into(),
            range: body.range,
        }),
    )
    .await
}

/// OpenAPI fragment of the jukebox routes.
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        state,
        set_mode,
        enqueue,
        remove,
        vote_skip,
        withdraw_vote,
        skip_now,
        ended,
        set_dj,
        set_playback,
        listen,
        leave,
        presign,
    ),
    components(schemas(
        ErrorBody,
        JukeboxItemView,
        JukeboxStateView,
        ModeBody,
        EnqueueBody,
        EndedBody,
        DjBody,
        PlaybackBody,
        JukeboxPresignBody,
        PresignView,
    ))
)]
pub struct JukeboxApiDoc;

/// The OpenAPI fragment of the jukebox routes.
pub fn openapi() -> utoipa::openapi::OpenApi {
    <JukeboxApiDoc as utoipa::OpenApi>::openapi()
}

/// The queue row a listener may be given a signature for: the playing track or the next waiting one (a
/// prefetch; a queue is not a download list), and only when the listener's own reach includes its mount. The
/// adder's reach never counts: no read access, no signature, and the answer is the same 404 as for an
/// unknown row.
pub(crate) fn presignable<'a>(
    doc: &'a Doc,
    item_id: i64,
    readable: &HashSet<i64>,
) -> MsgResult<&'a Item> {
    let next = doc.queued().next().map(|i| i.id);
    doc.items
        .iter()
        .find(|i| i.id == item_id)
        .filter(|i| i.state == ItemState::Playing || Some(i.id) == next)
        .filter(|i| readable.contains(&i.mount_id))
        .ok_or_else(not_found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_chat::jukebox::model::NewTrack;
    use sr_chat::jukebox::rules;

    fn tr(n: i64, mount: i64) -> NewTrack {
        NewTrack {
            track_id: n,
            mount_id: mount,
            key: format!("m{mount}/{n}.flac"),
            title: format!("Song {n}"),
            artist: "Band".into(),
            duration_ms: Some(180_000),
        }
    }

    /// Alice (1) queued three tracks: 10 plays, 11 is next, 12 waits behind. Bucket 7 is hers alone.
    fn doc() -> Doc {
        let mut d = Doc::new(5, 6);
        let mut n = 100;
        rules::enqueue(
            &mut d,
            1,
            &[tr(10, 7), tr(11, 8), tr(12, 7)],
            &mut || {
                n += 1;
                n
            },
            1_000,
        )
        .unwrap();
        d
    }

    fn set(ids: &[i64]) -> HashSet<i64> {
        ids.iter().copied().collect()
    }

    #[test]
    fn a_track_in_a_bucket_the_listener_cannot_read_is_never_signed_for_them() {
        let d = doc();
        // Bob reads bucket 8 only: the playing track (bucket 7) is not shared with him.
        assert!(presignable(&d, 101, &set(&[8])).is_err());
        // Alice reads both.
        assert_eq!(presignable(&d, 101, &set(&[7, 8])).unwrap().id, 101);
        // Nobody reads anything: nothing is signed.
        assert!(presignable(&d, 101, &set(&[])).is_err());
    }

    #[test]
    fn only_the_playing_and_the_next_track_can_be_signed() {
        let d = doc();
        let all = set(&[7, 8]);
        assert!(presignable(&d, 101, &all).is_ok(), "playing");
        assert!(presignable(&d, 102, &all).is_ok(), "next, for the prefetch");
        assert!(
            presignable(&d, 103, &all).is_err(),
            "further down the queue"
        );
        assert!(presignable(&d, 999, &all).is_err(), "unknown row");
    }

    #[test]
    fn the_state_names_only_what_the_viewer_may_read() {
        let d = doc();
        let access = VoiceAccess {
            guild_id: 6,
            listen: true,
            speak: false,
            manage: false,
        };
        let snap = Snapshot {
            now_ms: 4_000,
            position_ms: rules::position_at(&d, 4_000),
            votes: 0,
            needed: 1,
            listeners: 1,
            doc: d,
        };
        let v = state_view(5, Some(&snap), 2, &access, &set(&[8]), 4_000);
        let cur = v.current.as_ref().unwrap();
        assert!(!cur.playable);
        assert!(cur.title.is_none() && cur.artist.is_none());
        assert_eq!(cur.duration_ms, Some(180_000));
        assert_eq!(v.queue[0].title.as_deref(), Some("Song 11"));
        assert!(v.queue[1].title.is_none());
        assert_eq!(v.position_ms, 3_000);
        assert_eq!(v.started_at_ms, Some(1_000));
        assert!(!v.can_queue && !v.can_control && !v.i_am_dj);
    }

    #[test]
    fn an_idle_channel_reads_as_empty_not_as_an_error() {
        let access = VoiceAccess {
            guild_id: 6,
            listen: true,
            speak: true,
            manage: false,
        };
        let v = state_view(5, None, 2, &access, &set(&[]), 9_000);
        assert!(!v.active && v.current.is_none() && v.queue.is_empty());
        assert_eq!((v.server_now_ms, v.started_at_ms), (9_000, None));
        assert!(v.can_queue);
    }

    #[test]
    fn a_paused_track_has_no_start_instant() {
        let mut d = doc();
        rules::pause(&mut d, 3_000).unwrap();
        let access = VoiceAccess {
            guild_id: 6,
            listen: true,
            speak: true,
            manage: true,
        };
        let snap = Snapshot {
            now_ms: 9_000,
            position_ms: rules::position_at(&d, 9_000),
            votes: 0,
            needed: 1,
            listeners: 1,
            doc: d,
        };
        let v = state_view(5, Some(&snap), 1, &access, &set(&[7, 8]), 9_000);
        assert!(v.paused);
        assert_eq!((v.position_ms, v.started_at_ms), (2_000, None));
        assert!(v.can_control);
    }

    #[test]
    fn errors_map_to_the_wire() {
        assert!(matches!(failed(JukeboxError::NotFound), MsgError::Api(_)));
        let q = format!("{:?}", failed(JukeboxError::QueueFull));
        assert!(q.contains("QUEUE_FULL"), "{q}");
        let d = format!("{:?}", failed(JukeboxError::NotAllowed));
        assert!(
            d.contains("MissingPermissions") || d.contains("50013"),
            "{d}"
        );
        assert!(row("0").is_err() && row("-4").is_err() && row("x").is_err());
        assert_eq!(row("12").unwrap(), 12);
    }
}
