//! Reading state of the signed-in member: progress, read marks, history and bookmarks
//! (LIB-RDR-012, LIB-RDR-013), plus the events other devices receive (`SR_LIBRARY_UPDATE`).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

use crate::error::{LibraryError, LibraryResult};
use crate::model::{Chapter, ChapterRecord, Series, SeriesRecord, id};

pub const MAX_NOTE: usize = 1000;
pub const MAX_CFI: usize = 1024;
/// Bookmarks one account may keep in one chapter.
pub const MAX_BOOKMARKS_PER_CHAPTER: i64 = 200;
/// Gateway event name for Library state changes.
pub const EVENT_NAME: &str = "SR_LIBRARY_UPDATE";

/// Body of `PUT /library/chapters/{id}/progress`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ProgressInput {
    /// Page index (images/PDF) or spine item index (EPUB), from 0.
    pub page_index: i32,
    /// 0 to 100.
    pub percent: f32,
    /// EPUB position.
    pub cfi: Option<String>,
    /// Defaults to `percent >= 100`.
    pub completed: Option<bool>,
    /// `updated_at_ms` of the progress this client last saw (`0` for none). When the stored
    /// progress is newer the write is not applied and the stored one is returned.
    pub base_updated_at_ms: Option<i64>,
    /// Write even if the stored progress is newer (the member chose their own position).
    pub force: Option<bool>,
}

/// A validated progress write.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgressWrite {
    pub page_index: i32,
    pub percent: f32,
    pub cfi: Option<String>,
    pub completed: bool,
    /// Conflict guard; `None` writes unconditionally.
    pub base_updated_at_ms: Option<i64>,
}

/// A member's progress in one chapter.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Progress {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub chapter_id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub series_id: i64,
    pub page_index: i32,
    pub percent: f32,
    pub cfi: Option<String>,
    pub completed: bool,
    pub updated_at_ms: i64,
}

/// Answer of a progress save.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ProgressSaved {
    /// What is stored now.
    pub progress: Progress,
    /// `false`: the stored progress was newer than `base_updated_at_ms` and was left alone, so
    /// the client should ask "continue where?" and may repeat the save with `force`.
    pub applied: bool,
}

/// Body of `POST /library/chapters/{id}/read`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ReadMarkInput {
    /// `true` marks read, `false` unread.
    pub read: bool,
    /// Also mark every earlier chapter of the same edition.
    pub up_to: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ReadMarkResult {
    /// Chapters whose mark changed.
    pub changed: u64,
}

/// One row of the reading history.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct HistoryItem {
    pub series: Series,
    /// The chapter, with `progress` set.
    pub chapter: Chapter,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct HistoryPage {
    pub items: Vec<HistoryItem>,
    /// Pass back as `before` for the next page; absent at the end.
    pub next_before: Option<String>,
}

/// A history row before URLs are signed.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryRecord {
    pub series: SeriesRecord,
    /// `progress` is set.
    pub chapter: ChapterRecord,
}

/// Body of `PUT /library/chapters/{id}/bookmarks`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct BookmarkInput {
    pub page_index: i32,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[schema(as = ReadingBookmark)]
pub struct Bookmark {
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub chapter_id: i64,
    #[serde(with = "id")]
    #[schema(value_type = String)]
    pub series_id: i64,
    pub page_index: i32,
    pub note: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct BookmarkList {
    pub bookmarks: Vec<Bookmark>,
}

/// A bookmark with the titles the "my bookmarks" list shows.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
#[schema(as = ReadingBookmarkEntry)]
pub struct BookmarkEntry {
    pub bookmark: Bookmark,
    pub series_title: String,
    pub chapter_number: f64,
    pub chapter_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct BookmarkPage {
    pub items: Vec<BookmarkEntry>,
    /// Pass back as `before` for the next page; absent at the end.
    pub next_before: Option<String>,
}

/// A bookmark row with its list context.
#[derive(Debug, Clone, PartialEq)]
pub struct BookmarkRecord {
    pub bookmark: Bookmark,
    pub series_title: String,
    pub chapter_number: f64,
    pub chapter_title: Option<String>,
}

/// Why a bookmark could not be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookmarkOutcome {
    Saved,
    /// The chapter already holds [`MAX_BOOKMARKS_PER_CHAPTER`] bookmarks.
    LimitReached,
}

/// Storage port for reading state. `sr-db` implements it.
#[async_trait]
pub trait ReadingStateRepository: Send + Sync {
    async fn get_progress(&self, user: i64, chapter: i64) -> LibraryResult<Option<Progress>>;
    /// Save progress for `chapter` (of `series`). With a `base_updated_at_ms` and no `force`,
    /// a newer stored row is kept and returned with `applied = false`.
    async fn put_progress(
        &self,
        user: i64,
        series: i64,
        chapter: i64,
        w: &ProgressWrite,
        force: bool,
    ) -> LibraryResult<(Progress, bool)>;
    async fn clear_progress(&self, user: i64, chapter: i64) -> LibraryResult<bool>;
    /// Mark `chapter` (and with `up_to` every published earlier chapter of its edition) read or
    /// unread. Returns the number of chapters whose mark changed.
    async fn mark_read(
        &self,
        user: i64,
        chapter: &ChapterRecord,
        read: bool,
        up_to: bool,
    ) -> LibraryResult<u64>;
    /// Reading history over published, non-merged series, newest first. `per_series` keeps the
    /// latest chapter of each series. `before` is a `(updated_at_ms, chapter_id)` keyset cursor.
    async fn history(
        &self,
        user: i64,
        include_restricted: bool,
        per_series: bool,
        before: Option<(i64, i64)>,
        limit: u32,
    ) -> LibraryResult<Vec<HistoryRecord>>;

