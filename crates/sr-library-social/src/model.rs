//! Wire types of the Library social API. Ids are decimal strings, times Unix milliseconds.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::ids::id;

/// The caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewer {
    pub user_id: i64,
    /// The account confirmed it is 18 or older.
    pub adult: bool,
    /// `library.hide_age_restricted`: leave restricted series out of rankings.
    pub hide_restricted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, ToSchema)]
#[schema(as = LibraryRatingAggregate)]
pub struct RatingAggregate {
    /// One decimal; absent without ratings.
    pub average: Option<f64>,
    pub count: i64,
}

/// A series' ratings per scope (D-089): everyone, friends, and "people you know" (friends plus
/// people sharing a server), the caller's own score, and the global distribution.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct SeriesRatings {
    pub mine: Option<i32>,
    pub global: RatingAggregate,
    pub friends: RatingAggregate,
    pub known: RatingAggregate,
    /// Counts of scores 1 to 10 across everyone (index 0 is score 1).
    pub distribution: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ChapterRating {
    pub mine: Option<i32>,
    pub average: Option<f64>,
    pub count: i64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ScoreInput {
    /// 1 to 10.
    pub score: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = LibraryReview)]
pub struct Review {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub series_id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub author_id: i64,
    pub author_name: String,
    /// The author's 1 to 10 score for the series, when they rated it.
    pub rating: Option<i32>,
    pub body: String,
    pub spoiler: bool,
    pub helpful_count: i32,
    /// The caller marked it helpful.
    pub voted: bool,
    pub mine: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = LibraryReviewPage)]
pub struct ReviewPage {
    pub items: Vec<Review>,
    pub next_before: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = LibraryReviewInput)]
pub struct ReviewInput {
    pub body: String,
    /// Also sets the caller's rating of the series.
    pub rating: Option<i32>,
    pub spoiler: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
#[schema(as = LibraryReviewSort)]
pub enum ReviewSort {
    #[default]
    Helpful,
    Recent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = LibraryVisibility)]
pub enum Visibility {
    Private,
    Friends,
    Public,
}

impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Friends => "friends",
            Self::Public => "public",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "private" => Some(Self::Private),
            "friends" => Some(Self::Friends),
            "public" => Some(Self::Public),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ReadingList {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub owner_id: i64,
    pub owner_name: String,
    /// The caller owns the list.
    pub mine: bool,
    pub name: String,
    pub description: String,
    pub visibility: Visibility,
    pub featured: bool,
    pub item_count: i64,
    /// The first series of the list, for the fanned cover stack (at most four).
    #[serde(with = "crate::ids::id_vec")]
    #[schema(value_type = Vec<String>)]
    pub preview: Vec<i64>,
    /// With `?series=` on the member's own lists: whether that series is on this list.
    pub contains: Option<bool>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ListEntry {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub series_id: i64,
    pub position: i64,
    pub note: String,
    pub added_at_ms: i64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = LibraryListInput)]
pub struct ListInput {
    pub name: String,
    pub description: Option<String>,
    pub visibility: Option<Visibility>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = LibraryListPatch)]
pub struct ListPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub visibility: Option<Visibility>,
}

#[derive(Debug, Clone, Deserialize, ToSchema, Default)]
#[schema(as = LibraryListItemInput)]
pub struct ListItemInput {
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = LibraryListOrderInput)]
pub struct ListOrderInput {
    #[serde(with = "crate::ids::id_vec")]
    #[schema(value_type = Vec<String>)]
    pub series_ids: Vec<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum RankingPeriod {
    #[default]
    Week,
    Month,
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct RankingItem {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub series_id: i64,
    /// 1-based.
    pub rank: u32,
    pub score: f64,
    /// Distinct readers in the period (followers for `all`).
    pub readers: i64,
    pub rating_average: Option<f64>,
    pub rating_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Rankings {
    pub period: RankingPeriod,
    pub items: Vec<RankingItem>,
}

/// Unread published chapters of one followed series.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct UnreadCount {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub series_id: i64,
    pub unread: i64,
    /// Last time the member read something in this series.
    pub last_read_at_ms: Option<i64>,
    /// Number of the highest chapter published, for "sort by progress".
    pub chapter_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct UnreadCounts {
    pub items: Vec<UnreadCount>,
}
