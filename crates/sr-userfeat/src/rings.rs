//! PRF-RNG-001: 24-hour status rings.
//!
//! A ring is a photo, a short clip, a line of text or a track, pinned to the author's avatar for at most one day.
//! Its audience is **mutual friends** or **the members of the servers the author chose**, and nobody else. The
//! rule lives in one place, the SQL function `status_ring_visible_to` (migration 1710); every reader in this
//! module asks it, so a ring the viewer may not see answers exactly like a ring that does not exist
//! ([`UfError::NotFound`]). Strangers, blocked people and members of servers that were not chosen cannot tell the
//! two apart.
//!
//! * The author's viewer list ([`UserFeatures::ring_viewers`]) is read by the author only. Anyone else asking gets
//!   the same "unknown ring".
//! * Expiry is hard: [`UserFeatures::expire_rings`] deletes the row and queues its object (and the evidence copy)
//!   in `status_ring_purge`; [`UserFeatures::drain_ring_purge`] deletes the objects through a [`RingObjects`] port,
//!   from the author's own mount or the instance bucket under `rings/`.
//! * A ring a report froze (`hold_id`) keeps row and object until the hold ends; nobody reads it meanwhile. The
//!   evidence of a photo or clip on a member's own mount is copied to the instance bucket first
//!   ([`UserFeatures::copy_held_ring_evidence`]).
//! * [`ring_audience`] and [`viewer_may_see_a_ring_of`] are the functions the profile page calls for its
//!   `ring_tier` cap.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use utoipa::ToSchema;

use crate::error::{UfError, UfResult, invalid};
use crate::{UserFeatures, features::RINGS_ID};

/// The instance bucket is storage mount 1 (`sr_storage::mounts::INSTANCE_MOUNT_ID`).
pub const INSTANCE_MOUNT: i64 = 1;
/// A ring lives this long.
pub const LIFETIME_HOURS: i64 = 24;
/// Caption or text of a ring.
pub const MAX_BODY_CHARS: usize = 280;
/// Title or artist of a track ring.
pub const MAX_TRACK_CHARS: usize = 100;
/// Servers one ring may name.
pub const MAX_GUILDS: usize = 10;
/// Clip length, milliseconds.
pub const MAX_CLIP_MS: i64 = 60_000;
/// Object size, bytes (50 MiB).
pub const MAX_OBJECT_BYTES: i64 = 50 * 1024 * 1024;
/// Where a ring's object lives in the instance bucket.
pub const INSTANCE_PREFIX: &str = "rings/";
/// Where a held object's copy lives in the instance bucket.
pub const EVIDENCE_PREFIX: &str = "rings/evidence/";
/// Purge attempts before an entry is given up (the object is then left to the bucket's own lifecycle rule).
pub const PURGE_ATTEMPTS: i32 = 20;

/// What a ring shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RingKind {
    Photo,
    Clip,
    Text,
    Track,
}

impl RingKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Photo => "photo",
            Self::Clip => "clip",
            Self::Text => "text",
            Self::Track => "track",
        }
    }

    /// Photo and clip carry an object.
    pub const fn has_object(self) -> bool {
        matches!(self, Self::Photo | Self::Clip)
    }
}

/// Who may see a ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RingAudienceKind {
    /// Mutual friends.
    Friends,
    /// Members of the chosen servers.
    Guilds,
}

impl RingAudienceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Friends => "friends",
            Self::Guilds => "guilds",
        }
    }

    fn parse(s: &str) -> Self {
        if s == "guilds" {
            Self::Guilds
        } else {
            Self::Friends
        }
    }
}

/// The text of a track ring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RingTrack {
    pub title: String,
    #[serde(default)]
    pub artist: String,
}

/// The stored object of a photo or clip, as the route wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RingObject {
    /// 1 for the instance bucket, else the author's own mount.
    pub mount_id: i64,
    /// Mount-relative key (`rings/<id>`).
    pub object_key: String,
    pub content_type: String,
    pub bytes: i64,
    /// Clips only (1 to 60 000).
    pub duration_ms: Option<i64>,
}