    async fn list_bookmarks(&self, user: i64, chapter: i64) -> LibraryResult<Vec<Bookmark>>;
    /// Create the bookmark on the page, or update its note.
    async fn put_bookmark(
        &self,
        id: i64,
        user: i64,
        chapter: &ChapterRecord,
        page_index: i32,
        note: &str,
    ) -> LibraryResult<(BookmarkOutcome, Option<Bookmark>)>;
    /// Delete and return the member's bookmark.
    async fn delete_bookmark(&self, user: i64, id: i64) -> LibraryResult<Option<Bookmark>>;
    /// The member's bookmarks, newest first, over published, non-merged series.
    /// `before` is a `(created_at_ms, bookmark_id)` keyset cursor.
    async fn all_bookmarks(
        &self,
        user: i64,
        include_restricted: bool,
        before: Option<(i64, i64)>,
        limit: u32,
    ) -> LibraryResult<Vec<BookmarkRecord>>;
    /// Users following `series` with the bell on, with `id > after`, ascending.
    async fn followers_to_notify(
        &self,
        series: i64,
        after: i64,
        limit: u32,
    ) -> LibraryResult<Vec<i64>>;
}

/// Delivers `SR_LIBRARY_UPDATE` to member sessions (implemented over the event bus by `sr-api`).
#[async_trait]
pub trait LibraryEvents: Send + Sync {
    async fn emit(&self, users: Vec<i64>, payload: Value);

    /// SR_ACTIVITY_UPDATE (reading presence, LIB-RDR-017) to the given users. Sinks that do not
    /// carry presence keep the default, which drops it.
    async fn emit_activity(&self, users: Vec<i64>, payload: Value) {
        let _ = (users, payload);
    }
}

/// Validate a progress save against the chapter it belongs to.
pub fn progress_write(input: &ProgressInput, c: &ChapterRecord) -> LibraryResult<ProgressWrite> {
    if input.page_index < 0 || input.page_index > 1_000_000 {
        return Err(LibraryError::Invalid("page_index is out of range".into()));
    }
    if c.format == crate::model::ChapterFormat::Images
        && c.page_count > 0
        && input.page_index >= c.page_count
    {
        return Err(LibraryError::Invalid(format!(
            "page_index must be below the chapter's {} pages",
            c.page_count
        )));
    }
    if !input.percent.is_finite() || !(0.0..=100.0).contains(&input.percent) {
        return Err(LibraryError::Invalid(
            "percent must be between 0 and 100".into(),
        ));
    }
    let cfi = match input
        .cfi
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(s) if s.chars().count() > MAX_CFI || s.chars().any(char::is_control) => {
            return Err(LibraryError::Invalid("cfi is not valid".into()));
        }
        Some(s) => Some(s.to_owned()),
        None => None,
    };
    if input.base_updated_at_ms.is_some_and(|b| b < 0) {
        return Err(LibraryError::Invalid(
            "base_updated_at_ms must not be negative".into(),
        ));
    }
    Ok(ProgressWrite {
        page_index: input.page_index,
        percent: input.percent,
        cfi,
        completed: input.completed.unwrap_or(input.percent >= 100.0),
        base_updated_at_ms: input.base_updated_at_ms,
    })
}

