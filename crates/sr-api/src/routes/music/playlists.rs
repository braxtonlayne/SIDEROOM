//! Playlists (MUS-LIB-001): a member's own and a server's.
//!
//! Who may do what:
//! * A member's playlist is the owner's alone; anyone else gets 404 (no disclosure).
//! * A server's playlist is readable by the server's members (404 for others) and editable with the storage
//!   `upload` or `manage` grant (403 for a member without it), the same grant that uploads to the server's
//!   shelf.
//! * Putting a track on a list needs read access to its mount now; reading a list hides the tracks the viewer
//!   cannot read (counted in `hidden`).

use super::{
    GuildRights, MAX_ADD_BATCH, MusicLibDeps, PlaylistTrackView, check_tracks, forbidden,
    lib_failed, not_found, parse_row, reach_failed,
};
use crate::{
    auth::SessionOnly,
    openapi::ErrorBody,
    routes::{
        Ext, MaybeExt,
        admin::AdminDeps,
        messages::error::{ApiJson, MsgResult, invalid},
    },
};
use axum::{Json, extract::Path, http::StatusCode};
use serde::{Deserialize, Serialize};
use sr_storage::music_lib::{PlaylistOwner, PlaylistRow};
use utoipa::ToSchema;

/// A playlist.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlaylistView {
    /// Playlist snowflake.
    pub id: String,
    /// `user` or `guild`.
    pub owner_type: String,
    /// The member or the server.
    pub owner_id: String,
    /// 1 to 100 characters.
    pub name: String,
    /// Up to 500 characters.
    pub description: String,
    /// Who made it, when the account still exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Items on the list (some may be hidden from the viewer).
    pub item_count: i64,
    /// Creation time, unix milliseconds.
    pub created_ms: i64,
    /// Last change, unix milliseconds.
    pub updated_ms: i64,
    /// Whether the viewer may edit it.
    pub can_edit: bool,
}

impl PlaylistView {
    fn of(p: PlaylistRow, can_edit: bool) -> Self {
        let (owner_type, owner_id) = match p.owner {
            PlaylistOwner::User(u) => ("user", u),
            PlaylistOwner::Guild(g) => ("guild", g),
        };
        Self {
            id: p.id.to_string(),
            owner_type: owner_type.to_owned(),
            owner_id: owner_id.to_string(),
            name: p.name,
            description: p.description,
            created_by: p.created_by.map(|c| c.to_string()),
            item_count: p.item_count,
            created_ms: p.created_ms,
            updated_ms: p.updated_ms,
            can_edit,
        }
    }
}

/// An item of a playlist.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlaylistItemView {
    /// The item's own snowflake (a track may be on a list twice).
    pub item_id: String,
    /// Zero-based position among all items (hidden ones included).
    pub position: i32,
    /// The track.
    pub track: PlaylistTrackView,
}

/// A playlist with the items the viewer can reach.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlaylistDetailView {
    /// The playlist.
    pub playlist: PlaylistView,
    /// The reachable items in order.
    pub items: Vec<PlaylistItemView>,
    /// Items left out because their bucket is not readable by the viewer.
    pub hidden: i64,
}

/// Body of `POST` for a new playlist.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PlaylistBody {
    /// 1 to 100 characters.
    pub name: String,
    /// Up to 500 characters.
    #[serde(default)]
    pub description: Option<String>,
}

/// Body of `PATCH` for a playlist.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PlaylistPatchBody {
    /// 1 to 100 characters.
    #[serde(default)]
    pub name: Option<String>,
    /// Up to 500 characters.
    #[serde(default)]
    pub description: Option<String>,
}

/// Body of the add-items call.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddItemsBody {
    /// Track snowflakes, appended in this order (a track may be given twice). At most 500.
    pub track_ids: Vec<String>,
}

/// Body of the move call.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MoveItemBody {
    /// The new zero-based position (clamped to the list).
    pub position: i32,
}

fn clean_name(raw: &str) -> MsgResult<String> {
    let n = raw.trim();
    let len = n.chars().count();
    if len == 0 || len > 100 {
        return Err(invalid(
            "name",
            "BASE_TYPE_BAD_LENGTH",
            "Must be between 1 and 100 in length.",
        ));
    }
    Ok(n.to_owned())
}

fn clean_description(raw: &str) -> MsgResult<String> {
    let d = raw.trim();
    if d.chars().count() > 500 {
        return Err(invalid(
            "description",
            "BASE_TYPE_BAD_LENGTH",
            "Must be 500 or fewer in length.",
        ));
    }
    Ok(d.to_owned())
}

