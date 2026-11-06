//! The media showcase of a profile page (PRF-PRO-001, D-502, D-503).
//!
//! A `media_showcase` block names up to six audio or video objects from the member's own shelf (their own
//! mounts). The block is the owner's grant for exactly those objects to the audience `media_tier` admits: a
//! viewer it admits gets a ten-minute presigned read of each one, minted at play time on the owner's mount. The
//! viewer's own storage scopes play no part. The block is never served without a session.
//!
//! Every read re-checks the facts: the mount is active and the owner's, the object is listed in its catalog,
//! nobody delisted it (the catalog's own flag or the takedown register), and it is still audio or video. An item
//! that fails is left off the page and its mint answers the page's 404.

use std::collections::BTreeSet;

use sqlx::Row;

use super::{PackageService, ShowcaseView};
use crate::error::{PackageError, PackageResult};
use crate::profile_doc::{self, BlockKind, ProfileDoc, ShowcaseItem};
use crate::profile_privacy::Standing;

/// The classifier's classes that play as audio or video (`sr_storage::index::classify`).
fn media_of(class: &str) -> Option<&'static str> {
    match class {
        "track" => Some("audio"),
        "movie" | "episode" => Some("video"),
        _ => None,
    }
}

/// The file name of a mount-relative key.
fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// One catalog object that may be shown right now.
struct Live {
    shelf_id: i64,
    object_hash: String,
    path: String,
    title: String,
    duration_ms: Option<i64>,
    media: &'static str,
}

/// An object of a showcase, ready to presign on the owner's mount.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowcaseObject {
    /// The owner's mount.
    pub mount_id: i64,
    /// Mount-relative key.
    pub path: String,
    pub owner_id: i64,
}