/// A new ring.
#[derive(Debug, Clone)]
pub struct NewRing {
    pub kind: RingKind,
    pub body: String,
    pub track: Option<RingTrack>,
    pub audience: RingAudienceKind,
    pub guild_ids: Vec<i64>,
    pub object: Option<RingObject>,
}

/// A ring as a viewer sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow, ToSchema)]
pub struct Ring {
    pub id: String,
    pub author_id: String,
    pub kind: String,
    pub body: String,
    #[sqlx(skip)]
    pub track: Option<RingTrack>,
    /// Photo or clip: the file is at `GET /rings/{id}/media`.
    pub has_media: bool,
    pub content_type: Option<String>,
    pub duration_ms: Option<i32>,
    pub audience: String,
    /// The chosen servers. Filled for the author only; everybody else gets an empty list.
    pub guild_ids: Vec<String>,
    pub created_at: String,
    pub expires_at: String,
    /// The viewer has opened it already.
    pub seen: bool,
    /// It is the viewer's own ring.
    pub mine: bool,
}

#[derive(FromRow)]
struct RingRow {
    #[sqlx(flatten)]
    ring: Ring,
    track_json: Option<serde_json::Value>,
}

impl RingRow {
    fn into_ring(mut self) -> Ring {
        self.ring.track = self
            .track_json
            .and_then(|v| serde_json::from_value::<RingTrack>(v).ok());
        if !self.ring.mine {
            self.ring.guild_ids.clear();
        }
        self.ring
    }
}

macro_rules! ring_cols {
    () => {
        concat!(
            "r.id::text AS id, r.author_id::text AS author_id, r.kind, r.body, r.track AS track_json, \
             (r.object_key IS NOT NULL) AS has_media, r.content_type, r.duration_ms, r.audience, \
             (SELECT coalesce(array_agg(g.guild_id::text ORDER BY g.guild_id), ARRAY[]::text[]) \
                FROM status_ring_guilds g WHERE g.ring_id = r.id) AS guild_ids, ",
            iso!("r.created_at"),
            " AS created_at, ",
            iso!("r.expires_at"),
            " AS expires_at, \
             (r.author_id = $1 OR EXISTS (SELECT 1 FROM status_ring_views v WHERE v.ring_id = r.id AND v.viewer_id = $1)) AS seen, \
             (r.author_id = $1) AS mine"
        )
    };
}

/// One person's live rings, for the avatar ring and the tray.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct RingFeedEntry {
    pub author_id: String,
    /// Every ring is `seen`.
    pub all_seen: bool,
    /// Oldest first, the order they are played in.
    pub rings: Vec<Ring>,
}

/// One person who opened the author's ring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow, ToSchema)]
pub struct RingViewer {
    pub user_id: String,
    pub viewed_at: String,
}

/// The audience of a person's ring, for the profile page's `ring_tier` cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RingAudience {
    pub kind: RingAudienceKind,
    pub guild_ids: Vec<i64>,
}

/// What a ring's removal left for the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removed {
    /// The object is queued for deletion (`status_ring_purge`) and the job takes it.
    pub object_queued: bool,
    /// A hold keeps the row; it is hidden now and goes when the hold ends.
    pub kept_under_hold: bool,
}

/// What one expiry pass did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExpireReport {
    pub rows_deleted: u64,
    pub objects_queued: u64,
    pub held_skipped: u64,
}

/// What one purge pass did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PurgeReport {
    pub deleted: u64,
    pub retried: u64,
    pub gave_up: u64,
}

/// Why an object call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectFailure {
    /// The object or its mount is gone for good (a deleted mount, an erased one): nothing to retry.
    Gone,
    /// Try again at the next pass.
    Retry(String),
}

/// The object calls of the job, implemented by `sr-server` over the mounts. Keys are mount-relative.
#[async_trait]
pub trait RingObjects: Send + Sync {
    /// Delete one object. A missing object is success.
    async fn delete(&self, mount_id: i64, key: &str) -> Result<(), ObjectFailure>;
    /// Copy an object of a member's mount into the instance bucket at `to_key`.
    async fn copy_to_instance(
        &self,
        mount_id: i64,
        key: &str,
        to_key: &str,
    ) -> Result<(), ObjectFailure>;
}