/// Validate a bookmark note: trimmed, no control characters except newline.
pub fn bookmark_note(note: Option<&str>) -> LibraryResult<String> {
    let n = note.unwrap_or("").trim();
    if n.chars().count() > MAX_NOTE {
        return Err(LibraryError::Invalid(format!(
            "note must be at most {MAX_NOTE} characters"
        )));
    }
    if n.chars().any(|c| c.is_control() && c != '\n') {
        return Err(LibraryError::Invalid(
            "note contains control characters".into(),
        ));
    }
    Ok(n.to_owned())
}

// ---- event payloads ----------------------------------------------------

pub fn progress_event(p: &Progress) -> Value {
    json!({
        "type": "progress",
        "chapter_id": p.chapter_id.to_string(),
        "series_id": p.series_id.to_string(),
        "page_index": p.page_index,
        "percent": p.percent,
        "cfi": p.cfi,
        "completed": p.completed,
        "updated_at_ms": p.updated_at_ms,
    })
}

pub fn bookmark_event(kind: &str, b: &Bookmark) -> Value {
    json!({
        "type": kind,
        "id": b.id.to_string(),
        "chapter_id": b.chapter_id.to_string(),
        "series_id": b.series_id.to_string(),
        "page_index": b.page_index,
        "note": b.note,
        "created_at_ms": b.created_at_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ChapterFormat, ReadingDirection};

    fn chapter(pages: i32, format: ChapterFormat) -> ChapterRecord {
        ChapterRecord {
            id: 5,
            mount_id: 1,
            series_id: 4,
            edition_id: 3,
            volume_id: None,
            number: 1.0,
            title: None,
            page_count: pages,
            format,
            source_key: None,
            release_group: None,
            published: true,
            published_at_ms: Some(1),
            series_published: true,
            series_age_restricted: false,
            series_direction: ReadingDirection::Rtl,
            series_merged: false,
            progress: None,
        }
    }

    fn input(page: i32, percent: f32) -> ProgressInput {
        ProgressInput {
            page_index: page,
            percent,
            cfi: None,
            completed: None,
            base_updated_at_ms: None,
            force: None,
        }
    }

    #[test]
    fn progress_is_checked_against_the_chapter() {
        let c = chapter(10, ChapterFormat::Images);
        assert!(progress_write(&input(9, 90.0), &c).is_ok());
        assert!(progress_write(&input(10, 90.0), &c).is_err());
        assert!(progress_write(&input(-1, 0.0), &c).is_err());
        assert!(progress_write(&input(0, 100.5), &c).is_err());
        assert!(progress_write(&input(0, f32::NAN), &c).is_err());
        // EPUB positions are not page numbers.
        let e = chapter(0, ChapterFormat::Epub);
        assert!(progress_write(&input(42, 10.0), &e).is_ok());
    }

    #[test]
    fn completed_defaults_to_full_percent_and_cfi_is_trimmed() {
        let c = chapter(10, ChapterFormat::Images);
        assert!(progress_write(&input(9, 100.0), &c).unwrap().completed);
        assert!(!progress_write(&input(9, 99.0), &c).unwrap().completed);
        let mut i = input(0, 1.0);
        i.completed = Some(true);
        assert!(progress_write(&i, &c).unwrap().completed);
        i.cfi = Some("  epubcfi(/6/4!/4/2)  ".into());
        assert_eq!(
            progress_write(&i, &c).unwrap().cfi.as_deref(),
            Some("epubcfi(/6/4!/4/2)")
        );
        i.cfi = Some("x".repeat(MAX_CFI + 1));
        assert!(progress_write(&i, &c).is_err());
        i.cfi = Some("   ".into());
        assert_eq!(progress_write(&i, &c).unwrap().cfi, None);
        i.base_updated_at_ms = Some(-5);
        assert!(progress_write(&i, &c).is_err());
    }

    #[test]
    fn notes_are_bounded_and_clean() {
        assert_eq!(bookmark_note(None).unwrap(), "");
        assert_eq!(bookmark_note(Some("  hi  ")).unwrap(), "hi");
        assert!(bookmark_note(Some(&"x".repeat(MAX_NOTE + 1))).is_err());
        assert!(bookmark_note(Some("a\u{0}b")).is_err());
        assert_eq!(bookmark_note(Some("a\nb")).unwrap(), "a\nb");
    }

    #[test]
    fn events_carry_string_ids() {
        let p = Progress {
            chapter_id: 5,
            series_id: 4,
            page_index: 3,
            percent: 40.0,
            cfi: None,
            completed: false,
            updated_at_ms: 9,
        };
        let v = progress_event(&p);
        assert_eq!(v["type"], "progress");
        assert_eq!(v["chapter_id"], "5");
        assert_eq!(v["updated_at_ms"], 9);
    }
}
