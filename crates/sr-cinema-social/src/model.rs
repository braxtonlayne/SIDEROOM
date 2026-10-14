//! Wire types of the Cinema social API (docs/specs/cinema-media.md section 8).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::catalog::{EpisodeRef, TitleKind, TitleRef};
use crate::ids::{id, id_vec, opt_id};

// ---- progress ------------------------------------------------------------

/// Body of `PUT /cinema/titles/{id}[/episodes/{eid}]/progress`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = CinemaProgressInput)]
pub struct ProgressInput {
    /// Where the player is, in seconds.
    pub position_secs: i32,
    /// Length of the media, in seconds.
    pub duration_secs: i32,
    /// Defaults to `position >= 90%` and is sticky once set (a rewatch stays watched).
    pub completed: Option<bool>,
    /// `false` for a pause or stop: saves the position and ends the "watching" presence. Default true.
    pub playing: Option<bool>,
    /// `updated_at_ms` of the progress this client last saw (`0` for none). When the stored progress
    /// is newer the write is not applied and the stored one is returned.
    pub base_updated_at_ms: Option<i64>,
    /// Write even if the stored progress is newer.
    pub force: Option<bool>,
}

/// A validated progress write.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgressWrite {
    pub position_secs: i32,
    pub duration_secs: i32,
    pub percent: f32,
    pub completed: Option<bool>,
    pub playing: bool,
    pub base_updated_at_ms: Option<i64>,
    pub force: bool,
}

/// A member's progress in one movie or episode.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaProgress)]
pub struct Progress {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub title_id: i64,
    /// `"0"` for a movie.
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub episode_id: i64,
    pub position_secs: i32,
    pub duration_secs: i32,
    pub percent: f32,
    pub completed: bool,
    pub updated_at_ms: i64,
}

/// Answer of a progress save.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaProgressSaved)]
pub struct ProgressSaved {
    /// What is stored now (or, when paused, what was sent).
    pub progress: Progress,
    /// `false`: the stored progress was newer than `base_updated_at_ms` and was left alone, so the
    /// client should ask "continue where?" and may repeat the save with `force`.
    pub applied: bool,
    /// `false` when history is paused: the heartbeat was accepted but not stored.
    pub recorded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaWatchState)]
pub struct WatchState {
    pub items: Vec<Progress>,
}

/// Body of `POST /cinema/titles/{id}/watched`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = CinemaWatchedInput)]
pub struct WatchedInput {
    pub watched: bool,
    /// One episode.
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub episode_id: Option<i64>,
    /// A whole season.
    pub season: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[schema(as = CinemaWatchedResult)]
pub struct WatchedResult {
    /// Items whose mark changed.
    pub changed: u64,
}

// ---- history and continue watching ----------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaHistoryItem)]
pub struct HistoryItem {
    pub title: TitleRef,
    pub episode: Option<EpisodeRef>,
    pub progress: Progress,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaHistoryPage)]
pub struct HistoryPage {
    pub items: Vec<HistoryItem>,
    /// Pass back as `before` for the next page; absent at the end.
    pub next_before: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = CinemaHistorySettings)]
pub struct HistorySettings {
    pub paused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = CinemaContinueReason)]
pub enum ContinueReason {
    /// An unfinished item with its progress.
    Resume,
    /// A finished episode whose next episode is `episode`.
    UpNext,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaContinueItem)]
pub struct ContinueItem {
    pub reason: ContinueReason,
    pub title: TitleRef,
    pub episode: Option<EpisodeRef>,
    pub progress: Option<Progress>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaContinueWatching)]
pub struct ContinueWatching {
    pub items: Vec<ContinueItem>,
}

// ---- presence -------------------------------------------------------------

/// One friend who is watching something now.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaFriendWatching)]
pub struct FriendWatching {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub user_id: i64,
    pub title: TitleRef,
    pub episode: Option<EpisodeRef>,
    pub started_at_ms: i64,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaFriendsWatching)]
pub struct FriendsWatching {
    pub items: Vec<FriendWatching>,
}

// ---- lists ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = CinemaVisibility)]
pub enum Visibility {
    Private,
    /// Shared with friends.
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
        Some(match s {
            "private" => Self::Private,
            "friends" => Self::Friends,
            "public" => Self::Public,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = CinemaListKind)]
pub enum ListKind {
    /// "My list": one per account.
    Saved,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct CinemaList {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub owner_id: i64,
    pub name: String,
    pub description: String,
    pub kind: ListKind,
    pub visibility: Visibility,
    pub item_count: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaListItem)]
pub struct ListItem {
    pub title: TitleRef,
    pub note: String,
    pub added_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaListItemsPage)]
