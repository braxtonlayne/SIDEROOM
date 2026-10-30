//! `GET /music/library` and `POST /music/library/rescan` (MUS-IDX-001).

use super::tracks::{TrackView, view_of};
use super::{MusicDeps, may_write, not_found, reaches};
use crate::auth::SessionOnly;
use crate::openapi::ErrorBody;
use crate::routes::Ext;
use crate::routes::messages::error::{ApiJson, ApiQuery, MsgResult};
use crate::routes::storage::shelves::{ShelfSourceView, Signer, source_of, store_failed};
use crate::routes::storage::{ScopeDeps, facts_failed, parse_row, uid};
use axum::Json;
use serde::{Deserialize, Serialize};
use sr_storage::index::audio::{LibraryFilter, MAX_PAGE};
use sr_storage::scopes::ScopeFacts;
use std::collections::HashMap;
use utoipa::ToSchema;

/// Query of the listing.
#[derive(Debug, Default, Deserialize)]
pub struct LibraryQuery {
    /// Only this mount (snowflake).
    pub mount: Option<String>,
    /// Exact album.
    pub album: Option<String>,
    /// Exact artist (or album artist).
    pub artist: Option<String>,
    /// Substring of the title, artist or album.
    pub q: Option<String>,
    /// Only tracks whose tags are still to be read (the lazy tagger's work list).
    pub pending: Option<bool>,
    /// Page size (at most 200).
    pub limit: Option<u32>,
    /// Offset.
    pub offset: Option<u32>,
}

/// A page of tracks and the sources behind them.
#[derive(Debug, Serialize, ToSchema)]
pub struct TrackListView {
    /// Tracks, by album, disc and track number.
    pub items: Vec<TrackView>,
    /// All that match.
    pub total: i64,
    /// The mounts the caller reaches for music.
    pub sources: Vec<ShelfSourceView>,
}

/// Body of the rescan.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RescanBody {
    /// The mount whose tracks are read again.
    pub mount_id: String,
}

/// How many tracks were queued.
#[derive(Debug, Serialize, ToSchema)]
pub struct RescanView {
    /// Tracks whose tags will be read again.
    pub queued: u64,
}

fn text(v: Option<String>) -> Option<String> {
    v.map(|s| s.chars().take(300).collect::<String>())
        .filter(|s| !s.trim().is_empty())
}

/// The tracks of every mount the caller reaches (own, friends' shared music, servers' music), filtered.
#[utoipa::path(
    get,
    path = "/music/library",
    params(
        ("mount" = Option<String>, Query, description = "Only this mount"),
        ("album" = Option<String>, Query, description = "Exact album"),
        ("artist" = Option<String>, Query, description = "Exact artist or album artist"),
        ("q" = Option<String>, Query, description = "Substring of title, artist or album"),
        ("pending" = Option<bool>, Query, description = "Only tracks whose tags are still to be read"),
        ("limit" = Option<u32>, Query, description = "At most 200"),
        ("offset" = Option<u32>, Query, description = "Offset"),
    ),
    responses(
        (status = 200, body = TrackListView),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn list(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    ApiQuery(q): ApiQuery<LibraryQuery>,
) -> MsgResult<Json<TrackListView>> {
    let user = uid(&p);
    let reach = reaches(&d, &m, user).await?;
    let sources: Vec<ShelfSourceView> = reach.iter().map(source_of).collect();
    let mount = match q.mount.as_deref() {
        Some(raw) => Some(parse_row(raw)?),
        None => None,
    };
    // A mount the caller does not reach is simply absent: asking for it lists nothing.
    let ids: Vec<i64> = reach
        .iter()
        .map(|r| r.mount.id)
        .filter(|id| mount.is_none_or(|m| m == *id))
        .collect();
    let filter = LibraryFilter {
        mount,
        album: text(q.album),
        artist: text(q.artist),
        q: text(q.q),
        pending: q.pending.unwrap_or(false),
        limit: q.limit.unwrap_or(50).clamp(1, MAX_PAGE),
        offset: q.offset.unwrap_or(0).min(1_000_000),
    };
    let page = m.store.list(&ids, &filter).await.map_err(store_failed)?;
    // Per-key rules (a takedown delists a key) and per-mount write rights.
    let facts: &dyn ScopeFacts = &*d.store;
    let mut by_mount: HashMap<i64, Vec<String>> = HashMap::new();
    for t in &page.items {
        by_mount.entry(t.mount_id).or_default().push(t.key.clone());
    }
    let mut gone: HashMap<i64, Vec<String>> = HashMap::new();
    for (mid, keys) in &by_mount {
        let g = facts.delisted(*mid, keys).await.map_err(facts_failed)?;
        gone.insert(*mid, g);
    }
    let mut can: HashMap<i64, bool> = HashMap::new();
    for r in &reach {
        can.insert(r.mount.id, may_write(&d, user, &r.mount).await?);
    }
    let mut signer = Signer::new(&d, &reach);
    let mut items = Vec::with_capacity(page.items.len());
    for t in page.items {
        if gone.get(&t.mount_id).is_some_and(|g| g.contains(&t.key)) {
            continue;
        }
        let edit = can.get(&t.mount_id).copied().unwrap_or(false);
        items.push(view_of(t, edit, &mut signer).await);
    }
    Ok(Json(TrackListView {
        items,
        total: page.total,
        sources,
    }))
}

/// Queue every track of a mount for a tag read again ("Rescan tags"). The browser does the reading; this only
/// marks the rows. Needs the mount's owner or the `upload` grant.
#[utoipa::path(
    post,
    path = "/music/library/rescan",
    request_body = RescanBody,
    responses(
        (status = 200, body = RescanView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn rescan(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(m): Ext<MusicDeps>,
    ApiJson(body): ApiJson<RescanBody>,
) -> MsgResult<Json<RescanView>> {
    let user = uid(&p);
    let id = parse_row(&body.mount_id)?;
    let reach = reaches(&d, &m, user).await?;
    let mount = reach
        .iter()
        .find(|r| r.mount.id == id)
        .map(|r| r.mount.clone())
        .ok_or_else(not_found)?;
    if !may_write(&d, user, &mount).await? {
        return Err(sr_core::ErrorCode::MissingPermissions.into());
    }
    let queued = m.store.rescan(id).await.map_err(store_failed)?;
    Ok(Json(RescanView { queued }))
}
