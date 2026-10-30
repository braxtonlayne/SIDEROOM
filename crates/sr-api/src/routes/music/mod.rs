//! The member's music library (MUS-LIB-001, wave 30): playlists, favorites and the recently played list.
//!
//! * `playlists`: `/users/@me/music/playlists`, `/guilds/{id}/music/playlists`, `/music/playlists/{id}*`.
//!   A playlist belongs to a member or to a server. A server's playlist is readable by its members and
//!   editable with the storage `upload` (or `manage`) grant; a member's is the owner's alone (404 for others).
//! * `favorites`: `/users/@me/music/favorites*`.
//! * `recent`: `/users/@me/music/recent*` (200 rows per member, pruned by the store).
//!
//! Tracks are `music_tracks` rows (the index owns them). A track can be put on a list only by someone who
//! may read its mount now, and every read hides the tracks the viewer cannot reach any more (they are counted
//! in `hidden`, never named), so a server playlist never discloses a friend's bucket to the rest of the server.
//! Search is `GET /music/library?q=` of the index routes, not here.
//!
//! The caps `music.max_playlists` (100) and `music.max_items` (5000) are catalog settings.

pub mod favorites;
pub mod playlists;
pub mod recent;

use crate::{
    openapi::ErrorBody,
    routes::{
        admin::AdminDeps,
        messages::error::{MsgError, MsgResult},
    },
    state::AppState,
};
use async_trait::async_trait;
use axum::{
    Router,
    routing::{get, patch, post, put},
};
use serde::Serialize;
use sr_core::{ApiError, ErrorCode};
use sr_storage::music_lib::{LibError, MusicLibStore, StampedTrack, TrackRow};
use sr_storage::scopes::{Grant, ScopeFacts, Viewer};
use std::{collections::HashSet, sync::Arc};
use utoipa::{OpenApi, ToSchema};

/// Default of `music.max_playlists`.
pub const DEFAULT_MAX_PLAYLISTS: i64 = 100;
/// Default of `music.max_items`.
pub const DEFAULT_MAX_ITEMS: i64 = 5000;
/// Most tracks one add call may carry.
pub const MAX_ADD_BATCH: usize = 500;

/// What a member may do in a server, as far as playlists go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuildRights {
    /// Holds the storage `upload` (or `manage`) grant: may create and edit the server's playlists.
    pub can_edit: bool,
}

/// A failure of the reach lookup: nothing is admitted.
#[derive(Debug, Clone)]
pub struct ReachError(pub String);

/// What the storage scopes say about a member, behind a trait so the rules can be tested without a database.
#[async_trait]
pub trait MusicReach: Send + Sync {
    /// The mounts with music the member may read right now (own, friend share, server grant).
    async fn readable_mounts(&self, user: i64) -> Result<HashSet<i64>, ReachError>;
    /// The member's standing in a server: `None` for a non-member.
    async fn guild_rights(&self, user: i64, guild: i64) -> Result<Option<GuildRights>, ReachError>;
}

/// [`MusicReach`] over the real storage scopes (the same decision as the shelves).
pub struct ShelfReach {
    /// The facts, mount lookup and sealer of the storage routes.
    pub scopes: Arc<super::storage::ScopeDeps>,
    /// The shelf rows (which mounts have music).
    pub shelves: Arc<super::storage::ShelfDeps>,
}

#[async_trait]
impl MusicReach for ShelfReach {
    async fn readable_mounts(&self, user: i64) -> Result<HashSet<i64>, ReachError> {
        let facts: &dyn ScopeFacts = &*self.scopes.store;
        let reach = sr_storage::index::shelf::reachable(
            facts,
            &*self.scopes.mounts,
            &*self.shelves.store,
            Viewer {
                id: user,
                staff: false,
            },
            sr_storage::index::shelf::Section::Music,
        )
        .await
        .map_err(|e| ReachError(e.to_string()))?;
        Ok(reach.into_iter().map(|r| r.mount.id).collect())
    }

    async fn guild_rights(&self, user: i64, guild: i64) -> Result<Option<GuildRights>, ReachError> {
        let facts: &dyn ScopeFacts = &*self.scopes.store;
        let e = |e: sr_storage::scopes::FactsError| ReachError(e.to_string());
        let Some(standing) = facts.guild_standing(guild, user).await.map_err(e)? else {
            return Ok(None);
        };
        let roles = facts
            .role_grants(guild, &standing.role_ids)
            .await
            .map_err(e)?;
        let grants = sr_storage::scopes::grants_for(&standing, &roles);
        Ok(Some(GuildRights {
            can_edit: grants.has(Grant::Upload) || grants.has(Grant::Manage),
        }))
    }
}