// ----------------------------------------------------------------------------------------- the rules

/// What the member may do with a playlist: `Err(404)` when it is not theirs to see.
pub(crate) async fn rights(deps: &MusicLibDeps, user: i64, p: &PlaylistRow) -> MsgResult<bool> {
    match p.owner {
        PlaylistOwner::User(u) if u == user => Ok(true),
        PlaylistOwner::User(_) => Err(not_found()),
        PlaylistOwner::Guild(g) => match deps
            .reach
            .guild_rights(user, g)
            .await
            .map_err(reach_failed)?
        {
            Some(GuildRights { can_edit }) => Ok(can_edit),
            None => Err(not_found()),
        },
    }
}

async fn load(deps: &MusicLibDeps, user: i64, id: &str) -> MsgResult<(PlaylistRow, bool)> {
    let id = parse_row(id)?;
    let p = deps
        .store
        .playlist(id)
        .await
        .map_err(|e| lib_failed(e, "PLAYLIST_FULL"))?
        .ok_or_else(not_found)?;
    let can_edit = rights(deps, user, &p).await?;
    Ok((p, can_edit))
}

async fn load_editable(deps: &MusicLibDeps, user: i64, id: &str) -> MsgResult<PlaylistRow> {
    let (p, can_edit) = load(deps, user, id).await?;
    if !can_edit {
        return Err(forbidden());
    }
    Ok(p)
}

pub(crate) async fn svc_create(
    deps: &MusicLibDeps,
    cap: i64,
    user: i64,
    owner: PlaylistOwner,
    body: PlaylistBody,
) -> MsgResult<PlaylistView> {
    let name = clean_name(&body.name)?;
    let description = clean_description(body.description.as_deref().unwrap_or(""))?;
    if let PlaylistOwner::Guild(g) = owner {
        match deps
            .reach
            .guild_rights(user, g)
            .await
            .map_err(reach_failed)?
        {
            None => return Err(not_found()),
            Some(r) if !r.can_edit => return Err(forbidden()),
            Some(_) => {}
        }
    }
    let row = deps
        .store
        .create_playlist((deps.next_id)(), owner, &name, &description, user, cap)
        .await
        .map_err(|e| lib_failed(e, "PLAYLIST_LIMIT"))?;
    Ok(PlaylistView::of(row, true))
}

pub(crate) async fn svc_list_own(deps: &MusicLibDeps, user: i64) -> MsgResult<Vec<PlaylistView>> {
    let rows = deps
        .store
        .playlists_of(PlaylistOwner::User(user))
        .await
        .map_err(|e| lib_failed(e, "PLAYLIST_FULL"))?;
    Ok(rows
        .into_iter()
        .map(|p| PlaylistView::of(p, true))
        .collect())
}

pub(crate) async fn svc_list_guild(
    deps: &MusicLibDeps,
    user: i64,
    guild: i64,
) -> MsgResult<Vec<PlaylistView>> {
    let Some(r) = deps
        .reach
        .guild_rights(user, guild)
        .await
        .map_err(reach_failed)?
    else {
        return Err(not_found());
    };
    let rows = deps
        .store
        .playlists_of(PlaylistOwner::Guild(guild))
        .await
        .map_err(|e| lib_failed(e, "PLAYLIST_FULL"))?;
    Ok(rows
        .into_iter()
        .map(|p| PlaylistView::of(p, r.can_edit))
        .collect())
}

pub(crate) async fn svc_detail(
    deps: &MusicLibDeps,
    user: i64,
    id: &str,
) -> MsgResult<PlaylistDetailView> {
    let (p, can_edit) = load(deps, user, id).await?;
    let items = deps
        .store
        .items(p.id)
        .await
        .map_err(|e| lib_failed(e, "PLAYLIST_FULL"))?;
    let readable = deps.readable(user).await?;
    let total = i64::try_from(items.len()).unwrap_or(i64::MAX);
    let shown: Vec<PlaylistItemView> = items
        .into_iter()
        .filter(|i| readable.contains(&i.track.mount_id))
        .map(|i| PlaylistItemView {
            item_id: i.item_id.to_string(),
            position: i.position,
            track: PlaylistTrackView::of(i.track),
        })
        .collect();
    let hidden = total - i64::try_from(shown.len()).unwrap_or(0);
    Ok(PlaylistDetailView {
        playlist: PlaylistView::of(p, can_edit),
        items: shown,
        hidden,
    })
}

