//! The emulator's ROM list (ACT-EMU-001, wave 37): `GET /users/@me/games/roms`.
//!
//! The ROMs the indexer found under `emu/roms/` of every mount the caller may read them from: their own mounts, and
//! guild shelves where one of their roles holds `view_games` (or `manage`). A Friend Shelf never carries games.
//! The decision is [`sr_storage::index::rom::reachable_roms`] (one `scopes::access` per mount) plus delisting; the
//! server hands back keys only, the browser presigns a read with `POST /storage/mounts/{id}/presign` and fetches the
//! bytes itself, so no ROM ever passes through the instance.

use super::{ScopeDeps, ShelfDeps, uid};
use crate::auth::SessionOnly;
use crate::openapi::ErrorBody;
use crate::routes::Ext;
use crate::routes::messages::error::{MsgError, MsgResult};
use axum::Json;
use serde::Serialize;
use sr_core::ErrorCode;
use sr_storage::index::rom::reachable_roms;
use sr_storage::scopes::{Scope, ScopeFacts, Viewer};
use std::collections::{HashMap, HashSet};
use utoipa::ToSchema;

/// Most ROMs one call lists.
const MAX_ROMS: i64 = 2000;

/// One ROM.
#[derive(Debug, Serialize, ToSchema)]
pub struct RomView {
    /// Catalog row snowflake.
    pub id: String,
    /// The mount it is on.
    pub mount_id: String,
    /// Mount-relative key (`emu/roms/...`).
    pub key: String,
    /// Size in bytes.
    pub size: i64,
    /// Title from the file name.
    pub title: String,
    /// `nes`, `snes`, `gb`, `gbc`, `gba`, `genesis` or `ps1`; absent for a `.bin` (the browser reads the header).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// The core that plays it (a directory under `/emu/cores/`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub core: Option<String>,
    /// `own` or `guild`.
    pub scope: String,
    /// The mount's label.
    pub mount_label: String,
    /// The server whose shelf it is (`guild`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<String>,
}

/// The ROMs the caller may play.
#[derive(Debug, Serialize, ToSchema)]
pub struct RomListView {
    /// ROMs in title order.
    pub roms: Vec<RomView>,
}

fn store_failed(e: impl std::fmt::Display) -> MsgError {
    tracing::error!(error = %e, "rom list failed");
    ErrorCode::SrInternal.into()
}

/// The ROMs the caller may play, from their own and their servers' shelves.
#[utoipa::path(
    get,
    path = "/users/@me/games/roms",
    responses((status = 200, body = RomListView), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_roms(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(sh): Ext<ShelfDeps>,
) -> MsgResult<Json<RomListView>> {
    let facts: &dyn ScopeFacts = &*d.store;
    let reach = reachable_roms(
        facts,
        &*d.mounts,
        &*sh.store,
        Viewer {
            id: uid(&p),
            staff: false,
        },
    )
    .await
    .map_err(store_failed)?;
    let ids: Vec<i64> = reach.iter().map(|r| r.mount.id).collect();
    let rows = sh.store.roms(&ids, MAX_ROMS).await.map_err(store_failed)?;
    let by_mount: HashMap<i64, &sr_storage::index::rom::RomReach> =
        reach.iter().map(|r| (r.mount.id, r)).collect();
    // Drop what a takedown delisted, one facts call per mount.
    let mut gone: HashSet<(i64, String)> = HashSet::new();
    let mut keys: HashMap<i64, Vec<String>> = HashMap::new();
    for r in &rows {
        keys.entry(r.mount_id).or_default().push(r.key.clone());
    }
    for (mount, ks) in keys {
        for k in facts.delisted(mount, &ks).await.map_err(store_failed)? {
            gone.insert((mount, k));
        }
    }
    let mut roms = Vec::with_capacity(rows.len());
    for r in rows {
        let Some(rc) = by_mount.get(&r.mount_id) else {
            continue;
        };
        if gone.contains(&(r.mount_id, r.key.clone())) {
            continue;
        }
        let (scope, guild_id) = match rc.access.scope {
            Scope::Guild(g) => ("guild", Some(g.to_string())),
            _ => ("own", None),
        };
        roms.push(RomView {
            id: r.id.to_string(),
            mount_id: r.mount_id.to_string(),
            key: r.key,
            size: r.size,
            title: r.title,
            platform: r.platform,
            core: r.core,
            scope: scope.into(),
            mount_label: rc.mount.label.clone(),
            guild_id,
        });
    }
    Ok(Json(RomListView { roms }))
}
