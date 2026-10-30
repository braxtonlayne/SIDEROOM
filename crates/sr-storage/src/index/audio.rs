//! The music library's side of the index (MUS-IDX-001).
//!
//! The indexer ([`super::classify`]) lists audio under any prefix (a `Music/` folder names the artist and
//! album; anywhere else the extension and the file name decide) into `music_tracks`: mount, key, size and the
//! container facts of `storage_objects`. The tags inside the files are read later, in the member's browser
//! (`apps/web/src/music/index`), which fetches the first 1 MiB of a file by a presigned range, parses it with
//! `sr-music-wasm` and writes the result back through `PATCH /music/tracks/{id}`. The server never opens a
//! file. This module holds the types, the validation of what the browser writes and the [`MusicStore`] seam;
//! the Postgres side is `sr_db::music_repo`.
//!
//! Who writes: the owner of the mount, or on a server's shelf a role with `upload` (or `manage`); never staff
//! (staff reach nothing here, the viewer is always resolved with `staff: false`) and never a friend who can
//! merely listen. The fields the tagger writes are recorded as `overridden` (a re-index keeps them) and as
//! `tagged` (a rescan may overwrite them, unlike a field a member edited by hand).

use super::StoreError;
use super::shelf::PatchError;
use async_trait::async_trait;

/// Most characters in a title, artist, album or album artist (the column limit of `title`).
pub const MAX_TEXT: usize = 300;
/// Most genres on a track.
pub const MAX_GENRES: usize = 10;
/// Page size cap of the library listing.
pub const MAX_PAGE: u32 = 200;
/// The folder, on the same mount, the browser stores covers in.
pub const COVER_DIR: &str = "covers/";

/// Where a tag write comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagSource {
    /// The lazy tagger: values read from the file. Never replaces a field the member edited by hand.
    File,
    /// A member's edit: replaces anything, and from then on a rescan leaves the field alone.
    Edit,
}

/// How far a track's tags are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagsState {
    /// Never read.
    Pending,
    /// Tags written.
    Read,
    /// Read, and the file carries none.
    None,
    /// Could not be read (retried by "Rescan tags").
    Failed,
}

impl TagsState {
    /// The column / wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Read => "read",
            Self::None => "none",
            Self::Failed => "failed",
        }
    }
    /// Parse the column / wire name.
    pub fn parse(s: &str) -> Option<Self> {
        [Self::Pending, Self::Read, Self::None, Self::Failed]
            .into_iter()
            .find(|t| t.as_str() == s)
    }
}

/// A track of the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    /// Row id in `music_tracks`.
    pub id: i64,
    /// The mount it lives on.
    pub mount_id: i64,
    /// Mount-relative key of the audio file.
    pub key: String,
    /// Size in bytes.
    pub size: i64,
    /// Container (`mp3`, `flac`, `ogg`, `m4a`, `wav`, ...).
    pub container: String,
    /// Plays in a browser as is.
    pub web_native: bool,
    /// Title.
    pub title: String,
    /// Artist.
    pub artist: String,
    /// Album.
    pub album: String,
    /// Album artist.
    pub album_artist: String,
    /// Track number.
    pub track_no: Option<i32>,
    /// Disc number.
    pub disc_no: Option<i32>,
    /// Year.
    pub year: Option<i16>,
    /// Length in milliseconds, when the tagger saw it.
    pub duration_ms: Option<i32>,
    /// Genres.
    pub genres: Vec<String>,
    /// Cover key on the same mount.
    pub cover_key: Option<String>,
    /// Fields written by a tag read or an edit.
    pub overridden: Vec<String>,
    /// `pending`, `read`, `none` or `failed`.
    pub tags_state: String,
    /// When tags were last written (unix ms).
    pub tags_read_ms: Option<i64>,
}

