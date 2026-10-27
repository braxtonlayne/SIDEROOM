//! Wire and domain types of the Library catalog. Ids are Snowflakes and travel as
//! decimal strings in JSON, like every other SIDE ROOM API. Timestamps are Unix
//! milliseconds.

use crate::ingest::crop::PageCrop;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Serde adapter: `i64` id as a JSON string (accepts a number on input).
pub mod id {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &i64, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(v)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            S(String),
            N(i64),
        }
        match Raw::deserialize(d)? {
            Raw::S(s) => s.parse().map_err(serde::de::Error::custom),
            Raw::N(n) => Ok(n),
        }
    }
}

/// Serde adapter for `Option<i64>` ids.
pub mod opt_id {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<i64>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(v) => s.collect_str(v),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            S(String),
            N(i64),
        }
        match Option::<Raw>::deserialize(d)? {
            None => Ok(None),
            Some(Raw::S(s)) => s.parse().map(Some).map_err(serde::de::Error::custom),
            Some(Raw::N(n)) => Ok(Some(n)),
        }
    }
}

/// A string-backed enum with `as_str` / `parse` matching its database CHECK values.
macro_rules! str_enum {
    ($(#[$m:meta])* $name:ident { $($var:ident => $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($var),+ }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$var),+];

            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$var => $s),+ }
            }

            pub fn parse(s: &str) -> Option<Self> {
                match s { $($s => Some(Self::$var),)+ _ => None }
            }
        }
    };
}

str_enum!(
    /// `reading_series.kind`.
    SeriesKind {
        Manga => "manga",
        Manhwa => "manhwa",
        Manhua => "manhua",
        Webtoon => "webtoon",
        Comic => "comic",
        Novel => "novel",
        Book => "book",
    }
);

str_enum!(
    /// `reading_series.status`.
    SeriesStatus {
        Ongoing => "ongoing",
        Completed => "completed",
        Hiatus => "hiatus",
        Cancelled => "cancelled",
    }
);

str_enum!(
    /// Default reading mode of a series.
    ReadingDirection {
        Rtl => "rtl",
        Ltr => "ltr",
        Vertical => "vertical",
    }
);

str_enum!(
    /// How a chapter is stored.
    ChapterFormat {
        Images => "images",
        Pdf => "pdf",
        Epub => "epub",
    }
);

str_enum!(
    /// The "My library" shelf of a followed series.
    Shelf {
        Reading => "reading",
        PlanToRead => "plan_to_read",
        Completed => "completed",
        OnHold => "on_hold",
        Dropped => "dropped",
    }
);

str_enum!(
    /// Browse ordering.
    SortKey {
        Newest => "newest",
        Updated => "updated",
        Title => "title",
        Followers => "followers",
        Rating => "rating",
    }
);

str_enum!(
    /// How age-restricted series are treated by a listing. Everyone sees them by
    /// default (`include`); `hide` and `only` are optional filters (D-184).
    AgeFilter {
        Hide => "hide",
        Include => "include",
        Only => "only",
    }
);

/// Editable fields of a series (create, and full replace on update).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SeriesInput {
    pub kind: SeriesKind,
    pub title: String,
    #[serde(default)]
    pub alt_titles: Vec<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub artists: Vec<String>,
    #[serde(default = "default_status")]
    pub status: SeriesStatus,
    pub year: Option<i16>,
    #[serde(default)]
    pub synopsis: String,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Defaults from the kind (webtoon: vertical, comic: ltr, manga: rtl).
    pub reading_direction: Option<ReadingDirection>,
    #[serde(default)]
    pub age_restricted: bool,
    /// Storage key of the cover (under `library/`).
    pub cover_key: Option<String>,
}

fn default_status() -> SeriesStatus {
    SeriesStatus::Ongoing
}

