//! Wire and domain types of the Cinema catalog. Ids are Snowflakes and travel as decimal
//! strings in JSON, like every other SIDE ROOM API. Timestamps are Unix milliseconds.
//! The HTTP shapes are documented in docs/specs/cinema-media.md section 8.

use std::collections::BTreeMap;

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

/// Serde adapter for `Vec<i64>` ids.
pub mod id_list {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<i64>, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            S(String),
            N(i64),
        }
        Vec::<Raw>::deserialize(d)?
            .into_iter()
            .map(|r| match r {
                Raw::S(s) => s.parse().map_err(serde::de::Error::custom),
                Raw::N(n) => Ok(n),
            })
            .collect()
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
    /// `video_titles.kind`.
    TitleKind {
        Movie => "movie",
        Series => "series",
        Anime => "anime",
    }
);

str_enum!(
    /// `video_trailers.kind`.
    TrailerKind {
        Trailer => "trailer",
        Teaser => "teaser",
        Clip => "clip",
    }
);

str_enum!(
    /// `video_markers.kind`: the windows the player offers to skip.
    MarkerKind {
        Intro => "intro",
        Credits => "credits",
        Recap => "recap",
    }
);

str_enum!(
    /// How a collection is shown.
    CollectionDisplay {
        Row => "row",
        Hero => "hero",
        Banner => "banner",
    }
);

str_enum!(
    /// A page a collection can be placed on.
    Surface {
        Discover => "discover",
        Movies => "movies",
        Series => "series",
        Anime => "anime",
    }
);

str_enum!(
    /// Browse ordering.
    SortKey {
        Updated => "updated",
        Newest => "newest",
        Title => "title",
        Year => "year",
        Runtime => "runtime",
        Curated => "curated",
    }
);

impl SortKey {
    /// Whether the sort runs descending when the request does not say.
    pub const fn natural_desc(self) -> bool {
        matches!(self, Self::Updated | Self::Newest | Self::Year)
    }
}

str_enum!(
    /// How age-restricted titles are treated by a listing. Everyone sees them by default
    /// (`include`); `hide` and `only` are optional filters (D-184).
    AgeFilter {
        Hide => "hide",
        Include => "include",
        Only => "only",
    }
);

str_enum!(
    /// Where a row's titles come from.
    RowKind {
        Curated => "curated",
        Generated => "generated",
        Provided => "provided",
    }
);

/// Which rows a read may return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Members: published and not merged away. Age-restricted titles are included; only
    /// opening them needs age confirmation (D-184).
    Public,
    /// Publishers: everything.
    Staff,
}

// ---- titles -------------------------------------------------------------

/// Editable fields of a title (create, and full replace on update). The asset link is not
/// part of it, so editing metadata never unlinks a playable asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TitleInput {
    pub kind: TitleKind,
    pub title: String,
    pub original_title: Option<String>,
    #[serde(default)]
    pub alt_titles: Vec<String>,
    pub year: Option<i16>,
    /// Minutes: a film's runtime, or the typical episode runtime.
    pub runtime_min: Option<i32>,
    /// "PG-13", "TV-MA", ...
    pub content_rating: Option<String>,
    #[serde(default)]
    pub synopsis: String,
    pub tagline: Option<String>,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Audio languages (BCP 47).
    #[serde(default)]
    pub languages: Vec<String>,
    /// Subtitle languages (BCP 47).
    #[serde(default)]
    pub subtitle_languages: Vec<String>,
    #[serde(default)]
    pub cast: Vec<String>,
    #[serde(default)]
    pub directors: Vec<String>,
    #[serde(default)]
    pub writers: Vec<String>,
    #[serde(default)]
    pub studios: Vec<String>,
    /// Other crew as "Role: Name" lines.
    #[serde(default)]
    pub crew: Vec<String>,
    #[serde(default)]
    pub age_restricted: bool,
    /// Storage keys under `cinema/`.
    pub poster_key: Option<String>,
    pub backdrop_key: Option<String>,
    pub logo_key: Option<String>,
    /// `{"imdb": "tt0111161", "tmdb": "278"}`.
    #[serde(default)]
    #[schema(value_type = BTreeMap<String, String>)]
    pub external_ids: BTreeMap<String, String>,
}

