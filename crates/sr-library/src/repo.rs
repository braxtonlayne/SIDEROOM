//! The storage port of the Library. `sr-db` implements it over Postgres.
//!
//! The repository enforces visibility itself: every read takes a [`Scope`], so a
//! caller cannot forget the published / age-restricted filter (LIB-DATA-005).
//! Inputs reaching a write method are already validated by the service.

use async_trait::async_trait;

use crate::error::LibraryResult;
use crate::model::*;

/// A normalised series write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSeries {
    pub id: i64,
    pub input: SeriesInput,
    pub direction: ReadingDirection,
    pub created_by: i64,
}

/// A page of rows with the unpaged total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rows<T> {
    pub items: Vec<T>,
    pub total: i64,
}

/// A feed row before URLs are signed.
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateRecord {
    pub chapter_id: i64,
    pub chapter_number: f64,
    pub chapter_title: Option<String>,
    pub published_at_ms: i64,
    pub release_group: Option<String>,
    pub language: String,
    pub series_id: i64,
    pub series_title: String,
    pub kind: SeriesKind,
    pub age_restricted: bool,
    pub cover_key: Option<String>,
}

/// A chapter's neighbours inside its edition (published chapters only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Neighbours {
    pub prev: Option<i64>,
    pub next: Option<i64>,
}

#[async_trait]
pub trait LibraryRepository:
    crate::reading::ReadingStateRepository + crate::uploads::UploadRepository + Send + Sync
{
    // ---- reads ---------------------------------------------------------
    async fn browse(&self, q: &BrowseQuery, scope: Scope) -> LibraryResult<Rows<SeriesRecord>>;
    async fn get_series(&self, id: i64, scope: Scope) -> LibraryResult<Option<SeriesRecord>>;
    /// Cards for `ids` in the order given; invisible ones are dropped.
    async fn series_by_ids(&self, ids: &[i64], scope: Scope) -> LibraryResult<Vec<SeriesRecord>>;
    async fn list_editions(&self, series_id: i64) -> LibraryResult<Vec<Edition>>;
    async fn list_volumes(&self, series_id: i64) -> LibraryResult<Vec<Volume>>;
    /// Chapters of a series (all editions unless `q.edition` is given), by edition, number and
    /// id (reversed when `q.descending`). `viewer` attaches that user's reading progress. With
    /// `q.limit` at most that many rows after the `after` cursor are returned.
    async fn list_chapters(
        &self,
        series_id: i64,
        q: &ChapterListQuery,
        viewer: Option<i64>,
        scope: Scope,
    ) -> LibraryResult<Vec<ChapterRecord>>;
    /// A chapter with its series facts. Visibility is NOT applied: the service
    /// decides, so staff and members share one lookup.
    async fn get_chapter(
        &self,
        id: i64,
        viewer: Option<i64>,
    ) -> LibraryResult<Option<ChapterRecord>>;
    async fn list_pages(&self, chapter_id: i64) -> LibraryResult<Vec<PageRecord>>;
    async fn neighbours(&self, chapter_id: i64) -> LibraryResult<Neighbours>;
    /// Newest published chapters. `followed_by` limits to that user's follows;
    /// `before` is a `(published_at_ms, chapter_id)` keyset cursor.
    async fn latest_updates(
        &self,
        include_restricted: bool,
        followed_by: Option<i64>,
        before: Option<(i64, i64)>,
        limit: u32,
    ) -> LibraryResult<Vec<UpdateRecord>>;

    // ---- follows -------------------------------------------------------
    async fn get_follow(&self, user: i64, series: i64) -> LibraryResult<Option<Follow>>;
    async fn set_follow(
        &self,
        user: i64,
        series: i64,
        shelf: Shelf,
        notify: bool,
    ) -> LibraryResult<Follow>;
    async fn remove_follow(&self, user: i64, series: i64) -> LibraryResult<bool>;
    async fn list_follows(
        &self,
        user: i64,
        shelf: Option<Shelf>,
        include_restricted: bool,
        limit: u32,
        offset: u32,
    ) -> LibraryResult<Rows<(SeriesRecord, Follow)>>;

    // ---- catalog writes (publishers) ----------------------------------
    async fn create_series(&self, s: &NewSeries) -> LibraryResult<SeriesRecord>;
    async fn update_series(
        &self,
        id: i64,
        input: &SeriesInput,
        direction: ReadingDirection,
    ) -> LibraryResult<Option<SeriesRecord>>;
    /// Publish or unpublish. Publishing stamps `published_at` once.
    async fn set_series_published(
        &self,
        id: i64,
        published: bool,
    ) -> LibraryResult<Option<SeriesRecord>>;
    async fn delete_series(&self, id: i64) -> LibraryResult<bool>;
    /// Move editions, chapters, follows and progress of `source` into `target`
    /// and unpublish `source` (kept as a redirect). Returns the updated target.
    async fn merge_series(&self, source: i64, target: i64) -> LibraryResult<SeriesRecord>;

    async fn create_edition(
        &self,
        id: i64,
        series: i64,
        input: &EditionInput,
    ) -> LibraryResult<Edition>;
    async fn update_edition(&self, id: i64, input: &EditionInput)
    -> LibraryResult<Option<Edition>>;
    async fn get_edition(&self, id: i64) -> LibraryResult<Option<Edition>>;
    async fn delete_edition(&self, id: i64) -> LibraryResult<bool>;

    async fn create_volume(
        &self,
        id: i64,
        edition: i64,
        input: &VolumeInput,
    ) -> LibraryResult<Volume>;
    async fn delete_volume(&self, id: i64) -> LibraryResult<bool>;

    async fn create_chapter(
        &self,
        id: i64,
        edition: i64,
        input: &ChapterInput,
        created_by: i64,
    ) -> LibraryResult<ChapterRecord>;
    async fn update_chapter(
        &self,
        id: i64,
        input: &ChapterInput,
    ) -> LibraryResult<Option<ChapterRecord>>;
    /// Publish or unpublish a chapter and refresh the series' counters.
    async fn set_chapter_published(
        &self,
        id: i64,
        published: bool,
    ) -> LibraryResult<Option<ChapterRecord>>;
    async fn delete_chapter(&self, id: i64) -> LibraryResult<bool>;
    /// Replace the whole page list (reorder, replace and delete are all this).
    async fn replace_pages(
        &self,
        chapter: i64,
        pages: &[PageRecord],
    ) -> LibraryResult<Option<ChapterRecord>>;

    // ---- search index feed --------------------------------------------
    /// Published, non-merged series with `id > after`, ascending, for (re)indexing.
    async fn index_batch(&self, after: i64, limit: u32) -> LibraryResult<Vec<SeriesRecord>>;
}

/// A decoded chapter list request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChapterListQuery {
    pub edition: Option<i64>,
    pub after: Option<ChapterCursor>,
    /// Rows to return; `None` is all.
    pub limit: Option<u32>,
    pub descending: bool,
}
