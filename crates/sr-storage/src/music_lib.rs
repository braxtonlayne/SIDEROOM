//! The rows of the member's music library (MUS-LIB-001): playlists, favorites and the recently played list.
//! The store knows rows and caps; who may see or edit what is decided by the routes with the storage scopes.
//! `sr-db` (`music_lib_repo`) implements it over Postgres.

use async_trait::async_trait;

/// Most rows of the recently played list a member keeps.
pub const RECENT_KEEP: i64 = 200;

/// A failure of the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibError {
    /// No such playlist (or item, or track).
    NotFound,
    /// A cap was hit: the playlists of an owner, the items of a playlist, or the favorites of a member.
    Cap,
    /// The database failed.
    Store(String),
}

impl std::fmt::Display for LibError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("not found"),
            Self::Cap => f.write_str("limit reached"),
            Self::Store(m) => write!(f, "music store: {m}"),
        }
    }
}

impl std::error::Error for LibError {}

/// Who owns a playlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaylistOwner {
    /// A member's own playlist.
    User(i64),
    /// A server's playlist.
    Guild(i64),
}

/// A playlist row with its item count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistRow {
    /// Snowflake.
    pub id: i64,
    /// The owner.
    pub owner: PlaylistOwner,
    /// 1 to 100 characters.
    pub name: String,
    /// Up to 500 characters.
    pub description: String,
    /// Who made it (gone after the account is deleted).
    pub created_by: Option<i64>,
    /// How many items it holds (reachable or not).
    pub item_count: i64,
    /// Creation time, unix milliseconds.
    pub created_ms: i64,
    /// Last change, unix milliseconds.
    pub updated_ms: i64,
}

/// A listed track, as the library views need it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackRow {
    /// Snowflake.
    pub id: i64,
    /// The mount the audio lives in (the reach check is on this).
    pub mount_id: i64,
    /// Title.
    pub title: String,
    /// Artist (may be empty).
    pub artist: String,
    /// Album (may be empty).
    pub album: String,
    /// Album artist (may be empty).
    pub album_artist: String,
    /// Track number.
    pub track_no: Option<i32>,
    /// Disc number.
    pub disc_no: Option<i32>,
    /// Year.
    pub year: Option<i16>,
    /// Duration in milliseconds.
    pub duration_ms: Option<i32>,
    /// Genres.
    pub genres: Vec<String>,
    /// Tags.
    pub tags: Vec<String>,
    /// Cover key in the mount.
    pub cover_key: Option<String>,
}

/// An item of a playlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRow {
    /// The item's own id (a track may appear twice).
    pub item_id: i64,
    /// Zero-based position.
    pub position: i32,
    /// The track.
    pub track: TrackRow,
}

/// A track with the time it was favorited or last played.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StampedTrack {
    /// The track.
    pub track: TrackRow,
    /// Unix milliseconds.
    pub at_ms: i64,
}

