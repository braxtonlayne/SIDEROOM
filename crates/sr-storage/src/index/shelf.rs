//! What the shelves read (BYOS-CAT-002): the catalog rows a mount's index built, as one shape for the three
//! sections, and the decision of which mounts a viewer may see them from.
//!
//! The decision is [`reachable`], which runs `scopes::access` once per mount that has rows (not once per row), so
//! a page of 50 items costs a handful of facts queries however many items there are. A row is shown only when its
//! mount is reachable for the section's kind and its object is not delisted.

use async_trait::async_trait;

use super::StoreError;
use crate::mounts::{Mount, MountResolver};
use crate::scopes::{Access, Kind, ResolveError, ScopeFacts, Viewer, access};

/// The three shelf sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    /// Films and shows.
    Cinema,
    /// Manga, comics and books.
    Library,
    /// Tracks.
    Music,
}

impl Section {
    /// The path / column name.
    pub fn as_str(self) -> &'static str {
        self.kind().as_str()
    }
    /// Parse the path segment.
    pub fn parse(s: &str) -> Option<Self> {
        Kind::parse(s).and_then(Self::from_kind)
    }
    /// The scope kind of the section.
    pub fn kind(self) -> Kind {
        match self {
            Self::Cinema => Kind::Cinema,
            Self::Library => Kind::Library,
            Self::Music => Kind::Music,
        }
    }
    /// The section of a scope kind (`None` for games: the emulator lists ROMs through the bucket browser).
    pub fn from_kind(k: Kind) -> Option<Self> {
        match k {
            Kind::Cinema => Some(Self::Cinema),
            Kind::Library => Some(Self::Library),
            Kind::Music => Some(Self::Music),
            Kind::Games => None,
        }
    }
}

/// Filters of a shelf listing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShelfFilter {
    /// Title substring.
    pub q: Option<String>,
    /// `movie`, `series`, `anime` (cinema); `manga`, `comic`, `book` (library); ignored for music.
    pub kind: Option<String>,
    /// Only this mount (the "Shelves" filter's chip).
    pub mount: Option<i64>,
    /// Page size, at most 50.
    pub limit: u32,
    /// Offset.
    pub offset: u32,
}

/// One title, series, book or track on a shelf.
#[derive(Debug, Clone, PartialEq)]
pub struct ShelfItem {
    /// Row id in `video_titles`, `reading_series` or `music_tracks`.
    pub id: i64,
    /// The section.
    pub section: Section,
    /// `movie`, `series`, `anime`, `manga`, `comic`, `book` or `track`.
    pub kind: String,
    /// Title (a track's title, a series' name).
    pub title: String,
    /// A line under the title: `artist - album` for a track, empty otherwise.
    pub subtitle: String,
    /// Year.
    pub year: Option<i16>,
    /// The mount the rows live on.
    pub mount_id: i64,
    /// Poster or cover key on that mount.
    pub poster_key: Option<String>,
    /// Backdrop key on that mount.
    pub backdrop_key: Option<String>,
    /// Tags.
    pub tags: Vec<String>,
    /// Genres.
    pub genres: Vec<String>,
    /// Synopsis (fetched or typed).
    pub synopsis: String,
    /// The provider's attribution, when details were fetched from one.
    pub attribution: Option<String>,
    /// Provider ids (`tmdb`, `anilist`, ...), as JSON text.
    pub external_ids: String,
    /// Fields the member edited (a re-index leaves them alone).
    pub overridden: Vec<String>,
    /// Episodes, chapters or tracks listed (0 for a film or a track).
    pub parts: i64,
    /// The file of a film, a track or a single book; `None` for a series.
    pub object_key: Option<String>,
    /// Its container (`mp4`, `mkv`, `flac`, ...).
    pub container: String,
    /// Plays in a browser as is.
    pub web_native: bool,
}

/// An episode, chapter or volume of a series.
#[derive(Debug, Clone, PartialEq)]
pub struct ShelfPart {
    /// Row id in `video_episodes` or `reading_chapters`.
    pub id: i64,
    /// Season (episodes).
    pub season: Option<i32>,
    /// Episode, chapter or volume number.
    pub number: f64,
    /// Title.
    pub title: String,
    /// The file.
    pub object_key: String,
    /// Container.
    pub container: String,
    /// Plays in a browser as is.
    pub web_native: bool,
    /// Size in bytes.
    pub size: i64,
}

/// An item with its parts.
#[derive(Debug, Clone, PartialEq)]
pub struct ShelfDetail {
    /// The item.
    pub item: ShelfItem,
    /// Its episodes or chapters, in order.
    pub parts: Vec<ShelfPart>,
}

/// A page.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShelfPage {
    /// Items.
    pub items: Vec<ShelfItem>,
    /// All that match (before paging).
    pub total: i64,
}

/// An edit of an item. A set field is written and recorded in `overridden`; a `Some("")` clears text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShelfPatch {
    /// Title.
    pub title: Option<String>,
    /// Year; `Some(None)` clears.
    pub year: Option<Option<i16>>,
    /// Kind (`movie`/`series`/`anime`, `manga`/`comic`/`book`).
    pub kind: Option<String>,
    /// Tags.
    pub tags: Option<Vec<String>>,
    /// Genres.
    pub genres: Option<Vec<String>>,
    /// Synopsis.
    pub synopsis: Option<String>,
    /// Poster or cover key on the same mount; `Some(None)` clears.
    pub poster_key: Option<Option<String>>,
    /// Backdrop key on the same mount.
    pub backdrop_key: Option<Option<String>>,
    /// Artist (tracks).
    pub artist: Option<String>,
    /// Album (tracks).
    pub album: Option<String>,
    /// Track number (tracks).
    pub track_no: Option<Option<i32>>,
    /// Provider ids as JSON text (`{"tmdb":"603"}`); set together with `attribution`.
    pub external_ids: Option<String>,
    /// The provider's attribution text.
    pub attribution: Option<String>,
}