/// Filter of the library listing. The mounts are decided by the caller's reach before this is built.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibraryFilter {
    /// Only this mount.
    pub mount: Option<i64>,
    /// Exact album.
    pub album: Option<String>,
    /// Exact artist (matches the album artist too).
    pub artist: Option<String>,
    /// Case-insensitive substring over title, artist and album.
    pub q: Option<String>,
    /// Only tracks whose tags are still to be read (`pending` or `failed`): the lazy tagger's work list.
    pub pending: bool,
    /// Page size, at most [`MAX_PAGE`].
    pub limit: u32,
    /// Offset.
    pub offset: u32,
}

/// A page of tracks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrackPage {
    /// Tracks, ordered by album, disc, track number, title.
    pub items: Vec<Track>,
    /// All that match.
    pub total: i64,
}

/// A tag write. A set field is written; `None` leaves the column alone (a cleared number is `Some(None)`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TagWrite {
    /// Title.
    pub title: Option<String>,
    /// Artist.
    pub artist: Option<String>,
    /// Album.
    pub album: Option<String>,
    /// Album artist.
    pub album_artist: Option<String>,
    /// Track number.
    pub track_no: Option<Option<i32>>,
    /// Disc number.
    pub disc_no: Option<Option<i32>>,
    /// Year.
    pub year: Option<Option<i16>>,
    /// Length in milliseconds.
    pub duration_ms: Option<Option<i32>>,
    /// Genres.
    pub genres: Option<Vec<String>>,
    /// Cover key and its hash, together (`covers/<hash>.jpg`); `Some(None)` clears.
    pub cover: Option<Option<(String, String)>>,
    /// The state the track moves to (`read`, `none` or `failed`); an edit alone leaves it.
    pub state: Option<TagsState>,
}