/// Collaborators of the music library routes.
pub struct MusicLibDeps {
    /// The rows.
    pub store: Arc<dyn MusicLibStore>,
    /// The storage scopes.
    pub reach: Arc<dyn MusicReach>,
    /// Snowflake source.
    pub next_id: Arc<dyn Fn() -> i64 + Send + Sync>,
    /// Caps used when the settings service is not wired (tests).
    pub max_playlists: i64,
    /// See `max_playlists`.
    pub max_items: i64,
}

impl MusicLibDeps {
    /// Deps with the default caps.
    pub fn new(
        store: Arc<dyn MusicLibStore>,
        reach: Arc<dyn MusicReach>,
        next_id: Arc<dyn Fn() -> i64 + Send + Sync>,
    ) -> Self {
        Self {
            store,
            reach,
            next_id,
            max_playlists: DEFAULT_MAX_PLAYLISTS,
            max_items: DEFAULT_MAX_ITEMS,
        }
    }

    /// `music.max_playlists`: the setting when the admin service is wired, else the deps' default.
    pub async fn playlist_cap(&self, admin: Option<&AdminDeps>) -> i64 {
        setting(
            admin,
            sr_admin::settings::MUSIC_MAX_PLAYLISTS,
            self.max_playlists,
        )
        .await
    }

    /// `music.max_items`: the setting when the admin service is wired, else the deps' default.
    pub async fn item_cap(&self, admin: Option<&AdminDeps>) -> i64 {
        setting(admin, sr_admin::settings::MUSIC_MAX_ITEMS, self.max_items).await
    }

    /// The mounts the member may read (a failed lookup admits nothing and is a 500).
    pub(crate) async fn readable(&self, user: i64) -> MsgResult<HashSet<i64>> {
        self.reach.readable_mounts(user).await.map_err(reach_failed)
    }
}

async fn setting(admin: Option<&AdminDeps>, key: &str, default: i64) -> i64 {
    match admin {
        Some(a) => match a.service.setting_value_i64(key).await {
            Ok(v) if v > 0 => v,
            _ => default,
        },
        None => default,
    }
}

pub(crate) fn reach_failed(e: ReachError) -> MsgError {
    tracing::error!(error = %e.0, "music reach lookup failed");
    ErrorCode::SrInternal.into()
}

pub(crate) fn not_found() -> MsgError {
    MsgError::Api(ApiError::with_message(
        ErrorCode::SrNotFound,
        "Unknown playlist or track",
    ))
}

pub(crate) fn forbidden() -> MsgError {
    ErrorCode::MissingPermissions.into()
}

/// A 409 whose stable string code is the field error under `errors.music`.
fn conflict(code: &str, message: &str) -> MsgError {
    MsgError::Api(
        ApiError::with_message(ErrorCode::SrConflict, message).field("music", code, message),
    )
}

/// A store failure as the wire has it. `cap_code` names the cap that a [`LibError::Cap`] means here.
pub(crate) fn lib_failed(e: LibError, cap_code: &'static str) -> MsgError {
    match e {
        LibError::NotFound => not_found(),
        LibError::Cap => match cap_code {
            "PLAYLIST_LIMIT" => conflict(cap_code, "That is the most playlists you can keep."),
            "PLAYLIST_FULL" => conflict(cap_code, "That playlist is full."),
            _ => conflict(cap_code, "That list is full."),
        },
        LibError::Store(m) => {
            tracing::error!(error = %m, "music store failed");
            ErrorCode::SrInternal.into()
        }
    }
}

/// A snowflake in a path or body; malformed means "no such row".
pub(crate) fn parse_row(raw: &str) -> MsgResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(not_found)
}

/// A track as the lists show it.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PlaylistTrackView {
    /// Track snowflake.
    pub id: String,
    /// The mount the audio lives in.
    pub mount_id: String,
    /// Title.
    pub title: String,
    /// Artist (may be empty).
    pub artist: String,
    /// Album (may be empty).
    pub album: String,
    /// Album artist (may be empty).
    pub album_artist: String,
    /// Track number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_no: Option<i32>,
    /// Disc number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_no: Option<i32>,
    /// Year.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i16>,
    /// Duration in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i32>,
    /// Genres.
    pub genres: Vec<String>,
    /// Tags.
    pub tags: Vec<String>,
    /// Cover key in the mount.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_key: Option<String>,
}

