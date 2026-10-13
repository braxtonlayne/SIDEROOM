//! Member operations on reading state: progress, read marks, history, bookmarks, and the
//! new-chapter fan-out to followers. Types and the storage port are in [`crate::reading`].

use serde_json::{Value, json};

use crate::error::{LibraryError, LibraryResult};
use crate::model::{ChapterFormat, ChapterRecord, Scope};
use crate::reading::*;
use crate::rules;
use crate::service::{LibraryService, Viewer};

/// Followers handled per page of a notification fan-out, and the page cap.
const NOTIFY_PAGE: u32 = 1000;
const NOTIFY_MAX_PAGES: usize = 50;

impl LibraryService {
    pub(crate) async fn emit(&self, user: i64, payload: Value) {
        if let Some(e) = &self.events {
            e.emit(vec![user], payload).await;
        }
    }

    /// A chapter the viewer may open: published, in a published series, and for a restricted
    /// series only after age confirmation (D-184).
    async fn openable_chapter(&self, v: Viewer, chapter: i64) -> LibraryResult<ChapterRecord> {
        let c = self
            .repo
            .get_chapter(chapter, Some(v.user_id))
            .await?
            .filter(|c| c.published && c.series_published && !c.series_merged)
            .ok_or(LibraryError::NotFound("chapter"))?;
        if c.series_age_restricted && !v.adult {
            return Err(LibraryError::AgeConfirmationRequired);
        }
        Ok(c)
    }

    /// The viewer's progress in a chapter.
    pub async fn progress(&self, v: Viewer, chapter: i64) -> LibraryResult<Progress> {
        let c = self.openable_chapter(v, chapter).await?;
        self.repo
            .get_progress(v.user_id, c.id)
            .await?
            .ok_or(LibraryError::NotFound("progress"))
    }

    /// Save the viewer's progress (LIB-RDR-012). See [`ProgressSaved::applied`] for conflicts.
    pub async fn save_progress(
        &self,
        v: Viewer,
        chapter: i64,
        input: ProgressInput,
    ) -> LibraryResult<ProgressSaved> {
        let c = self.openable_chapter(v, chapter).await?;
        let w = progress_write(&input, &c)?;
        let (progress, applied) = self
            .repo
            .put_progress(
                v.user_id,
                c.series_id,
                c.id,
                &w,
                input.force.unwrap_or(false),
            )
            .await?;
        if applied {
            self.emit(v.user_id, progress_event(&progress)).await;
        }
        Ok(ProgressSaved { progress, applied })
    }

    /// Mark a chapter unread. Returns whether there was progress.
    pub async fn clear_progress(&self, v: Viewer, chapter: i64) -> LibraryResult<bool> {
        let c = self.openable_chapter(v, chapter).await?;
        let removed = self.repo.clear_progress(v.user_id, c.id).await?;
        if removed {
            self.emit(
                v.user_id,
                json!({
                    "type": "progress_cleared",
                    "chapter_id": c.id.to_string(),
                    "series_id": c.series_id.to_string(),
                }),
            )
            .await;
        }
        Ok(removed)
    }

    /// Read marks: this chapter, and with `up_to` every earlier chapter of its edition.
    pub async fn mark_read(
        &self,
        v: Viewer,
        chapter: i64,
        input: ReadMarkInput,
    ) -> LibraryResult<ReadMarkResult> {
        let c = self.openable_chapter(v, chapter).await?;
        let up_to = input.up_to.unwrap_or(false);
        let changed = self
            .repo
            .mark_read(v.user_id, &c, input.read, up_to)
            .await?;
        if changed > 0 {
            self.emit(
                v.user_id,
                json!({
                    "type": "read_marks",
                    "series_id": c.series_id.to_string(),
                    "chapter_id": c.id.to_string(),
                    "read": input.read,
                    "up_to": up_to,
                }),
            )
            .await;
        }
        Ok(ReadMarkResult { changed })
    }