/// A title row as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct TitleRecord {
    pub id: i64,
    pub kind: TitleKind,
    pub title: String,
    pub original_title: Option<String>,
    pub alt_titles: Vec<String>,
    pub sort_title: String,
    pub year: Option<i16>,
    pub runtime_min: Option<i32>,
    pub content_rating: Option<String>,
    pub synopsis: String,
    pub tagline: Option<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub languages: Vec<String>,
    pub subtitle_languages: Vec<String>,
    pub cast: Vec<String>,
    pub directors: Vec<String>,
    pub writers: Vec<String>,
    pub studios: Vec<String>,
    pub crew: Vec<String>,
    pub age_restricted: bool,
    pub poster_key: Option<String>,
    pub backdrop_key: Option<String>,
    pub logo_key: Option<String>,
    pub external_ids: BTreeMap<String, String>,
    pub asset_id: Option<i64>,
    pub published: bool,
    pub published_at_ms: Option<i64>,
    pub merged_into: Option<i64>,
    pub season_count: i32,
    pub episode_count: i32,
    pub last_content_at_ms: Option<i64>,
    pub has_trailer: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// The value the browse query ordered by, for the keyset cursor (browse only).
    pub sort_value: Option<SortValue>,
}

/// A title as the API shows it.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Title {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub kind: TitleKind,
    pub title: String,
    pub original_title: Option<String>,
    pub alt_titles: Vec<String>,
    pub year: Option<i16>,
    pub runtime_min: Option<i32>,
    pub content_rating: Option<String>,
    pub synopsis: String,
    pub tagline: Option<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub languages: Vec<String>,
    pub subtitle_languages: Vec<String>,
    pub cast: Vec<String>,
    pub directors: Vec<String>,
    pub writers: Vec<String>,
    pub studios: Vec<String>,
    pub crew: Vec<String>,
    pub age_restricted: bool,
    /// Signed, short-lived artwork URLs.
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub logo_url: Option<String>,
    #[schema(value_type = BTreeMap<String, String>)]
    pub external_ids: BTreeMap<String, String>,
    /// Seasons with a published episode.
    pub season_count: i32,
    /// Published episodes.
    pub episode_count: i32,
    pub has_trailer: bool,
    /// Something can be opened: the title's own asset or a published episode.
    pub playable: bool,
    pub published: bool,
    pub published_at_ms: Option<i64>,
    pub last_content_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// The playable asset of a film. Staff routes only.
    #[serde(with = "opt_id", skip_serializing_if = "Option::is_none", default)]
    #[schema(value_type = Option<String>)]
    pub asset_id: Option<i64>,
}

/// A page of a browse listing.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct TitlePage {
    pub items: Vec<Title>,
    /// Titles matching the filters, independent of the cursor.
    pub total: i64,
    /// Pass as `cursor` for the next page; absent on the last page.
    pub next_cursor: Option<String>,
}