/// An edit of one episode or chapter.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PartPatch {
    /// Title.
    pub title: Option<String>,
    /// Number (episode, chapter or volume).
    pub number: Option<f64>,
    /// Season (episodes).
    pub season: Option<i32>,
}

/// Why an edit did not apply.
#[derive(Debug, thiserror::Error)]
pub enum PatchError {
    /// No such item on the mount.
    #[error("not found")]
    NotFound,
    /// The value cannot be stored (a duplicate episode number, a key that is not on the mount).
    #[error("invalid: {0}")]
    Invalid(String),
    /// The store failed.
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Reads and edits of shelf rows. Every method takes the mounts the caller already proved reachable.
#[async_trait]
pub trait ShelfStore: Send + Sync {
    /// Mounts that have indexed ROMs (wave 37; the emulator's list, see `super::rom`). The default is none, for stores
    /// that predate the emulator.
    async fn mounts_with_roms(&self) -> Result<Vec<i64>, StoreError> {
        Ok(Vec::new())
    }
    /// The ROMs of `mounts`, in title order, at most `limit`.
    async fn roms(
        &self,
        _mounts: &[i64],
        _limit: i64,
    ) -> Result<Vec<super::rom::RomRow>, StoreError> {
        Ok(Vec::new())
    }
    /// Mounts that have listed rows of `section` (the candidates [`reachable`] decides over).
    async fn mounts_with(&self, section: Section) -> Result<Vec<i64>, StoreError>;
    /// A page of items from `mounts`.
    async fn list(
        &self,
        section: Section,
        mounts: &[i64],
        f: &ShelfFilter,
    ) -> Result<ShelfPage, StoreError>;
    /// One item with its parts, if it lives on one of `mounts`.
    async fn get(
        &self,
        section: Section,
        id: i64,
        mounts: &[i64],
    ) -> Result<Option<ShelfDetail>, StoreError>;
    /// The mount an item lives on (to decide whether the caller may edit it), without the scope filter.
    async fn mount_of(&self, section: Section, id: i64) -> Result<Option<i64>, StoreError>;
    /// Apply an edit; the fields it sets become overrides. Returns the item.
    async fn patch(&self, section: Section, id: i64, patch: &ShelfPatch) -> Result<(), PatchError>;
    /// Apply an edit to one part (episode, chapter); the item must live on `mount_id`.
    async fn patch_part(
        &self,
        section: Section,
        id: i64,
        part: i64,
        mount_id: i64,
        patch: &PartPatch,
    ) -> Result<(), PatchError>;
    /// The object keys of `ids` (to drop delisted ones from a page).
    async fn object_keys(
        &self,
        section: Section,
        ids: &[i64],
    ) -> Result<Vec<(i64, String)>, StoreError>;
}

/// A mount the viewer reaches, with how.
#[derive(Debug, Clone)]
pub struct Reach {
    /// The mount.
    pub mount: Mount,
    /// How the viewer reaches it, and for which kinds.
    pub access: Access,
}

/// The mounts with listed rows of `section` that `viewer` may see, with the scope each is reached by. A mount
/// the viewer cannot reach (not a friend, no share of the kind, no grant, disabled, owner suspended) is simply
/// absent; a facts failure fails the whole call (nothing is admitted).
pub async fn reachable(
    facts: &dyn ScopeFacts,
    mounts: &dyn MountResolver,
    store: &dyn ShelfStore,
    viewer: Viewer,
    section: Section,
) -> Result<Vec<Reach>, StoreError> {
    let mut out = Vec::new();
    for id in store.mounts_with(section).await? {
        let m = match mounts.by_id(id).await {
            Ok(Some(m)) => m,
            Ok(None) => continue,
            Err(e) => return Err(StoreError(e.to_string())),
        };
        match access(facts, viewer, &m).await {
            Ok(a) => {
                if a.unrestricted || a.kinds.admits(Some(section.kind())) {
                    out.push(Reach {
                        mount: m,
                        access: a,
                    });
                }
            }
            Err(ResolveError::Refused(_)) => {}
            Err(ResolveError::Facts(e)) => return Err(StoreError(e.to_string())),
        }
    }
    Ok(out)
}

/// Drop items whose object is delisted. `delisted(mount, keys)` is `ScopeFacts::delisted`.
pub async fn drop_delisted(
    facts: &dyn ScopeFacts,
    store: &dyn ShelfStore,
    section: Section,
    items: Vec<ShelfItem>,
) -> Result<Vec<ShelfItem>, StoreError> {
    let ids: Vec<i64> = items.iter().map(|i| i.id).collect();
    let keys = store.object_keys(section, &ids).await?;
    let mut gone: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut by_mount: std::collections::HashMap<i64, Vec<(i64, String)>> = Default::default();
    for it in &items {
        if let Some((_, k)) = keys.iter().find(|(id, _)| *id == it.id) {
            by_mount
                .entry(it.mount_id)
                .or_default()
                .push((it.id, k.clone()));
        }
    }
    for (mount, rows) in by_mount {
        let ks: Vec<String> = rows.iter().map(|(_, k)| k.clone()).collect();
        let hidden = facts
            .delisted(mount, &ks)
            .await
            .map_err(|e| StoreError(e.to_string()))?;
        for (id, k) in rows {
            if hidden.contains(&k) {
                gone.insert(id);
            }
        }
    }
    Ok(items
        .into_iter()
        .filter(|i| !gone.contains(&i.id))
        .collect())
}