/// The stored object of a ring, for the media route.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct RingMedia {
    pub mount_id: i64,
    pub object_key: String,
    pub content_type: String,
    pub bytes: i64,
}

/// The audience of the newest live ring `author` has, or `None` when they have none ("no ring").
pub async fn ring_audience(pool: &PgPool, author: i64) -> UfResult<Option<RingAudience>> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, audience FROM status_rings \
         WHERE author_id = $1 AND deleted_at IS NULL AND expires_at > now() \
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(author)
    .fetch_optional(pool)
    .await?;
    let Some((id, audience)) = row else {
        return Ok(None);
    };
    let guild_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT guild_id FROM status_ring_guilds WHERE ring_id = $1 ORDER BY guild_id",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(Some(RingAudience {
        kind: RingAudienceKind::parse(&audience),
        guild_ids,
    }))
}

/// Whether `viewer` may see at least one live ring of `author` (a profile page shows the ring only then). The
/// same SQL rule as every other reader; an author sees their own.
pub async fn viewer_may_see_a_ring_of(pool: &PgPool, viewer: i64, author: i64) -> UfResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM status_rings r WHERE r.author_id = $2 \
           AND r.expires_at > now() AND status_ring_visible_to(r.id, $1))",
    )
    .bind(viewer)
    .bind(author)
    .fetch_one(pool)
    .await?)
}

/// Whether `viewer` may see this one ring (the SQL rule itself).
pub async fn ring_visible_to(pool: &PgPool, ring: i64, viewer: i64) -> UfResult<bool> {
    Ok(sqlx::query_scalar("SELECT status_ring_visible_to($1, $2)")
        .bind(ring)
        .bind(viewer)
        .fetch_one(pool)
        .await?)
}

fn not_found() -> UfError {
    UfError::NotFound("ring")
}

fn check_text(what: &str, s: &str, max: usize) -> UfResult<String> {
    let s = s.trim();
    if s.chars().count() > max {
        return Err(invalid(format!("{what} must be {max} characters or fewer")));
    }
    if s.chars().any(|c| c.is_control() && c != '\n') {
        return Err(invalid(format!(
            "{what} has characters that are not allowed"
        )));
    }
    Ok(s.to_owned())
}

impl UserFeatures {
    /// Limits in force: `(max_per_day, max_clip_ms, max_bytes)`.
    pub async fn ring_limits(&self) -> UfResult<(i64, i64, i64)> {
        let cfg = self.settings(RINGS_ID, None).await?;
        Ok((
            cfg.int("max_per_day", 20).clamp(1, 200),
            cfg.int("max_clip_seconds", 60).clamp(1, 60) * 1000,
            cfg.int("max_mebibytes", 50).clamp(1, 50) * 1024 * 1024,
        ))
    }