// ---- seasons and episodes ------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SeasonInput {
    /// 0 is the specials season.
    pub number: i32,
    pub name: Option<String>,
    pub synopsis: Option<String>,
    pub poster_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeasonRecord {
    pub id: i64,
    pub title_id: i64,
    pub number: i32,
    pub name: Option<String>,
    pub synopsis: Option<String>,
    pub poster_key: Option<String>,
    pub episode_count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct Season {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub title_id: i64,
    pub number: i32,
    pub name: Option<String>,
    pub synopsis: Option<String>,
    pub poster_url: Option<String>,
    /// Published episodes (all of them on staff routes).
    pub episode_count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct EpisodeInput {
    pub number: i32,
    /// Running number across seasons (anime).
    pub absolute_number: Option<i32>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub synopsis: String,
    /// `YYYY-MM-DD`.
    pub air_date: Option<String>,
    pub runtime_min: Option<i32>,
    pub still_key: Option<String>,
}

/// An episode row with the facts of its season and title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeRecord {
    pub id: i64,
    pub title_id: i64,
    pub season_id: i64,
    pub season_number: i32,
    pub number: i32,
    pub absolute_number: Option<i32>,
    pub title: String,
    pub synopsis: String,
    pub air_date: Option<String>,
    pub runtime_min: Option<i32>,
    pub still_key: Option<String>,
    pub asset_id: Option<i64>,
    pub published: bool,
    pub published_at_ms: Option<i64>,
    pub title_published: bool,
    pub title_merged: bool,
    pub title_age_restricted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct Episode {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub title_id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub season_id: i64,
    pub season_number: i32,
    pub number: i32,
    pub absolute_number: Option<i32>,
    pub title: String,
    pub synopsis: String,
    /// `YYYY-MM-DD`.
    pub air_date: Option<String>,
    pub runtime_min: Option<i32>,
    pub still_url: Option<String>,
    pub playable: bool,
    pub published: bool,
    pub published_at_ms: Option<i64>,
    /// Staff routes only.
    #[serde(with = "opt_id", skip_serializing_if = "Option::is_none", default)]
    #[schema(value_type = Option<String>)]
    pub asset_id: Option<i64>,
}

/// Seasons and a page of episodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct EpisodeList {
    pub seasons: Vec<Season>,
    pub episodes: Vec<Episode>,
    /// Pass as `after` for the next page; absent on the last page.
    pub next_cursor: Option<String>,
}

// ---- variants, trailers, markers -------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct VariantInput {
    /// "Director's Cut", "Extended".
    pub label: String,
    pub notes: Option<String>,
    pub runtime_min: Option<i32>,
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub asset_id: Option<i64>,
    pub position: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantRecord {
    pub id: i64,
    pub title_id: i64,
    pub label: String,
    pub notes: Option<String>,
    pub runtime_min: Option<i32>,
    pub asset_id: Option<i64>,
    pub position: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct Variant {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub title_id: i64,
    pub label: String,
    pub notes: Option<String>,
    pub runtime_min: Option<i32>,
    pub playable: bool,
    pub position: i32,
    #[serde(with = "opt_id", skip_serializing_if = "Option::is_none", default)]
    #[schema(value_type = Option<String>)]
    pub asset_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TrailerInput {
    pub label: String,
    #[serde(default = "default_trailer_kind")]
    pub kind: TrailerKind,
    pub language: Option<String>,
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub asset_id: Option<i64>,
}

fn default_trailer_kind() -> TrailerKind {
    TrailerKind::Trailer
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrailerRecord {
    pub id: i64,
    pub title_id: i64,
    pub label: String,
    pub kind: TrailerKind,
    pub language: Option<String>,
    pub asset_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct Trailer {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub title_id: i64,
    pub label: String,
    pub kind: TrailerKind,
    pub language: Option<String>,
    pub playable: bool,
    /// The title is age-restricted and the account has not confirmed its age, so `asset_id`
    /// is withheld.
    pub age_gated: bool,
    #[serde(with = "opt_id", skip_serializing_if = "Option::is_none", default)]
    #[schema(value_type = Option<String>)]
    pub asset_id: Option<i64>,
}

/// A skip window as sent by editors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MarkerInput {
    pub kind: MarkerKind,
    pub start_ms: i32,
    pub end_ms: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct MarkersBody {
    pub markers: Vec<MarkerInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkerRecord {
    pub id: i64,
    pub episode_id: Option<i64>,
    pub kind: MarkerKind,
    pub start_ms: i32,
    pub end_ms: i32,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct Marker {
    pub kind: MarkerKind,
    pub start_ms: i32,
    pub end_ms: i32,
    /// Staff routes only.
    #[serde(with = "opt_id", skip_serializing_if = "Option::is_none", default)]
    #[schema(value_type = Option<String>)]
    pub id: Option<i64>,
    /// `manual` or `auto`. Staff routes only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// A member credited for a film or for episodes of a title (CIN-SOC-003). Members who hide their
/// contributions, or were hidden by staff, are not listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[schema(as = CinemaContributor)]
pub struct Contributor {
    pub user_id: i64,
    pub name: String,
    /// 1 when the member is credited for the title itself (a film or a whole series).
    pub titles: i64,
    /// Credited episodes of this title.
    pub episodes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[schema(as = CinemaContributors)]
pub struct Contributors {
    pub contributors: Vec<Contributor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct MarkerList {
    pub markers: Vec<Marker>,
}

// ---- detail -----------------------------------------------------------------

/// A title with what belongs to it, as members see it.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct TitleDetail {
    pub title: Title,
    pub seasons: Vec<Season>,
    pub variants: Vec<Variant>,
    pub trailers: Vec<Trailer>,
    /// Skip windows of the title itself (films); episodes have their own.
    pub markers: Vec<Marker>,
}

/// Everything about a title, unpublished parts included (staff).
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct AdminTitleDetail {
    pub title: Title,
    pub seasons: Vec<Season>,
    pub episodes: Vec<Episode>,
    pub variants: Vec<Variant>,
    pub trailers: Vec<Trailer>,
    pub markers: Vec<Marker>,
}

/// What a player needs to open a title or an episode.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PlayTarget {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub title_id: i64,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub episode_id: Option<i64>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub variant_id: Option<i64>,
    /// The asset the playback routes take.
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub asset_id: i64,
    pub title: String,
    pub episode_title: Option<String>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub runtime_min: Option<i32>,
    pub age_restricted: bool,
    pub markers: Vec<Marker>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub next_episode_id: Option<i64>,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub previous_episode_id: Option<i64>,
}

// ---- collections -------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CollectionInput {
    /// Lower-case letters, digits and dashes.
    pub slug: String,
    pub name: String,
    pub eyebrow: Option<String>,
    pub blurb: Option<String>,
    #[serde(default = "default_display")]
    pub display: CollectionDisplay,
    /// Pages the collection appears on.
    #[serde(default)]
    pub surfaces: Vec<Surface>,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub published: bool,
}

fn default_display() -> CollectionDisplay {
    CollectionDisplay::Row
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionRecord {
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub eyebrow: Option<String>,
    pub blurb: Option<String>,
    pub display: CollectionDisplay,
    pub surfaces: Vec<Surface>,
    pub position: i32,
    pub published: bool,
    pub item_count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct Collection {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    pub slug: String,
    pub name: String,
    pub eyebrow: Option<String>,
    pub blurb: Option<String>,
    pub display: CollectionDisplay,
    pub surfaces: Vec<Surface>,
    pub position: i32,
    pub published: bool,
    /// Titles in the collection (members count the published ones).
    pub item_count: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct CollectionList {
    pub items: Vec<Collection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct CollectionDetail {
    pub collection: Collection,
    pub items: Vec<Title>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct CollectionItemsBody {
    /// Titles in display order; replaces the current set.
    #[serde(with = "id_list")]
    #[schema(value_type = Vec<String>)]
    pub title_ids: Vec<i64>,
}

// ---- rows ---------------------------------------------------------------------

/// A title in a row, with the extras a row provider may attach.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct RowItem {
    #[serde(flatten)]
    pub title: Title,
    /// The episode to resume or play next ("continue watching").
    #[serde(with = "opt_id", skip_serializing_if = "Option::is_none", default)]
    #[schema(value_type = Option<String>)]
    pub episode_id: Option<i64>,
    /// 0 to 100, for the poster card's progress bar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_percent: Option<f32>,
    /// "Because you watched X", "3 friends are watching".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Row {
    /// `new-this-week`, `genre:Drama`, `collection:123`, `continue-watching`, ...
    pub key: String,
    pub kind: RowKind,
    pub display: CollectionDisplay,
    pub title: String,
    pub eyebrow: Option<String>,
    pub subtitle: Option<String>,
    #[serde(with = "opt_id", skip_serializing_if = "Option::is_none", default)]
    #[schema(value_type = Option<String>)]
    pub collection_id: Option<i64>,
    /// Query string for `GET /cinema/titles` listing the same set ("Explore all").
    pub explore: Option<String>,
    pub items: Vec<RowItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Discover {
    /// Tonight's feature carousel (display `hero`).
    pub hero: Row,
    pub rows: Vec<Row>,
}

// ---- facets and search ----------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct FacetCount {
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, ToSchema)]
pub struct Facets {
    pub genres: Vec<FacetCount>,
    pub ratings: Vec<FacetCount>,
    pub languages: Vec<FacetCount>,
    pub subtitle_languages: Vec<FacetCount>,
    pub year_min: Option<i16>,
    pub year_max: Option<i16>,
    pub runtime_max: Option<i32>,
    pub total: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaInstantSearch)]
pub struct InstantSearch {
    pub titles: Vec<Title>,
    /// Matching cast, directors, writers and studios.
    pub people: Vec<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
}

// ---- browse ----------------------------------------------------------------------

/// The value a browse query ordered by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortValue {
    Int(i64),
    Text(String),
}

/// A keyset cursor: the last row of the previous page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub value: SortValue,
    pub id: i64,
}

/// Browse filters (CIN-DATA-003). Empty means unfiltered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseQuery {
    pub kind: Option<TitleKind>,
    /// A title must have every one.
    pub genres: Vec<String>,
    /// A title with any of them is left out.
    pub exclude_genres: Vec<String>,
    pub tags: Vec<String>,
    pub year_from: Option<i16>,
    pub year_to: Option<i16>,
    /// A title with any of these content ratings.
    pub ratings: Vec<String>,
    pub runtime_min: Option<i32>,
    pub runtime_max: Option<i32>,
    pub language: Option<String>,
    pub subtitle_language: Option<String>,
    pub age: AgeFilter,
    pub collection: Option<i64>,
    pub sort: SortKey,
    /// `None` takes the sort's natural direction.
    pub descending: Option<bool>,
    /// Staff only: published or not.
    pub published: Option<bool>,
    /// Staff only: title substring.
    pub title_contains: Option<String>,
    /// Generated rows: content changed at or after this time.
    pub since_ms: Option<i64>,
    /// Generated rows: only titles with a published episode.
    pub episodic_only: bool,
    /// Generated rows: only titles that have a backdrop.
    pub with_backdrop: bool,
    pub limit: u32,
    /// Offset paging, staff lists only. Members use `after`.
    pub offset: u32,
    pub after: Option<Cursor>,
}

impl Default for BrowseQuery {
    fn default() -> Self {
        Self {
            kind: None,
            genres: Vec::new(),
            exclude_genres: Vec::new(),
            tags: Vec::new(),
            year_from: None,
            year_to: None,
            ratings: Vec::new(),
            runtime_min: None,
            runtime_max: None,
            language: None,
            subtitle_language: None,
            age: AgeFilter::Include,
            collection: None,
            sort: SortKey::Updated,
            descending: None,
            published: None,
            title_contains: None,
            since_ms: None,
            episodic_only: false,
            with_backdrop: false,
            limit: 24,
            offset: 0,
            after: None,
        }
    }
}

impl BrowseQuery {
    pub fn is_descending(&self) -> bool {
        self.descending.unwrap_or(self.sort.natural_desc())
    }
}

/// A move of data caused by a merge: `(duplicate episode, episode that took its place)`.
pub type EpisodeMap = Vec<(i64, i64)>;

/// The outcome of merging a title away.
#[derive(Debug, Clone, PartialEq)]
pub struct MergeOutcome {
    pub target: TitleRecord,
    /// Episodes of the source that were dropped as duplicates of target episodes.
    pub episode_map: EpisodeMap,
}

// ---- artwork uploads -------------------------------------------------------------

str_enum!(
    /// What an artwork upload is for; only names the object.
    ArtworkSlot {
        Poster => "poster",
        Backdrop => "backdrop",
        Logo => "logo",
        Still => "still",
        SeasonPoster => "season-poster",
    }
);

str_enum!(
    /// Image formats accepted for artwork.
    ArtworkType {
        Webp => "image/webp",
        Jpeg => "image/jpeg",
        Png => "image/png",
    }
);

impl ArtworkType {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Webp => "webp",
            Self::Jpeg => "jpg",
            Self::Png => "png",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, ToSchema)]
pub struct ArtworkRequest {
    pub slot: ArtworkSlot,
    /// `image/webp`, `image/jpeg` or `image/png`.
    pub content_type: String,
}

/// A reserved artwork object: `PUT` the image to `upload_url` with the same `Content-Type`,
/// then save `key` as `poster_key` / `backdrop_key` / `logo_key` / `still_key` / season `poster_key`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ArtworkUpload {
    pub key: String,
    pub upload_url: String,
    pub content_type: String,
    pub expires_in_secs: u64,
}