pub(crate) async fn svc_edit(
    deps: &MusicLibDeps,
    user: i64,
    id: &str,
    body: PlaylistPatchBody,
) -> MsgResult<PlaylistView> {
    let p = load_editable(deps, user, id).await?;
    let name = body.name.as_deref().map(clean_name).transpose()?;
    let description = body
        .description
        .as_deref()
        .map(clean_description)
        .transpose()?;
    let row = deps
        .store
        .update_playlist(p.id, name.as_deref(), description.as_deref())
        .await
        .map_err(|e| lib_failed(e, "PLAYLIST_FULL"))?;
    Ok(PlaylistView::of(row, true))
}

pub(crate) async fn svc_remove(deps: &MusicLibDeps, user: i64, id: &str) -> MsgResult<()> {
    let p = load_editable(deps, user, id).await?;
    deps.store
        .delete_playlist(p.id)
        .await
        .map_err(|e| lib_failed(e, "PLAYLIST_FULL"))
}

pub(crate) async fn svc_add_items(
    deps: &MusicLibDeps,
    cap: i64,
    user: i64,
    id: &str,
    body: AddItemsBody,
) -> MsgResult<PlaylistView> {
    let p = load_editable(deps, user, id).await?;
    if body.track_ids.is_empty() || body.track_ids.len() > MAX_ADD_BATCH {
        return Err(invalid(
            "track_ids",
            "BASE_TYPE_BAD_LENGTH",
            "Must be between 1 and 500 in length.",
        ));
    }
    let tracks: Vec<i64> = body
        .track_ids
        .iter()
        .map(|t| parse_row(t))
        .collect::<MsgResult<_>>()?;
    check_tracks(deps, user, &tracks).await?;
    let items: Vec<(i64, i64)> = tracks.iter().map(|t| ((deps.next_id)(), *t)).collect();
    deps.store
        .add_items(p.id, &items, user, cap)
        .await
        .map_err(|e| lib_failed(e, "PLAYLIST_FULL"))?;
    let row = deps
        .store
        .playlist(p.id)
        .await
        .map_err(|e| lib_failed(e, "PLAYLIST_FULL"))?
        .ok_or_else(not_found)?;
    Ok(PlaylistView::of(row, true))
}

pub(crate) async fn svc_move_item(
    deps: &MusicLibDeps,
    user: i64,
    id: &str,
    item: &str,
    position: i32,
) -> MsgResult<()> {
    let p = load_editable(deps, user, id).await?;
    let item = parse_row(item)?;
    deps.store
        .move_item(p.id, item, position)
        .await
        .map_err(|e| lib_failed(e, "PLAYLIST_FULL"))
}

pub(crate) async fn svc_remove_item(
    deps: &MusicLibDeps,
    user: i64,
    id: &str,
    item: &str,
) -> MsgResult<()> {
    let p = load_editable(deps, user, id).await?;
    let item = parse_row(item)?;
    deps.store
        .remove_item(p.id, item)
        .await
        .map_err(|e| lib_failed(e, "PLAYLIST_FULL"))
}

// ---------------------------------------------------------------------------------------- the routes

/// The caller's own playlists, newest first.
#[utoipa::path(
    get,
    path = "/users/@me/music/playlists",
    responses((status = 200, body = Vec<PlaylistView>)),
    security(("session" = []))
)]
pub async fn list_own(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<MusicLibDeps>,
) -> MsgResult<Json<Vec<PlaylistView>>> {
    Ok(Json(svc_list_own(&d, p.user_id).await?))
}

/// Make a playlist of the caller's own.
#[utoipa::path(
    post,
    path = "/users/@me/music/playlists",
    request_body = PlaylistBody,
    responses(
        (status = 201, body = PlaylistView),
        (status = 400, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "PLAYLIST_LIMIT: music.max_playlists reached"),
    ),
    security(("session" = []))
)]
pub async fn create_own(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<MusicLibDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    ApiJson(body): ApiJson<PlaylistBody>,
) -> MsgResult<(StatusCode, Json<PlaylistView>)> {
    let cap = d.playlist_cap(admin.as_deref()).await;
    let v = svc_create(&d, cap, p.user_id, PlaylistOwner::User(p.user_id), body).await?;
    Ok((StatusCode::CREATED, Json(v)))
}