impl TagWrite {
    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Reads and writes of track rows. Every read takes the mounts the caller already proved reachable.
#[async_trait]
pub trait MusicStore: Send + Sync {
    /// Mounts that have listed tracks (the candidates [`super::shelf::reachable`] decides over).
    async fn mounts(&self) -> Result<Vec<i64>, StoreError>;
    /// A page of tracks from `mounts`.
    async fn list(&self, mounts: &[i64], f: &LibraryFilter) -> Result<TrackPage, StoreError>;
    /// One track, if it lives on one of `mounts`.
    async fn get(&self, id: i64, mounts: &[i64]) -> Result<Option<Track>, StoreError>;
    /// The mount a track lives on, without the scope filter (to decide whether the caller may write).
    async fn mount_of(&self, id: i64) -> Result<Option<i64>, StoreError>;
    /// Write tags. `File` writes skip fields the member edited by hand; `Edit` writes always apply.
    async fn write(&self, id: i64, w: &TagWrite, source: TagSource) -> Result<(), PatchError>;
    /// Queue every listed track of `mount_id` for a tag read again ("Rescan tags"). Returns how many.
    async fn rescan(&self, mount_id: i64) -> Result<u64, StoreError>;
}

/// Trim, drop control characters and cut to `max` characters. Tags come from files a stranger may have made.
pub fn clean_text(raw: &str, max: usize) -> String {
    let t: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    t.chars().take(max).collect()
}

/// Genres: cleaned, de-duplicated (case-insensitively), at most [`MAX_GENRES`] of 64 characters.
pub fn clean_genres(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for g in raw {
        let g = clean_text(g, 64);
        if g.is_empty() || out.iter().any(|o| o.eq_ignore_ascii_case(&g)) {
            continue;
        }
        out.push(g);
        if out.len() == MAX_GENRES {
            break;
        }
    }
    out
}

/// The key of a cover with this hash.
pub fn cover_key_for(hash: &str) -> String {
    format!("{COVER_DIR}{hash}.jpg")
}

/// The hash of a cover key, when the key is exactly `covers/<hash>.jpg` with a lower-case hex hash of 8 to 64
/// characters. Anything else (another folder, a path, a different extension) is refused: the browser may only
/// point a track at a cover it stores in that folder.
pub fn cover_hash_of(key: &str) -> Option<&str> {
    let hash = key.strip_prefix(COVER_DIR)?.strip_suffix(".jpg")?;
    let ok = (8..=64).contains(&hash.len())
        && hash
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    ok.then_some(hash)
}

/// The names a write records in `overridden` (the vocabulary of the indexer's upsert: `track` covers the track
/// and disc numbers, `poster` the cover).
pub fn field_names(w: &TagWrite) -> Vec<&'static str> {
    [
        (w.title.is_some(), "title"),
        (w.artist.is_some(), "artist"),
        (w.album.is_some(), "album"),
        (w.album_artist.is_some(), "album_artist"),
        (w.track_no.is_some() || w.disc_no.is_some(), "track"),
        (w.year.is_some(), "year"),
        (w.genres.is_some(), "genres"),
        (w.cover.is_some(), "poster"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{Class, NameParser, Options, SidecarMap, VideoFacts, classify_all};

    struct NoVideo;
    impl NameParser for NoVideo {
        fn video(&self, path: &str) -> VideoFacts {
            VideoFacts {
                title: path.to_owned(),
                ..VideoFacts::default()
            }
        }
    }

    #[test]
    fn audio_is_listed_under_any_prefix() {
        let listing: Vec<(String, u64)> = [
            "Music/Daft Punk/Discovery/01 - One More Time.flac",
            "Stuff/Mixes/Set 1.mp3",
            "loose.ogg",
            "deep/a/b/c/d/Song.m4a",
            "Movies/Alien.mp4",
        ]
        .iter()
        .map(|k| ((*k).to_owned(), 10))
        .collect();
        let out = classify_all(&listing, &NoVideo, &SidecarMap::new(), Options::default());
        let tracks: Vec<&str> = out
            .iter()
            .filter(|c| c.class == Class::Track && !c.ignored)
            .map(|c| c.key.as_str())
            .collect();
        assert_eq!(tracks.len(), 4, "{tracks:?}");
        assert!(tracks.contains(&"Stuff/Mixes/Set 1.mp3"));
        assert!(tracks.contains(&"loose.ogg"));
        assert!(!tracks.contains(&"Movies/Alien.mp4"));
    }

    #[test]
    fn cover_keys_are_the_covers_folder_and_a_hex_hash_only() {
        assert_eq!(cover_hash_of("covers/0123abcd.jpg"), Some("0123abcd"));
        assert_eq!(cover_key_for("0123abcd"), "covers/0123abcd.jpg");
        for bad in [
            "covers/../x.jpg",
            "covers/ABCDEF12.jpg",
            "covers/0123abc.jpg",
            "covers/0123abcd.png",
            "other/0123abcd.jpg",
            "covers/a/0123abcd.jpg",
            "covers/0123abcg.jpg",
            "",
        ] {
            assert_eq!(cover_hash_of(bad), None, "{bad}");
        }
        assert_eq!(
            cover_hash_of(&format!("covers/{}.jpg", "a".repeat(65))),
            None
        );
    }

    #[test]
    fn text_from_a_file_is_flattened_and_cut() {
        assert_eq!(clean_text("  A\u{0}\tB \n C  ", 300), "A B C");
        assert_eq!(clean_text(&"x".repeat(400), 300).chars().count(), 300);
        let g = clean_genres(&["Rock".into(), "rock".into(), " ".into(), "Pop".into()]);
        assert_eq!(g, vec!["Rock".to_owned(), "Pop".to_owned()]);
        let many: Vec<String> = (0..30).map(|i| format!("g{i}")).collect();
        assert_eq!(clean_genres(&many).len(), MAX_GENRES);
    }

    #[test]
    fn a_write_names_the_overrides_the_indexer_knows() {
        let w = TagWrite {
            title: Some("t".into()),
            track_no: Some(Some(1)),
            cover: Some(None),
            ..TagWrite::default()
        };
        assert_eq!(field_names(&w), vec!["title", "track", "poster"]);
        assert!(TagWrite::default().is_empty());
        assert!(!w.is_empty());
        assert_eq!(TagsState::parse("failed"), Some(TagsState::Failed));
        assert_eq!(TagsState::parse("x"), None);
    }
}