impl PlaylistTrackView {
    pub(crate) fn of(t: TrackRow) -> Self {
        Self {
            id: t.id.to_string(),
            mount_id: t.mount_id.to_string(),
            title: t.title,
            artist: t.artist,
            album: t.album,
            album_artist: t.album_artist,
            track_no: t.track_no,
            disc_no: t.disc_no,
            year: t.year,
            duration_ms: t.duration_ms,
            genres: t.genres,
            tags: t.tags,
            cover_key: t.cover_key,
        }
    }
}

/// A track with the time it was favorited or last played.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StampedTrackView {
    /// The track.
    pub track: PlaylistTrackView,
    /// When, unix milliseconds.
    pub at_ms: i64,
}

/// The tracks of a stamped list the viewer can reach, and how many were left out.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StampedListView {
    /// Newest first.
    pub items: Vec<StampedTrackView>,
    /// Tracks left out because their bucket is not readable by the viewer any more.
    pub hidden: i64,
}

/// Keep what `readable` admits.
pub(crate) fn reachable_only(rows: Vec<StampedTrack>, readable: &HashSet<i64>) -> StampedListView {
    let mut hidden = 0_i64;
    let mut items = Vec::with_capacity(rows.len());
    for r in rows {
        if readable.contains(&r.track.mount_id) {
            items.push(StampedTrackView {
                track: PlaylistTrackView::of(r.track),
                at_ms: r.at_ms,
            });
        } else {
            hidden += 1;
        }
    }
    StampedListView { items, hidden }
}

/// Tracks the member may put on a list: every id must exist (listed) and be readable. Anything else is a
/// 404, the same answer for "no such track" and "not yours to read".
pub(crate) async fn check_tracks(
    deps: &MusicLibDeps,
    user: i64,
    ids: &[i64],
) -> MsgResult<Vec<TrackRow>> {
    let rows = deps
        .store
        .tracks(ids)
        .await
        .map_err(|e| lib_failed(e, "LIST_FULL"))?;
    let readable = deps.readable(user).await?;
    for id in ids {
        match rows.iter().find(|t| t.id == *id) {
            Some(t) if readable.contains(&t.mount_id) => {}
            _ => return Err(not_found()),
        }
    }
    Ok(rows)
}

/// All music library routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/users/@me/music/playlists",
            get(playlists::list_own).post(playlists::create_own),
        )
        .route(
            "/guilds/{guild_id}/music/playlists",
            get(playlists::list_guild).post(playlists::create_guild),
        )
        .route(
            "/music/playlists/{playlist_id}",
            get(playlists::detail)
                .patch(playlists::edit)
                .delete(playlists::remove),
        )
        .route(
            "/music/playlists/{playlist_id}/items",
            post(playlists::add_items),
        )
        .route(
            "/music/playlists/{playlist_id}/items/{item_id}",
            patch(playlists::move_item).delete(playlists::remove_item),
        )
        .route("/users/@me/music/favorites", get(favorites::list))
        .route(
            "/users/@me/music/favorites/{track_id}",
            put(favorites::add).delete(favorites::remove),
        )
        .route(
            "/users/@me/music/recent",
            get(recent::list).post(recent::record).delete(recent::clear),
        )
}

#[derive(OpenApi)]
#[openapi(
    paths(
        playlists::list_own,
        playlists::create_own,
        playlists::list_guild,
        playlists::create_guild,
        playlists::detail,
        playlists::edit,
        playlists::remove,
        playlists::add_items,
        playlists::move_item,
        playlists::remove_item,
        favorites::list,
        favorites::add,
        favorites::remove,
        recent::list,
        recent::record,
        recent::clear,
    ),
    components(schemas(
        ErrorBody,
        PlaylistTrackView,
        StampedTrackView,
        StampedListView,
        playlists::PlaylistView,
        playlists::PlaylistItemView,
        playlists::PlaylistDetailView,
        playlists::PlaylistBody,
        playlists::PlaylistPatchBody,
        playlists::AddItemsBody,
        playlists::MoveItemBody,
        recent::RecordBody,
    ))
)]
pub struct MusicLibApiDoc;

/// The OpenAPI fragment of the music library routes.
pub fn openapi() -> utoipa::openapi::OpenApi {
    MusicLibApiDoc::openapi()
}

#[cfg(test)]
mod tests;