    /// Check the limits of an upload before any byte is stored, so the route can refuse early.
    pub async fn ring_precheck(&self, author: i64) -> UfResult<()> {
        self.require(RINGS_ID, None).await?;
        let (per_day, _, _) = self.ring_limits().await?;
        let today: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM status_ring_posts WHERE author_id = $1 AND posted_at > now() - interval '24 hours'",
        )
        .bind(author)
        .fetch_one(&self.pool)
        .await?;
        if today >= per_day {
            return Err(UfError::Conflict(format!(
                "You can post {per_day} rings a day. Try again later."
            )));
        }
        Ok(())
    }

    /// A fresh ring id, for a route that must name the stored object after the ring before the row exists.
    pub fn new_ring_id(&self) -> i64 {
        self.next_id()
    }

    /// Post a ring as `author`.
    pub async fn create_ring(&self, author: i64, new: NewRing) -> UfResult<Ring> {
        let id = self.next_id();
        self.create_ring_with_id(author, id, new).await
    }

    /// [`Self::create_ring`] with the id chosen by the caller (from [`Self::new_ring_id`]).
    pub async fn create_ring_with_id(&self, author: i64, id: i64, new: NewRing) -> UfResult<Ring> {
        self.ring_precheck(author).await?;
        let (_, max_clip_ms, max_bytes) = self.ring_limits().await?;
        let body = check_text("text", &new.body, MAX_BODY_CHARS)?;
        match new.kind {
            RingKind::Text if body.is_empty() => return Err(invalid("a text ring needs text")),
            RingKind::Track if new.track.is_none() => {
                return Err(invalid("a track ring needs a title"));
            }
            _ => {}
        }
        if new.kind != RingKind::Track && new.track.is_some() {
            return Err(invalid("only a track ring has a track"));
        }
        let track = match new.track {
            Some(t) => {
                let title = check_text("title", &t.title, MAX_TRACK_CHARS)?;
                if title.is_empty() {
                    return Err(invalid("a track ring needs a title"));
                }
                Some(RingTrack {
                    title,
                    artist: check_text("artist", &t.artist, MAX_TRACK_CHARS)?,
                })
            }
            None => None,
        };
        match (&new.object, new.kind.has_object()) {
            (Some(o), true) => {
                if o.bytes <= 0 || o.bytes > max_bytes.min(MAX_OBJECT_BYTES) {
                    return Err(invalid("that file is too big for a ring"));
                }
                match (new.kind, o.duration_ms) {
                    (RingKind::Clip, Some(d))
                        if (1..=max_clip_ms.min(MAX_CLIP_MS)).contains(&d) => {}
                    (RingKind::Clip, _) => {
                        return Err(invalid("a clip is at most 60 seconds long"));
                    }
                    (_, Some(_)) => return Err(invalid("only a clip has a length")),
                    _ => {}
                }
            }
            (None, true) => return Err(invalid("a photo or clip needs a file")),
            (Some(_), false) => return Err(invalid("a text or track ring has no file")),
            (None, false) => {}
        }
        let mut guilds = new.guild_ids.clone();
        guilds.sort_unstable();
        guilds.dedup();
        match new.audience {
            RingAudienceKind::Friends if !guilds.is_empty() => {
                return Err(invalid("a ring for friends names no servers"));
            }
            RingAudienceKind::Guilds if guilds.is_empty() || guilds.len() > MAX_GUILDS => {
                return Err(invalid(format!(
                    "choose between 1 and {MAX_GUILDS} servers for this ring"
                )));
            }
            _ => {}
        }

        let mut tx = self.pool.begin().await?;
        // Serialise one author's posts so the daily limit holds under a double click.
        sqlx::query("SELECT id FROM accounts WHERE id = $1 FOR UPDATE")
            .bind(author)
            .fetch_optional(&mut *tx)
            .await?;
        if !guilds.is_empty() {
            let member: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM guild_members m JOIN guilds g ON g.id = m.guild_id AND g.deleted_at IS NULL \
                 WHERE m.user_id = $1 AND NOT m.pending AND m.guild_id = ANY($2)",
            )
            .bind(author)
            .bind(&guilds)
            .fetch_one(&mut *tx)
            .await?;
            if usize::try_from(member).unwrap_or(0) != guilds.len() {
                // The same answer as for an unknown server: the author is told nothing about servers they are not in.
                return Err(UfError::NotFound("server"));
            }
        }
        let (per_day, _, _) = self.ring_limits().await?;
        let today: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM status_ring_posts WHERE author_id = $1 AND posted_at > now() - interval '24 hours'",
        )
        .bind(author)
        .fetch_one(&mut *tx)
        .await?;
        if today >= per_day {
            return Err(UfError::Conflict(format!(
                "You can post {per_day} rings a day. Try again later."
            )));
        }
        let obj = new.object.as_ref();
        sqlx::query(
            "INSERT INTO status_rings (id, author_id, kind, body, track, mount_id, object_key, content_type, bytes, \
             duration_ms, audience, created_at, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, now(), now() + make_interval(hours => $12))",
        )
        .bind(id)
        .bind(author)
        .bind(new.kind.as_str())
        .bind(&body)
        .bind(track.as_ref().map(|t| serde_json::json!(t)))
        .bind(obj.map(|o| o.mount_id))
        .bind(obj.map(|o| o.object_key.clone()))
        .bind(obj.map(|o| o.content_type.clone()))
        .bind(obj.map(|o| o.bytes))
        .bind(obj.and_then(|o| o.duration_ms).map(|d| d as i32))
        .bind(new.audience.as_str())
        .bind(i32::try_from(LIFETIME_HOURS).unwrap_or(24))
        .execute(&mut *tx)
        .await?;
        for g in &guilds {
            sqlx::query("INSERT INTO status_ring_guilds (ring_id, guild_id) VALUES ($1, $2)")
                .bind(id)
                .bind(g)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("INSERT INTO status_ring_posts (author_id) VALUES ($1)")
            .bind(author)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.ring_for(author, id).await
    }

    /// One ring as `viewer` sees it; "unknown ring" when they may not (or it is not there).
    pub async fn ring_for(&self, viewer: i64, id: i64) -> UfResult<Ring> {
        self.require(RINGS_ID, None).await?;
        let row = sqlx::query_as::<_, RingRow>(concat!(
            "SELECT ",
            ring_cols!(),
            " FROM status_rings r WHERE r.id = $2 AND status_ring_visible_to(r.id, $1)"
        ))
        .bind(viewer)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(not_found)?;
        Ok(row.into_ring())
    }

    /// Every live ring `viewer` may see, grouped by author: unseen authors first, then the newest. Includes the
    /// viewer's own.
    pub async fn ring_feed(&self, viewer: i64) -> UfResult<Vec<RingFeedEntry>> {
        self.require(RINGS_ID, None).await?;
        let rows = sqlx::query_as::<_, RingRow>(concat!(
            "SELECT ",
            ring_cols!(),
            " FROM status_rings r WHERE r.expires_at > now() AND r.deleted_at IS NULL \
              AND status_ring_visible_to(r.id, $1) ORDER BY r.created_at, r.id LIMIT 400"
        ))
        .bind(viewer)
        .fetch_all(&self.pool)
        .await?;
        let mut out: Vec<RingFeedEntry> = Vec::new();
        for ring in rows.into_iter().map(RingRow::into_ring) {
            match out.iter_mut().find(|e| e.author_id == ring.author_id) {
                Some(e) => {
                    e.all_seen &= ring.seen;
                    e.rings.push(ring);
                }
                None => out.push(RingFeedEntry {
                    author_id: ring.author_id.clone(),
                    all_seen: ring.seen,
                    rings: vec![ring],
                }),
            }
        }
        let me = viewer.to_string();
        out.sort_by(|a, b| {
            let key = |e: &RingFeedEntry| {
                (
                    e.author_id != me,
                    e.all_seen,
                    std::cmp::Reverse(e.rings.last().map(|r| r.created_at.clone())),
                )
            };
            key(a).cmp(&key(b))
        });
        Ok(out)
    }

    /// The author's own live rings.
    pub async fn my_rings(&self, author: i64) -> UfResult<Vec<Ring>> {
        self.require(RINGS_ID, None).await?;
        let rows = sqlx::query_as::<_, RingRow>(concat!(
            "SELECT ",
            ring_cols!(),
            " FROM status_rings r WHERE r.author_id = $1 AND r.expires_at > now() AND r.deleted_at IS NULL \
              ORDER BY r.created_at, r.id"
        ))
        .bind(author)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(RingRow::into_ring).collect())
    }

    /// Where the file of a ring is, when `viewer` may see the ring.
    pub async fn ring_media(&self, viewer: i64, id: i64) -> UfResult<RingMedia> {
        self.require(RINGS_ID, None).await?;
        sqlx::query_as::<_, RingMedia>(
            "SELECT r.mount_id, r.object_key, r.content_type, r.bytes FROM status_rings r \
             WHERE r.id = $2 AND r.object_key IS NOT NULL AND status_ring_visible_to(r.id, $1)",
        )
        .bind(viewer)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(not_found)
    }

    /// Note that `viewer` opened the ring. The author's own opens are not recorded.
    pub async fn record_ring_view(&self, viewer: i64, id: i64) -> UfResult<()> {
        self.require(RINGS_ID, None).await?;
        let author: Option<i64> = sqlx::query_scalar(
            "SELECT r.author_id FROM status_rings r WHERE r.id = $2 AND status_ring_visible_to(r.id, $1)",
        )
        .bind(viewer)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let author = author.ok_or_else(not_found)?;
        if author != viewer {
            sqlx::query(
                "INSERT INTO status_ring_views (ring_id, viewer_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(id)
            .bind(viewer)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// The people who opened the author's ring, newest first. Only the author may ask: anybody else (and anybody
    /// when the ring is not there) gets "unknown ring".
    pub async fn ring_viewers(&self, author: i64, id: i64) -> UfResult<Vec<RingViewer>> {
        self.require(RINGS_ID, None).await?;
        let mine: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM status_rings WHERE id = $1 AND author_id = $2 \
               AND deleted_at IS NULL AND expires_at > now())",
        )
        .bind(id)
        .bind(author)
        .fetch_one(&self.pool)
        .await?;
        if !mine {
            return Err(not_found());
        }
        Ok(sqlx::query_as::<_, RingViewer>(concat!(
            "SELECT v.viewer_id::text AS user_id, ",
            iso!("v.viewed_at"),
            " AS viewed_at FROM status_ring_views v WHERE v.ring_id = $1 ORDER BY v.viewed_at DESC, v.viewer_id LIMIT 500"
        ))
        .bind(id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Delete the author's own ring now. A ring under a hold is hidden and stays until the hold ends.
    pub async fn delete_ring(&self, author: i64, id: i64) -> UfResult<Removed> {
        let mut tx = self.pool.begin().await?;
        let row: Option<(Option<i64>, Option<String>, Option<i64>, Option<String>)> =
            sqlx::query_as(
                "SELECT mount_id, object_key, hold_id, evidence_key FROM status_rings \
             WHERE id = $1 AND author_id = $2 AND deleted_at IS NULL FOR UPDATE",
            )
            .bind(id)
            .bind(author)
            .fetch_optional(&mut *tx)
            .await?;
        let (mount, key, hold, evidence) = row.ok_or_else(not_found)?;
        if hold.is_some() {
            sqlx::query("UPDATE status_rings SET deleted_at = now() WHERE id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(Removed {
                object_queued: false,
                kept_under_hold: true,
            });
        }
        let queued = self.queue_purge(&mut tx, mount, key, evidence).await?;
        sqlx::query("DELETE FROM status_rings WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Removed {
            object_queued: queued,
            kept_under_hold: false,
        })
    }

    async fn queue_purge(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        mount: Option<i64>,
        key: Option<String>,
        evidence: Option<String>,
    ) -> UfResult<bool> {
        let mut any = false;
        if let (Some(mount), Some(key)) = (mount, key) {
            sqlx::query("INSERT INTO status_ring_purge (mount_id, object_key) VALUES ($1, $2)")
                .bind(mount)
                .bind(key)
                .execute(&mut **tx)
                .await?;
            any = true;
        }
        if let Some(ev) = evidence {
            sqlx::query("INSERT INTO status_ring_purge (mount_id, object_key) VALUES ($1, $2)")
                .bind(INSTANCE_MOUNT)
                .bind(ev)
                .execute(&mut **tx)
                .await?;
            any = true;
        }
        Ok(any)
    }

    /// The job's expiry pass: delete every ring past `expires_at` (and every ring its author deleted whose hold
    /// has ended) that no hold freezes, queue its objects, prune the allowance table. At most `limit` rows.
    pub async fn expire_rings(&self, limit: i64) -> UfResult<ExpireReport> {
        let mut tx = self.pool.begin().await?;
        let due: Vec<(i64, Option<i64>, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT id, mount_id, object_key, evidence_key FROM status_rings \
             WHERE hold_id IS NULL AND (expires_at <= now() OR deleted_at IS NOT NULL) \
             ORDER BY expires_at LIMIT $1 FOR UPDATE SKIP LOCKED",
        )
        .bind(limit)
        .fetch_all(&mut *tx)
        .await?;
        let mut report = ExpireReport::default();
        for (id, mount, key, evidence) in due {
            if self.queue_purge(&mut tx, mount, key, evidence).await? {
                report.objects_queued += 1;
            }
            sqlx::query("DELETE FROM status_rings WHERE id = $1 AND hold_id IS NULL")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            report.rows_deleted += 1;
        }
        report.held_skipped = u64::try_from(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM status_rings WHERE hold_id IS NOT NULL AND (expires_at <= now() OR deleted_at IS NOT NULL)",
            )
            .fetch_one(&mut *tx)
            .await?,
        )
        .unwrap_or(0);
        sqlx::query("DELETE FROM status_ring_posts WHERE posted_at < now() - interval '2 days'")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(report)
    }

    /// The job's purge pass: delete queued objects through `objects`. A failed delete is retried at the next
    /// pass, up to [`PURGE_ATTEMPTS`] times.
    pub async fn drain_ring_purge(
        &self,
        objects: &dyn RingObjects,
        limit: i64,
    ) -> UfResult<PurgeReport> {
        let rows: Vec<(i64, i64, String, i32)> = sqlx::query_as(
            "SELECT id, mount_id, object_key, attempts FROM status_ring_purge ORDER BY created_at, id LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut report = PurgeReport::default();
        for (id, mount, key, attempts) in rows {
            match objects.delete(mount, &key).await {
                Ok(()) | Err(ObjectFailure::Gone) => {
                    sqlx::query("DELETE FROM status_ring_purge WHERE id = $1")
                        .bind(id)
                        .execute(&self.pool)
                        .await?;
                    report.deleted += 1;
                }
                Err(ObjectFailure::Retry(why)) => {
                    if attempts + 1 >= PURGE_ATTEMPTS {
                        tracing::error!(mount, key, %why, "giving up deleting a ring object");
                        sqlx::query("DELETE FROM status_ring_purge WHERE id = $1")
                            .bind(id)
                            .execute(&self.pool)
                            .await?;
                        report.gave_up += 1;
                    } else {
                        tracing::warn!(mount, key, %why, "ring object not deleted yet");
                        sqlx::query(
                            "UPDATE status_ring_purge SET attempts = attempts + 1 WHERE id = $1",
                        )
                        .bind(id)
                        .execute(&self.pool)
                        .await?;
                        report.retried += 1;
                    }
                }
            }
        }
        Ok(report)
    }

    /// Copy the object of every held photo or clip that lives on a member's own mount to the instance bucket, so
    /// the evidence outlives the member's bucket and connection. Returns how many were copied.
    pub async fn copy_held_ring_evidence(
        &self,
        objects: &dyn RingObjects,
        limit: i64,
    ) -> UfResult<u64> {
        let rows: Vec<(i64, i64, String)> = sqlx::query_as(
            "SELECT id, mount_id, object_key FROM status_rings \
             WHERE hold_id IS NOT NULL AND evidence_key IS NULL AND object_key IS NOT NULL AND mount_id <> $2 \
             ORDER BY id LIMIT $1",
        )
        .bind(limit)
        .bind(INSTANCE_MOUNT)
        .fetch_all(&self.pool)
        .await?;
        let mut copied = 0;
        for (id, mount, key) in rows {
            let to = format!("{EVIDENCE_PREFIX}{id}");
            match objects.copy_to_instance(mount, &key, &to).await {
                Ok(()) => {
                    sqlx::query(
                        "UPDATE status_rings SET evidence_key = $2 WHERE id = $1 AND evidence_key IS NULL",
                    )
                    .bind(id)
                    .bind(&to)
                    .execute(&self.pool)
                    .await?;
                    copied += 1;
                }
                Err(e) => tracing::warn!(
                    ring = id,
                    ?e,
                    "could not copy a held ring to the instance bucket"
                ),
            }
        }
        Ok(copied)
    }

    /// Where staff read a held ring's file: the evidence copy when there is one, else the original. `None` when
    /// the ring is not under a hold. The caller (the logged read) checks the staff permission and writes the log.
    pub async fn held_ring_media(&self, id: i64) -> UfResult<Option<RingMedia>> {
        Ok(sqlx::query_as::<_, RingMedia>(
            "SELECT CASE WHEN evidence_key IS NOT NULL THEN 1 ELSE mount_id END AS mount_id, \
                    COALESCE(evidence_key, object_key) AS object_key, content_type, bytes \
             FROM status_rings WHERE id = $1 AND hold_id IS NOT NULL AND object_key IS NOT NULL",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?)
    }
}