/// The rows of the library. One implementation over Postgres; the routes' tests use an in-memory one.
#[async_trait]
pub trait MusicLibStore: Send + Sync {
    /// Create a playlist unless the owner already has `cap` of them ([`LibError::Cap`]).
    async fn create_playlist(
        &self,
        id: i64,
        owner: PlaylistOwner,
        name: &str,
        description: &str,
        created_by: i64,
        cap: i64,
    ) -> Result<PlaylistRow, LibError>;
    /// One playlist.
    async fn playlist(&self, id: i64) -> Result<Option<PlaylistRow>, LibError>;
    /// The playlists of an owner, newest first.
    async fn playlists_of(&self, owner: PlaylistOwner) -> Result<Vec<PlaylistRow>, LibError>;
    /// Rename and/or re-describe.
    async fn update_playlist(
        &self,
        id: i64,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<PlaylistRow, LibError>;
    /// Delete a playlist and its items.
    async fn delete_playlist(&self, id: i64) -> Result<(), LibError>;
    /// The items in order, with their listed tracks (an item whose track vanished is left out).
    async fn items(&self, playlist_id: i64) -> Result<Vec<ItemRow>, LibError>;
    /// Append tracks (`(item id, track id)`), refusing the whole batch when it would pass `cap` items or when a
    /// track is unknown or not listed. Returns the new item count.
    async fn add_items(
        &self,
        playlist_id: i64,
        items: &[(i64, i64)],
        added_by: i64,
        cap: i64,
    ) -> Result<i64, LibError>;
    /// Remove one item; the rest close up.
    async fn remove_item(&self, playlist_id: i64, item_id: i64) -> Result<(), LibError>;
    /// Move an item to `position` (clamped); the rest shift.
    async fn move_item(
        &self,
        playlist_id: i64,
        item_id: i64,
        position: i32,
    ) -> Result<(), LibError>;
    /// Listed tracks by id (unknown ids are absent).
    async fn tracks(&self, ids: &[i64]) -> Result<Vec<TrackRow>, LibError>;
    /// A member's favorites, newest first.
    async fn favorites(&self, user: i64) -> Result<Vec<StampedTrack>, LibError>;
    /// Favorite a listed track (idempotent); [`LibError::Cap`] past `cap` favorites.
    async fn add_favorite(&self, user: i64, track: i64, cap: i64) -> Result<(), LibError>;
    /// Remove a favorite (removing one that is not there is fine).
    async fn remove_favorite(&self, user: i64, track: i64) -> Result<(), LibError>;
    /// A member's recently played tracks, newest first.
    async fn recent(&self, user: i64, limit: i64) -> Result<Vec<StampedTrack>, LibError>;
    /// Record a play: the track moves to the top; the list is pruned to [`RECENT_KEEP`].
    async fn record_recent(&self, user: i64, track: i64) -> Result<(), LibError>;
    /// Forget the history.
    async fn clear_recent(&self, user: i64) -> Result<(), LibError>;
}

/// In-memory [`MusicLibStore`] for tests (the routes' unit tests build on it). It mirrors the Postgres rules:
/// caps, dense positions, listed tracks only, recent pruned to [`RECENT_KEEP`].
#[derive(Debug, Default)]
pub struct MemoryMusicLib {
    inner: std::sync::Mutex<MemoryLib>,
}

#[derive(Debug, Default)]
struct MemoryLib {
    tracks: Vec<TrackRow>,
    playlists: Vec<PlaylistRow>,
    /// `(playlist id, item id, track id)` in position order per playlist.
    items: Vec<(i64, i64, i64)>,
    /// `(user, track, stamp)`.
    favorites: Vec<(i64, i64, i64)>,
    /// `(user, track, stamp)`.
    recent: Vec<(i64, i64, i64)>,
    clock: i64,
}

impl MemoryLib {
    fn tick(&mut self) -> i64 {
        self.clock += 1;
        self.clock
    }
    fn track(&self, id: i64) -> Option<TrackRow> {
        self.tracks.iter().find(|t| t.id == id).cloned()
    }
    fn count(&self, playlist: i64) -> i64 {
        self.items.iter().filter(|i| i.0 == playlist).count() as i64
    }
    fn view(&self, p: &PlaylistRow) -> PlaylistRow {
        PlaylistRow {
            item_count: self.count(p.id),
            ..p.clone()
        }
    }
}

impl MemoryMusicLib {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }
    fn with<R>(&self, f: impl FnOnce(&mut MemoryLib) -> R) -> R {
        let mut g = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut g)
    }
    /// Add a listed track.
    pub fn add_track(&self, t: TrackRow) {
        self.with(|m| m.tracks.push(t));
    }
    /// Make a track vanish from the listing (the index no longer sees the file).
    pub fn vanish(&self, id: i64) {
        self.with(|m| m.tracks.retain(|t| t.id != id));
    }
}

#[async_trait]
impl MusicLibStore for MemoryMusicLib {
    async fn create_playlist(
        &self,
        id: i64,
        owner: PlaylistOwner,
        name: &str,
        description: &str,
        created_by: i64,
        cap: i64,
    ) -> Result<PlaylistRow, LibError> {
        self.with(|m| {
            let held = m.playlists.iter().filter(|p| p.owner == owner).count() as i64;
            if held >= cap {
                return Err(LibError::Cap);
            }
            let now = m.tick();
            let row = PlaylistRow {
                id,
                owner,
                name: name.to_owned(),
                description: description.to_owned(),
                created_by: Some(created_by),
                item_count: 0,
                created_ms: now,
                updated_ms: now,
            };
            m.playlists.push(row.clone());
            Ok(row)
        })
    }

    async fn playlist(&self, id: i64) -> Result<Option<PlaylistRow>, LibError> {
        Ok(self.with(|m| m.playlists.iter().find(|p| p.id == id).map(|p| m.view(p))))
    }

    async fn playlists_of(&self, owner: PlaylistOwner) -> Result<Vec<PlaylistRow>, LibError> {
        Ok(self.with(|m| {
            let mut v: Vec<PlaylistRow> = m
                .playlists
                .iter()
                .filter(|p| p.owner == owner)
                .map(|p| m.view(p))
                .collect();
            v.sort_by_key(|p| std::cmp::Reverse(p.id));
            v
        }))
    }