/// A series row as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesRecord {
    pub id: i64,
    /// The storage mount holding this row's objects (1 is the instance bucket). Media URLs and reads go
    /// through it (`MountStorages::for_mount`), never through the instance handle.
    pub mount_id: i64,
    pub kind: SeriesKind,
    pub title: String,
    pub alt_titles: Vec<String>,
    pub authors: Vec<String>,
    pub artists: Vec<String>,
    pub status: SeriesStatus,
    pub year: Option<i16>,
    pub synopsis: String,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub reading_direction: ReadingDirection,
    pub age_restricted: bool,
    pub cover_key: Option<String>,
    pub published: bool,
    pub published_at_ms: Option<i64>,
    pub merged_into: Option<i64>,
    pub chapter_count: i32,
    pub last_chapter_at_ms: Option<i64>,
    pub follower_count: i32,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// A series as the API shows it.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Series {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub kind: SeriesKind,
    pub title: String,
    pub alt_titles: Vec<String>,
    pub authors: Vec<String>,
    pub artists: Vec<String>,
    pub status: SeriesStatus,
    pub year: Option<i16>,
    pub synopsis: String,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub reading_direction: ReadingDirection,
    pub age_restricted: bool,
    /// Signed, short-lived cover URL.
    pub cover_url: Option<String>,
    pub published: bool,
    pub published_at_ms: Option<i64>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub merged_into: Option<i64>,
    /// Published chapters across all editions.
    pub chapter_count: i32,
    pub last_chapter_at_ms: Option<i64>,
    pub follower_count: i32,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EditionInput {
    /// BCP 47 language tag.
    pub language: String,
    /// "Official English", "Scanlation X".
    pub label: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct Edition {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub series_id: i64,
    pub language: String,
    pub label: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct VolumeInput {
    pub number: f64,
    pub title: Option<String>,
    pub cover_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Volume {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub edition_id: i64,
    pub number: f64,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ChapterInput {
    pub number: f64,
    pub title: Option<String>,
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub volume_id: Option<i64>,
    #[serde(default = "default_format")]
    pub format: ChapterFormat,
    /// Storage key of the original file (`pdf` / `epub` chapters).
    pub source_key: Option<String>,
    pub release_group: Option<String>,
}

fn default_format() -> ChapterFormat {
    ChapterFormat::Images
}

/// A chapter row as stored, with the visibility facts of its series.
#[derive(Debug, Clone, PartialEq)]
pub struct ChapterRecord {
    pub id: i64,
    /// The storage mount holding this row's objects (1 is the instance bucket). Media URLs and reads go
    /// through it (`MountStorages::for_mount`), never through the instance handle.
    pub mount_id: i64,
    pub series_id: i64,
    pub edition_id: i64,
    pub volume_id: Option<i64>,
    pub number: f64,
    pub title: Option<String>,
    pub page_count: i32,
    pub format: ChapterFormat,
    pub source_key: Option<String>,
    pub release_group: Option<String>,
    pub published: bool,
    pub published_at_ms: Option<i64>,
    pub series_published: bool,
    pub series_age_restricted: bool,
    pub series_direction: ReadingDirection,
    pub series_merged: bool,
    /// Reading progress of the requesting user, when there is one.
    pub progress: Option<ChapterProgress>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, ToSchema)]
pub struct ChapterProgress {
    pub page_index: i32,
    pub percent: f32,
    pub completed: bool,
    /// When this progress was last saved; send it back as `base_updated_at_ms` when saving.
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Chapter {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub series_id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub edition_id: i64,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub volume_id: Option<i64>,
    pub number: f64,
    pub title: Option<String>,
    pub page_count: i32,
    pub format: ChapterFormat,
    pub release_group: Option<String>,
    pub published: bool,
    pub published_at_ms: Option<i64>,
    /// The caller's progress (read / unread marks); absent when there is none.
    pub progress: Option<ChapterProgress>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ChapterList {
    pub editions: Vec<Edition>,
    pub volumes: Vec<Volume>,
    pub chapters: Vec<Chapter>,
    /// Pass back as `after` for the next page; absent at the end or when no `limit` was given.
    pub next_cursor: Option<String>,
}

/// Paging of a chapter list (keyset over edition, number, id).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChapterQuery {
    /// Only this edition's chapters.
    pub edition: Option<i64>,
    /// Cursor from the previous page.
    pub after: Option<String>,
    /// `None` returns every chapter.
    pub limit: Option<u32>,
    pub descending: bool,
}

/// Decoded chapter cursor: the last row of the previous page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChapterCursor {
    pub edition_id: i64,
    /// Chapter number in hundredths (`NUMERIC(8,2)`).
    pub number_cents: i64,
    pub id: i64,
}

/// One stored page of a chapter.
#[derive(Debug, Clone, PartialEq)]
pub struct PageRecord {
    pub index: i32,
    pub key: String,
    /// `(width, key)` sized variants, ascending.
    pub variants: Vec<(u32, String)>,
    pub width: i32,
    pub height: i32,
    pub is_double_spread: bool,
    pub blurhash: Option<String>,
    pub crop: Option<PageCrop>,
}

/// Replacement page list entry (admin).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PageInput {
    /// Full-size WebP object key; must live under `library/{series}/{chapter}/`.
    pub key: String,
    /// Sized variants as `{"720": key, ...}`.
    #[serde(default)]
    pub variants: std::collections::BTreeMap<String, String>,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub is_double_spread: bool,
    pub blurhash: Option<String>,
    /// Borders the reader may cut off (pixels of the full-size page); see `ingest::crop`.
    #[serde(default)]
    pub crop: Option<PageCrop>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct PageVariant {
    pub width: u32,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct PageEntry {
    pub index: i32,
    pub width: i32,
    pub height: i32,
    pub is_double_spread: bool,
    pub blurhash: Option<String>,
    /// Uniform borders the reader can crop (pixels of the full-size image), when detected.
    pub crop: Option<PageCrop>,
    /// Full-size signed URL.
    pub url: String,
    /// Smaller signed variants; pick by viewport width times device pixel ratio.
    pub variants: Vec<PageVariant>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PageManifest {
    pub chapter: Chapter,
    pub reading_direction: ReadingDirection,
    pub pages: Vec<PageEntry>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub prev_chapter_id: Option<i64>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub next_chapter_id: Option<i64>,
    /// Unix seconds when the signed URLs stop working.
    pub expires_at: u64,
    /// Signed link to the original file of an EPUB or PDF chapter (such a chapter has no pages).
    pub source_url: Option<String>,
}

/// A followed series with the user's shelf.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Follow {
    pub shelf: Shelf,
    pub notify: bool,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct FollowedSeries {
    pub series: Series,
    pub follow: Follow,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct SeriesDetail {
    pub series: Series,
    pub editions: Vec<Edition>,
    /// The caller's follow, if any.
    pub follow: Option<Follow>,
}

/// One entry of the latest-updates feed.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct UpdateItem {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub chapter_id: i64,
    pub chapter_number: f64,
    pub chapter_title: Option<String>,
    pub published_at_ms: i64,
    pub release_group: Option<String>,
    pub language: String,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub series_id: i64,
    pub series_title: String,
    pub kind: SeriesKind,
    pub age_restricted: bool,
    pub cover_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct UpdatesPage {
    pub items: Vec<UpdateItem>,
    /// Pass back as `before` for the next page; absent at the end.
    pub next_before: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct SeriesPage {
    pub items: Vec<Series>,
    pub total: i64,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct FollowPage {
    pub items: Vec<FollowedSeries>,
    pub total: i64,
    pub limit: u32,
    pub offset: u32,
}

/// Instant-search result.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct InstantSearch {
    pub series: Vec<Series>,
    pub authors: Vec<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
}

/// Which rows a read may return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Members: published and not merged away. Age-restricted series are included;
    /// only opening their pages needs age confirmation (D-184).
    Public,
    /// Publishers: everything.
    Staff,
}

/// Browse filters (LIB-DATA-003). Empty means unfiltered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseQuery {
    pub kind: Option<SeriesKind>,
    pub status: Option<SeriesStatus>,
    pub genres: Vec<String>,
    pub exclude_genres: Vec<String>,
    pub tags: Vec<String>,
    /// Series that have a published chapter in this language.
    pub language: Option<String>,
    pub year_from: Option<i16>,
    pub year_to: Option<i16>,
    /// Series whose average rating (1 to 10) is at least this.
    pub rating_min: Option<i16>,
    pub age: AgeFilter,
    pub sort: SortKey,
    /// Overrides the natural direction of `sort`.
    pub descending: Option<bool>,
    /// Staff scope only: restrict to published / unpublished series.
    pub published: Option<bool>,
    /// Staff scope only: title substring.
    pub title_contains: Option<String>,
    pub limit: u32,
    pub offset: u32,
}

impl Default for BrowseQuery {
    fn default() -> Self {
        Self {
            kind: None,
            status: None,
            genres: Vec::new(),
            exclude_genres: Vec::new(),
            tags: Vec::new(),
            language: None,
            year_from: None,
            year_to: None,
            rating_min: None,
            age: AgeFilter::Include,
            sort: SortKey::Updated,
            descending: None,
            published: None,
            title_contains: None,
            limit: 24,
            offset: 0,
        }
    }
}

str_enum!(
    /// What an uploaded file is expected to be (from the chapter's format).
    UploadKind {
        Archive => "archive",
        Epub => "epub",
        Pdf => "pdf",
    }
);

str_enum!(
    /// Where an upload is in its life.
    UploadStatus {
        AwaitingUpload => "awaiting_upload",
        Queued => "queued",
        Running => "running",
        Done => "done",
        Failed => "failed",
        Expired => "expired",
    }
);

/// An upload row as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct UploadRecord {
    pub id: i64,
    pub chapter_id: i64,
    pub series_id: i64,
    pub user_id: Option<i64>,
    pub filename: String,
    pub kind: UploadKind,
    pub size_bytes: i64,
    pub object_key: String,
    pub status: UploadStatus,
    pub error: Option<String>,
    pub warnings: Vec<String>,
    pub page_count: Option<i32>,
    pub metadata: serde_json::Value,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub finished_at_ms: Option<i64>,
}

/// An upload as the API shows it (the status the client polls).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Upload {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub chapter_id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub series_id: i64,
    pub filename: String,
    pub kind: UploadKind,
    pub size_bytes: i64,
    pub status: UploadStatus,
    /// Why the upload failed, naming the offending page or file.
    pub error: Option<String>,
    /// Ignored files and unreadable metadata.
    pub warnings: Vec<String>,
    /// Pages stored (archives) or spine items (EPUB).
    pub page_count: Option<i32>,
    /// Title, number, volume, writers and so on read from the file, for the editor to offer.
    pub metadata: serde_json::Value,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub finished_at_ms: Option<i64>,
}

/// Body of `POST /admin/library/chapters/{id}/uploads`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = LibraryUploadRequest)]
pub struct UploadRequest {
    /// Original file name; its extension must fit the chapter's format.
    pub filename: String,
    pub size_bytes: u64,
    /// Ask for a resumable multipart upload: parts of 64 MB sent in parallel, resumable after a
    /// refresh. Without it the slot is one presigned `PUT`.
    #[serde(default)]
    pub multipart: bool,
}

/// A new upload slot: PUT the file to `upload_url`, then complete the upload.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = LibraryUploadSlot)]
pub struct UploadSlot {
    pub upload: Upload,
    /// Set for a multipart upload (then `upload_url` is empty).
    pub multipart: Option<crate::multipart::MultipartSlot>,
    pub upload_url: String,
    /// Always `PUT`.
    pub upload_method: String,
    pub expires_in_secs: u64,
    /// The operator's size limit for this upload.
    pub max_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct UploadList {
    pub uploads: Vec<Upload>,
}