/// A server's playlists (for its members).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/music/playlists",
    params(("guild_id" = String, Path, description = "Server snowflake")),
    responses((status = 200, body = Vec<PlaylistView>), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_guild(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<MusicLibDeps>,
    Path(guild): Path<String>,
) -> MsgResult<Json<Vec<PlaylistView>>> {
    Ok(Json(
        svc_list_guild(&d, p.user_id, parse_row(&guild)?).await?,
    ))
}

/// Make a playlist of a server (needs the storage `upload` grant there).
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/music/playlists",
    params(("guild_id" = String, Path, description = "Server snowflake")),
    request_body = PlaylistBody,
    responses(
        (status = 201, body = PlaylistView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "PLAYLIST_LIMIT: music.max_playlists reached"),
    ),
    security(("session" = []))
)]
pub async fn create_guild(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<MusicLibDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    Path(guild): Path<String>,
    ApiJson(body): ApiJson<PlaylistBody>,
) -> MsgResult<(StatusCode, Json<PlaylistView>)> {
    let cap = d.playlist_cap(admin.as_deref()).await;
    let guild = parse_row(&guild)?;
    let v = svc_create(&d, cap, p.user_id, PlaylistOwner::Guild(guild), body).await?;
    Ok((StatusCode::CREATED, Json(v)))
}

/// One playlist with the items the caller can reach.
#[utoipa::path(
    get,
    path = "/music/playlists/{playlist_id}",
    params(("playlist_id" = String, Path, description = "Playlist snowflake")),
    responses((status = 200, body = PlaylistDetailView), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn detail(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<MusicLibDeps>,
    Path(id): Path<String>,
) -> MsgResult<Json<PlaylistDetailView>> {
    Ok(Json(svc_detail(&d, p.user_id, &id).await?))
}

/// Rename or re-describe a playlist.
#[utoipa::path(
    patch,
    path = "/music/playlists/{playlist_id}",
    params(("playlist_id" = String, Path, description = "Playlist snowflake")),
    request_body = PlaylistPatchBody,
    responses(
        (status = 200, body = PlaylistView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn edit(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<MusicLibDeps>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<PlaylistPatchBody>,
) -> MsgResult<Json<PlaylistView>> {
    Ok(Json(svc_edit(&d, p.user_id, &id, body).await?))
}

/// Delete a playlist.
#[utoipa::path(
    delete,
    path = "/music/playlists/{playlist_id}",
    params(("playlist_id" = String, Path, description = "Playlist snowflake")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn remove(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<MusicLibDeps>,
    Path(id): Path<String>,
) -> MsgResult<StatusCode> {
    svc_remove(&d, p.user_id, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Append tracks to a playlist.
#[utoipa::path(
    post,
    path = "/music/playlists/{playlist_id}/items",
    params(("playlist_id" = String, Path, description = "Playlist snowflake")),
    request_body = AddItemsBody,
    responses(
        (status = 200, body = PlaylistView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody, description = "No such playlist, or a track that does not exist or is not readable by the caller"),
        (status = 409, body = ErrorBody, description = "PLAYLIST_FULL: music.max_items reached"),
    ),
    security(("session" = []))
)]
pub async fn add_items(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<MusicLibDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<AddItemsBody>,
) -> MsgResult<Json<PlaylistView>> {
    let cap = d.item_cap(admin.as_deref()).await;
    Ok(Json(svc_add_items(&d, cap, p.user_id, &id, body).await?))
}

/// Move an item within a playlist.
#[utoipa::path(
    patch,
    path = "/music/playlists/{playlist_id}/items/{item_id}",
    params(
        ("playlist_id" = String, Path, description = "Playlist snowflake"),
        ("item_id" = String, Path, description = "Item snowflake"),
    ),
    request_body = MoveItemBody,
    responses(
        (status = 204, description = "Moved"),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn move_item(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<MusicLibDeps>,
    Path((id, item)): Path<(String, String)>,
    ApiJson(body): ApiJson<MoveItemBody>,
) -> MsgResult<StatusCode> {
    svc_move_item(&d, p.user_id, &id, &item, body.position).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove an item from a playlist.
#[utoipa::path(
    delete,
    path = "/music/playlists/{playlist_id}/items/{item_id}",
    params(
        ("playlist_id" = String, Path, description = "Playlist snowflake"),
        ("item_id" = String, Path, description = "Item snowflake"),
    ),
    responses(
        (status = 204, description = "Removed"),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn remove_item(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<MusicLibDeps>,
    Path((id, item)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    svc_remove_item(&d, p.user_id, &id, &item).await?;
    Ok(StatusCode::NO_CONTENT)
}