    async fn update_playlist(
        &self,
        id: i64,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<PlaylistRow, LibError> {
        self.with(|m| {
            let now = m.tick();
            let p = m
                .playlists
                .iter_mut()
                .find(|p| p.id == id)
                .ok_or(LibError::NotFound)?;
            if let Some(n) = name {
                p.name = n.to_owned();
            }
            if let Some(d) = description {
                p.description = d.to_owned();
            }
            p.updated_ms = now;
            let p = p.clone();
            Ok(m.view(&p))
        })
    }

    async fn delete_playlist(&self, id: i64) -> Result<(), LibError> {
        self.with(|m| {
            let before = m.playlists.len();
            m.playlists.retain(|p| p.id != id);
            if m.playlists.len() == before {
                return Err(LibError::NotFound);
            }
            m.items.retain(|i| i.0 != id);
            Ok(())
        })
    }

    async fn items(&self, playlist_id: i64) -> Result<Vec<ItemRow>, LibError> {
        Ok(self.with(|m| {
            let mut out = Vec::new();
            for (p, item, track) in &m.items {
                if *p != playlist_id {
                    continue;
                }
                if let Some(t) = m.track(*track) {
                    out.push(ItemRow {
                        item_id: *item,
                        position: i32::try_from(out.len()).unwrap_or(i32::MAX),
                        track: t,
                    });
                }
            }
            out
        }))
    }

    async fn add_items(
        &self,
        playlist_id: i64,
        items: &[(i64, i64)],
        _added_by: i64,
        cap: i64,
    ) -> Result<i64, LibError> {
        self.with(|m| {
            if !m.playlists.iter().any(|p| p.id == playlist_id) {
                return Err(LibError::NotFound);
            }
            if items.iter().any(|(_, t)| m.track(*t).is_none()) {
                return Err(LibError::NotFound);
            }
            let held = m.count(playlist_id);
            if held + items.len() as i64 > cap {
                return Err(LibError::Cap);
            }
            for (item, track) in items {
                m.items.push((playlist_id, *item, *track));
            }
            Ok(held + items.len() as i64)
        })
    }

    async fn remove_item(&self, playlist_id: i64, item_id: i64) -> Result<(), LibError> {
        self.with(|m| {
            let before = m.items.len();
            m.items.retain(|i| !(i.0 == playlist_id && i.1 == item_id));
            if m.items.len() == before {
                return Err(LibError::NotFound);
            }
            Ok(())
        })
    }

    async fn move_item(
        &self,
        playlist_id: i64,
        item_id: i64,
        position: i32,
    ) -> Result<(), LibError> {
        self.with(|m| {
            let mut mine: Vec<(i64, i64, i64)> = m
                .items
                .iter()
                .filter(|i| i.0 == playlist_id)
                .copied()
                .collect();
            let from = mine
                .iter()
                .position(|i| i.1 == item_id)
                .ok_or(LibError::NotFound)?;
            let moved = mine.remove(from);
            let to = usize::try_from(position.max(0))
                .unwrap_or(0)
                .min(mine.len());
            mine.insert(to, moved);
            m.items.retain(|i| i.0 != playlist_id);
            m.items.extend(mine);
            Ok(())
        })
    }

    async fn tracks(&self, ids: &[i64]) -> Result<Vec<TrackRow>, LibError> {
        Ok(self.with(|m| ids.iter().filter_map(|id| m.track(*id)).collect()))
    }

    async fn favorites(&self, user: i64) -> Result<Vec<StampedTrack>, LibError> {
        Ok(self.with(|m| stamped_of(m, &m.favorites, user, usize::MAX)))
    }

    async fn add_favorite(&self, user: i64, track: i64, cap: i64) -> Result<(), LibError> {
        self.with(|m| {
            if m.track(track).is_none() {
                return Err(LibError::NotFound);
            }
            if m.favorites.iter().any(|f| f.0 == user && f.1 == track) {
                return Ok(());
            }
            let held = m.favorites.iter().filter(|f| f.0 == user).count() as i64;
            if held >= cap {
                return Err(LibError::Cap);
            }
            let now = m.tick();
            m.favorites.push((user, track, now));
            Ok(())
        })
    }

    async fn remove_favorite(&self, user: i64, track: i64) -> Result<(), LibError> {
        self.with(|m| m.favorites.retain(|f| !(f.0 == user && f.1 == track)));
        Ok(())
    }

    async fn recent(&self, user: i64, limit: i64) -> Result<Vec<StampedTrack>, LibError> {
        let n = usize::try_from(limit.clamp(1, RECENT_KEEP)).unwrap_or(1);
        Ok(self.with(|m| stamped_of(m, &m.recent, user, n)))
    }

    async fn record_recent(&self, user: i64, track: i64) -> Result<(), LibError> {
        self.with(|m| {
            if m.track(track).is_none() {
                return Err(LibError::NotFound);
            }
            m.recent.retain(|r| !(r.0 == user && r.1 == track));
            let now = m.tick();
            m.recent.push((user, track, now));
            let mut stamps: Vec<i64> = m
                .recent
                .iter()
                .filter(|r| r.0 == user)
                .map(|r| r.2)
                .collect();
            stamps.sort_unstable_by(|a, b| b.cmp(a));
            let keep = usize::try_from(RECENT_KEEP).unwrap_or(0);
            if let Some(cut) = stamps.get(keep).copied() {
                m.recent.retain(|r| !(r.0 == user && r.2 <= cut));
            }
            Ok(())
        })
    }

    async fn clear_recent(&self, user: i64) -> Result<(), LibError> {
        self.with(|m| m.recent.retain(|r| r.0 != user));
        Ok(())
    }
}

fn stamped_of(
    m: &MemoryLib,
    rows: &[(i64, i64, i64)],
    user: i64,
    limit: usize,
) -> Vec<StampedTrack> {
    let mut v: Vec<&(i64, i64, i64)> = rows.iter().filter(|r| r.0 == user).collect();
    v.sort_by_key(|r| std::cmp::Reverse(r.2));
    v.iter()
        .filter_map(|r| m.track(r.1).map(|track| StampedTrack { track, at_ms: r.2 }))
        .take(limit)
        .collect()
}