    /// Reading history, newest first.
    pub async fn history(
        &self,
        v: Viewer,
        per_series: bool,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> LibraryResult<HistoryPage> {
        let before = before.map(rules::decode_cursor).transpose()?;
        let limit = limit.unwrap_or(24).clamp(1, rules::MAX_LIMIT);
        let mut rows = self
            .repo
            .history(
                v.user_id,
                v.include_restricted(),
                per_series,
                before,
                limit + 1,
            )
            .await?;
        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_before = more.then(|| rows.last()).flatten().and_then(|r| {
            r.chapter
                .progress
                .map(|p| rules::encode_cursor(p.updated_at_ms, r.chapter.id))
        });
        let exp = self.expires_at();
        Ok(HistoryPage {
            items: rows
                .into_iter()
                .map(|r| HistoryItem {
                    chapter: Self::chapter(&r.chapter),
                    series: self.series(r.series, exp),
                })
                .collect(),
            next_before,
        })
    }

    /// The viewer's bookmarks in a chapter.
    pub async fn chapter_bookmarks(&self, v: Viewer, chapter: i64) -> LibraryResult<BookmarkList> {
        let c = self.openable_chapter(v, chapter).await?;
        Ok(BookmarkList {
            bookmarks: self.repo.list_bookmarks(v.user_id, c.id).await?,
        })
    }

    /// Create the bookmark on a page, or change its note (LIB-RDR-013).
    pub async fn save_bookmark(
        &self,
        v: Viewer,
        chapter: i64,
        input: BookmarkInput,
    ) -> LibraryResult<Bookmark> {
        let c = self.openable_chapter(v, chapter).await?;
        let in_range = input.page_index >= 0
            && !(c.format == ChapterFormat::Images
                && c.page_count > 0
                && input.page_index >= c.page_count);
        if !in_range {
            return Err(LibraryError::Invalid("page_index is out of range".into()));
        }
        let note = bookmark_note(input.note.as_deref())?;
        let id = (self.ids)();
        match self
            .repo
            .put_bookmark(id, v.user_id, &c, input.page_index, &note)
            .await?
        {
            (BookmarkOutcome::Saved, Some(b)) => {
                self.emit(v.user_id, bookmark_event("bookmark_set", &b))
                    .await;
                Ok(b)
            }
            _ => Err(LibraryError::Conflict(format!(
                "a chapter holds at most {MAX_BOOKMARKS_PER_CHAPTER} bookmarks"
            ))),
        }
    }

    /// Remove one of the viewer's bookmarks. Returns whether it existed.
    pub async fn delete_bookmark(&self, v: Viewer, id: i64) -> LibraryResult<bool> {
        let removed = self.repo.delete_bookmark(v.user_id, id).await?;
        if let Some(b) = &removed {
            self.emit(v.user_id, bookmark_event("bookmark_removed", b))
                .await;
        }
        Ok(removed.is_some())
    }

    /// All of the viewer's bookmarks, newest first.
    pub async fn my_bookmarks(
        &self,
        v: Viewer,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> LibraryResult<BookmarkPage> {
        let before = before.map(rules::decode_cursor).transpose()?;
        let limit = limit.unwrap_or(24).clamp(1, rules::MAX_LIMIT);
        let mut rows = self
            .repo
            .all_bookmarks(v.user_id, v.include_restricted(), before, limit + 1)
            .await?;
        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_before = more
            .then(|| rows.last())
            .flatten()
            .map(|r| rules::encode_cursor(r.bookmark.created_at_ms, r.bookmark.id));
        Ok(BookmarkPage {
            items: rows
                .into_iter()
                .map(|r| BookmarkEntry {
                    bookmark: r.bookmark,
                    series_title: r.series_title,
                    chapter_number: r.chapter_number,
                    chapter_title: r.chapter_title,
                })
                .collect(),
            next_before,
        })
    }

    /// Tell every follower with the bell on that `chapter` was published (LIB-SOC-007 hook).
    /// Pages through the followers, capped so a huge series cannot hold the task forever.
    /// Returns how many followers were addressed.
    pub async fn notify_followers(&self, chapter: &ChapterRecord) -> LibraryResult<u64> {
        let Some(events) = &self.events else {
            return Ok(0);
        };
        let series = self
            .repo
            .get_series(chapter.series_id, Scope::Staff)
            .await?
            .ok_or(LibraryError::NotFound("series"))?;
        let payload = json!({
            "type": "chapter_published",
            "series_id": series.id.to_string(),
            "series_title": series.title,
            "chapter_id": chapter.id.to_string(),
            "chapter_number": chapter.number,
            "chapter_title": chapter.title,
            "published_at_ms": chapter.published_at_ms,
        });
        let (mut after, mut sent) = (0_i64, 0_u64);
        for _ in 0..NOTIFY_MAX_PAGES {
            let users = self
                .repo
                .followers_to_notify(series.id, after, NOTIFY_PAGE)
                .await?;
            let Some(last) = users.last().copied() else {
                break;
            };
            let full = users.len() as u32 == NOTIFY_PAGE;
            sent += users.len() as u64;
            events.emit(users, payload.clone()).await;
            after = last;
            if !full {
                break;
            }
        }
        Ok(sent)
    }
}