impl PackageService {
    /// The objects of `items` that are on an active mount `owner` owns, listed in its catalog, not delisted and
    /// audio or video, keyed by `(shelf, hash)`.
    async fn live_showcase(&self, owner: i64, items: &[ShowcaseItem]) -> PackageResult<Vec<Live>> {
        let shelves: Vec<i64> = items
            .iter()
            .filter_map(|i| i.shelf_id.parse().ok())
            .collect();
        let hashes: Vec<String> = items.iter().map(|i| i.object_hash.clone()).collect();
        if shelves.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            "SELECT o.shelf_id, o.object_hash, o.path, o.class, o.owner_title, \
                    t.title AS tag_title, t.duration_ms::bigint AS duration_ms \
             FROM shelf_objects o \
             JOIN storage_mounts m ON m.id = o.shelf_id \
             LEFT JOIN storage_objects so ON so.mount_id = o.shelf_id AND so.key = o.path \
             LEFT JOIN music_tracks t ON t.mount_object_id = so.id \
             WHERE m.owner_kind = 'user' AND m.owner_id = $1 AND m.status = 'active' AND m.deleted_at IS NULL \
               AND o.shelf_id = ANY($2) AND o.object_hash = ANY($3) \
               AND o.listed AND o.delisted_at IS NULL \
               AND NOT EXISTS (SELECT 1 FROM takedown_items ti WHERE ti.kind = 'storage_object' \
                     AND ti.state = 'disabled' AND ti.target_id = o.shelf_id AND ti.object_key = o.path)",
        )
        .bind(owner)
        .bind(&shelves)
        .bind(&hashes)
        .fetch_all(self.pool())
        .await?;
        let mut out = Vec::new();
        for r in &rows {
            let class: String = r.try_get("class")?;
            let Some(media) = media_of(&class) else {
                continue;
            };
            let path: String = r.try_get("path")?;
            let owner_title: String = r.try_get("owner_title")?;
            let tag_title: Option<String> = r.try_get("tag_title")?;
            let title = if !owner_title.trim().is_empty() {
                owner_title
            } else if let Some(t) = tag_title.filter(|t| !t.trim().is_empty()) {
                t
            } else {
                file_name(&path).to_owned()
            };
            out.push(Live {
                shelf_id: r.try_get("shelf_id")?,
                object_hash: r.try_get("object_hash")?,
                path,
                title,
                duration_ms: r.try_get("duration_ms")?,
                media,
            });
        }
        Ok(out)
    }

    /// Save-time check of a document's showcase: every item is on an active mount the saver owns and in its
    /// catalog as audio or video. The error names the field path of the first item that is not.
    pub(super) async fn check_showcase(&self, user: i64, doc: &ProfileDoc) -> PackageResult<()> {
        let Some(at) = doc
            .blocks
            .iter()
            .position(|b| b.kind == BlockKind::MediaShowcase)
        else {
            return Ok(());
        };
        let items = profile_doc::showcase_items(doc);
        let live = self.live_showcase(user, &items).await?;
        for (j, it) in items.iter().enumerate() {
            let found = live
                .iter()
                .any(|l| l.shelf_id.to_string() == it.shelf_id && l.object_hash == it.object_hash);
            if !found {
                return Err(PackageError::Refused(vec![format!(
                    "blocks[{at}].config.items[{j}]: is not an audio or video file on one of your own shelves"
                )]));
            }
        }
        Ok(())
    }

    /// The showcase as a page shows it: the document's items in order, each resolved now; the ones that are
    /// gone are left out.
    pub(super) async fn showcase_views(
        &self,
        owner: i64,
        doc: &ProfileDoc,
    ) -> PackageResult<Vec<ShowcaseView>> {
        let items = profile_doc::showcase_items(doc);
        let live = self.live_showcase(owner, &items).await?;
        let mut seen = BTreeSet::new();
        Ok(items
            .iter()
            .filter_map(|it| {
                let l = live.iter().find(|l| {
                    l.shelf_id.to_string() == it.shelf_id && l.object_hash == it.object_hash
                })?;
                seen.insert(&l.object_hash).then(|| ShowcaseView {
                    shelf_id: it.shelf_id.clone(),
                    object_hash: l.object_hash.clone(),
                    title: l.title.clone(),
                    duration_ms: l.duration_ms,
                    media: l.media.to_owned(),
                    caption: it.caption.clone(),
                })
            })
            .collect())
    }

    /// What the showcase mint presigns for `viewer`: the object named `object_hash` in the current document of
    /// the account `username`, when the viewer's rank admits the showcase block and the object is live now.
    /// `NotFound("profile")` for every refusal (no such page, blocked, below the tier, no block, a hash the
    /// document does not name, an object that is gone or delisted), so the route is no oracle.
    pub async fn showcase_object(
        &self,
        viewer: i64,
        username: &str,
        object_hash: &str,
        public_possible: bool,
    ) -> PackageResult<ShowcaseObject> {
        let nf = || PackageError::NotFound("profile");
        if !profile_doc::is_object_hash(object_hash) {
            return Err(nf());
        }
        let (user, raw) = self
            .user_by(
                "a.username = $1::citext",
                None,
                Some(username.trim_start_matches('~')),
            )
            .await?
            .ok_or_else(nf)?;
        let owner: i64 = user.id.parse().map_err(|_| nf())?;
        let Standing::Viewer(rank) = self.standing_of(Some(viewer), owner).await? else {
            return Err(nf());
        };
        let (_, stored) = self.privacy_row(owner).await?;
        let policy = stored.effective(public_possible);
        if !policy.served(BlockKind::MediaShowcase, rank) {
            return Err(nf());
        }
        let doc = profile_doc::parse_doc(&raw, false).unwrap_or_default();
        let items: Vec<ShowcaseItem> = profile_doc::showcase_items(&doc)
            .into_iter()
            .filter(|i| i.object_hash == object_hash)
            .collect();
        let live = self.live_showcase(owner, &items).await?;
        let l = live.into_iter().next().ok_or_else(nf)?;
        Ok(ShowcaseObject {
            mount_id: l.shelf_id,
            path: l.path,
            owner_id: owner,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_tracks_and_films_play() {
        assert_eq!(media_of("track"), Some("audio"));
        assert_eq!(media_of("movie"), Some("video"));
        assert_eq!(media_of("episode"), Some("video"));
        for c in ["book", "comic", "artwork", "rom", "other", ""] {
            assert_eq!(media_of(c), None);
        }
        assert_eq!(file_name("Music/Band/01 Song.flac"), "01 Song.flac");
        assert_eq!(file_name("clip.mp4"), "clip.mp4");
    }
}