pub struct ListItemsPage {
    pub items: Vec<ListItem>,
    pub next_before: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaListDetail)]
pub struct ListDetail {
    pub list: CinemaList,
    pub items: Vec<ListItem>,
    pub next_before: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaListsResponse)]
pub struct ListsResponse {
    pub lists: Vec<CinemaList>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = CinemaListInput)]
pub struct ListInput {
    pub name: String,
    pub description: Option<String>,
    pub visibility: Option<Visibility>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = CinemaListPatch)]
pub struct ListPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub visibility: Option<Visibility>,
}

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[schema(as = CinemaListItemInput)]
pub struct ListItemInput {
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = CinemaListOrderInput)]
pub struct ListOrderInput {
    #[serde(with = "id_vec")]
    #[schema(value_type = Vec<String>)]
    pub title_ids: Vec<i64>,
}

// ---- ratings, reviews -------------------------------------------------------

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[schema(as = CinemaRatingInput)]
pub struct RatingInput {
    /// 1 to 10.
    pub score: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaRatingAggregate)]
pub struct RatingAggregate {
    pub average: Option<f64>,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaRatings)]
pub struct Ratings {
    /// The caller's own score.
    pub mine: Option<i32>,
    pub global: RatingAggregate,
    pub friends: RatingAggregate,
    /// People who share a server with the caller.
    pub known: RatingAggregate,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = CinemaReviewInput)]
pub struct ReviewInput {
    pub body: String,
    pub spoiler: Option<bool>,
    /// 1 to 10; also sets the caller's rating of the title.
    pub rating: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaReview)]
pub struct Review {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub title_id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub author_id: i64,
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
#[schema(as = CinemaReviewPage)]
pub struct ReviewPage {
    pub items: Vec<Review>,
    pub next_before: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = CinemaReviewSort)]
pub enum ReviewSort {
    Helpful,
    Recent,
}

// ---- comments ---------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = CinemaCommentInput)]
pub struct CommentInput {
    pub body: String,
    pub spoiler: Option<bool>,
    /// Reply to this top-level comment.
    #[serde(default, with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub parent_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = CinemaCommentPatch)]
pub struct CommentPatch {
    pub body: Option<String>,
    pub spoiler: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaComment)]
pub struct Comment {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub title_id: i64,
    /// `"0"` for a comment on the title itself.
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub episode_id: i64,
    #[serde(with = "opt_id")]
    #[schema(value_type = Option<String>)]
    pub parent_id: Option<i64>,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub author_id: i64,
    pub body: String,
    pub spoiler: bool,
    pub like_count: i32,
    pub reply_count: i64,
    pub liked: bool,
    pub mine: bool,
    pub created_at_ms: i64,
    pub edited_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaCommentPage)]
pub struct CommentPage {
    pub items: Vec<Comment>,
    pub next_before: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[schema(as = CinemaReportInput)]
pub struct ReportInput {
    pub reason: String,
}

// ---- discovery ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = CinemaTrendScope)]
pub enum TrendScope {
    Global,
    Friends,
    Known,
}

impl TrendScope {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "global" => Self::Global,
            "friends" => Self::Friends,
            "known" => Self::Known,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaTrendingItem)]
pub struct TrendingItem {
    pub title: TitleRef,
    pub score: f64,
    /// Distinct viewers in the last 7 days within the scope.
    pub viewers: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaTrending)]
pub struct Trending {
    pub scope: TrendScope,
    pub items: Vec<TrendingItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaRecommendation)]
pub struct Recommendation {
    pub title: TitleRef,
    pub score: f64,
    /// `cowatch`, `tags`.
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaRecommendationRow)]
pub struct RecommendationRow {
    /// "Because you watched ...".
    pub because: TitleRef,
    pub items: Vec<Recommendation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = CinemaRecommendations)]
pub struct Recommendations {
    pub rows: Vec<RecommendationRow>,
}

/// Who is asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewer {
    pub user_id: i64,
    /// The account confirmed it is 18 or older.
    pub adult: bool,
}

/// Filter of the trending kind.
pub type KindFilter = Option<TitleKind>;
