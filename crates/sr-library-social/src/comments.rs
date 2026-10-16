//! Comments on a series or a chapter (LIB-SOC-003, LIB-RDR-014): one reply level, spoiler flag,
//! likes and reports. Authors the viewer blocked or ignored are left out of listings.

use sqlx::{Row, postgres::PgRow};

use crate::error::{SocialError, SocialResult};
use crate::model::*;
use crate::rules;
use crate::service::LibrarySocial;

const NOT_BLOCKED: &str = "NOT EXISTS (SELECT 1 FROM relationships bl WHERE bl.user_id = $1 \
     AND bl.target_id = c.user_id AND bl.type IN (2, 6))";

const COLS: &str = "c.id, c.series_id, c.chapter_id, c.parent_id, c.user_id, \
     COALESCE(a.display_name, a.username::text) AS author_name, c.body, c.spoiler, c.like_count, \
     (SELECT count(*) FROM reading_comments rc WHERE rc.parent_id = c.id AND NOT rc.hidden) AS reply_count, \
     EXISTS (SELECT 1 FROM reading_comment_likes l WHERE l.comment_id = c.id AND l.user_id = $1) AS liked, \
     (extract(epoch FROM c.created_at) * 1000)::bigint AS created_ms, \
     (extract(epoch FROM c.edited_at) * 1000)::bigint AS edited_ms";

const FROM: &str = "reading_comments c JOIN accounts a ON a.id = c.user_id";

fn comment_from(r: &PgRow, viewer: i64) -> SocialResult<Comment> {
    let author: i64 = r.try_get("user_id")?;
    Ok(Comment {
        id: r.try_get("id")?,
        series_id: r.try_get("series_id")?,
        chapter_id: r.try_get("chapter_id")?,
        parent_id: r.try_get("parent_id")?,
        author_id: author,
        author_name: r.try_get("author_name")?,
        body: r.try_get("body")?,
        spoiler: r.try_get("spoiler")?,
        like_count: r.try_get("like_count")?,
        reply_count: r.try_get("reply_count")?,
        liked: r.try_get("liked")?,
        mine: author == viewer,
        created_at_ms: r.try_get("created_ms")?,
        edited_at_ms: r.try_get("edited_ms")?,
    })
}

fn page(rows: &[PgRow], viewer: i64, limit: u32, total: i64) -> SocialResult<CommentPage> {
    let more = rows.len() > limit as usize;
    let mut items: Vec<Comment> = rows
        .iter()
        .map(|r| comment_from(r, viewer))
        .collect::<Result<_, _>>()?;
    items.truncate(limit as usize);
    let next_before = more
        .then(|| items.last())
        .flatten()
        .map(|c| rules::encode_cursor(&[c.created_at_ms, c.id]));
    Ok(CommentPage {
        items,
        next_before,
        total,
    })
}

impl LibrarySocial {
    /// The series a chapter belongs to, when the chapter and series are published.
    async fn chapter_series(&self, chapter: i64) -> SocialResult<i64> {
        sqlx::query_scalar(
            "SELECT c.series_id FROM reading_chapters c JOIN reading_series s ON s.id = c.series_id \
             WHERE c.id = $1 AND c.published AND s.published AND s.merged_into IS NULL",
        )
        .bind(chapter)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(SocialError::NotFound("chapter"))
    }

    /// Post a comment on a series (`chapter` 0) or chapter, or reply to a top-level comment.
    pub async fn post_comment(
        &self,
        v: Viewer,
        series: i64,
        chapter: i64,
        input: CommentInput,
    ) -> SocialResult<Comment> {
        let series = if chapter == 0 {
            series
        } else {
            self.chapter_series(chapter).await?
        };
        self.actionable_series(v, series).await?;
        let body = rules::clean_text(&input.body, rules::MAX_COMMENT, "body", false)?;
        if let Some(parent) = input.parent_id {
            let ok: Option<bool> = sqlx::query_scalar(
                "SELECT parent_id IS NULL FROM reading_comments \
                 WHERE id = $1 AND series_id = $2 AND chapter_id = $3 AND NOT hidden",
            )
            .bind(parent)
            .bind(series)
            .bind(chapter)
            .fetch_optional(&self.pool)
            .await?;
            match ok {
                None => return Err(SocialError::NotFound("comment")),
                Some(false) => {
                    return Err(SocialError::Invalid(
                        "replies are one level deep: reply to the top-level comment".into(),
                    ));
                }
                Some(true) => {}
            }
        }
        let id = (self.ids)();
        sqlx::query(
            "INSERT INTO reading_comments (id, user_id, series_id, chapter_id, parent_id, body, spoiler) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(id)
        .bind(v.user_id)
        .bind(series)
        .bind(chapter)
        .bind(input.parent_id)
        .bind(&body)
        .bind(input.spoiler.unwrap_or(false))
        .execute(&self.pool)
        .await?;
        self.comment_by_id(v.user_id, id).await
    }

    pub(crate) async fn comment_by_id(&self, viewer: i64, id: i64) -> SocialResult<Comment> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {COLS} FROM {FROM} WHERE c.id = $2"
        )))
        .bind(viewer)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(SocialError::NotFound("comment"))?;
        comment_from(&row, viewer)
    }

    /// Top-level comments of a series (`chapter` 0) or chapter, newest first.
    pub async fn comments(
        &self,
        v: Viewer,
        series: i64,
        chapter: i64,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> SocialResult<CommentPage> {
        let series = if chapter == 0 {
            self.visible_series(series).await?;
            series
        } else {
            self.chapter_series(chapter).await?
        };
        let before = before.map(|b| rules::decode_cursor(b, 2)).transpose()?;
        let limit = rules::limit(limit);
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {COLS} FROM {FROM} WHERE c.series_id = $2 AND c.chapter_id = $3 \
               AND c.parent_id IS NULL AND NOT c.hidden AND {NOT_BLOCKED} \
               AND ($4::bigint IS NULL OR ((extract(epoch FROM c.created_at) * 1000)::bigint, c.id) < ($4, $5)) \
             ORDER BY created_ms DESC, c.id DESC LIMIT $6"
        )))
        .bind(v.user_id)
        .bind(series)
        .bind(chapter)
        .bind(before.as_ref().map(|b| b[0]))
        .bind(before.as_ref().map_or(0, |b| b[1]))
        .bind(i64::from(limit) + 1)
        .fetch_all(&self.pool)
        .await?;
        let total: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM reading_comments \
             WHERE series_id = $1 AND chapter_id = $2 AND NOT hidden",
        )
        .bind(series)
        .bind(chapter)
        .fetch_one(&self.pool)
        .await?;
        page(&rows, v.user_id, limit, total)
    }

    /// Replies to a top-level comment, oldest first.
    pub async fn comment_replies(
        &self,
        v: Viewer,
        comment: i64,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> SocialResult<CommentPage> {
        let parent = self.visible_comment(v.user_id, comment).await?;
        let before = before.map(|b| rules::decode_cursor(b, 2)).transpose()?;
        let limit = rules::limit(limit);
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {COLS} FROM {FROM} WHERE c.parent_id = $2 AND NOT c.hidden AND {NOT_BLOCKED} \
               AND ($3::bigint IS NULL OR ((extract(epoch FROM c.created_at) * 1000)::bigint, c.id) > ($3, $4)) \
             ORDER BY created_ms ASC, c.id ASC LIMIT $5"
        )))
        .bind(v.user_id)
        .bind(parent.id)
        .bind(before.as_ref().map(|b| b[0]))
        .bind(before.as_ref().map_or(0, |b| b[1]))
        .bind(i64::from(limit) + 1)
        .fetch_all(&self.pool)
        .await?;
        page(&rows, v.user_id, limit, parent.reply_count)
    }

    /// A comment on a published series that is not hidden.
    async fn visible_comment(&self, viewer: i64, id: i64) -> SocialResult<Comment> {
        let ok: Option<i64> = sqlx::query_scalar(
            "SELECT c.id FROM reading_comments c JOIN reading_series s ON s.id = c.series_id \
             WHERE c.id = $1 AND NOT c.hidden AND s.published AND s.merged_into IS NULL",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        ok.ok_or(SocialError::NotFound("comment"))?;
        self.comment_by_id(viewer, id).await
    }

    /// Edit one's own comment.
    pub async fn edit_comment(
        &self,
        v: Viewer,
        id: i64,
        patch: CommentPatch,
    ) -> SocialResult<Comment> {
        let current = self.visible_comment(v.user_id, id).await?;
        if current.author_id != v.user_id {
            return Err(SocialError::NotFound("comment"));
        }
        let body = match &patch.body {
            Some(b) => rules::clean_text(b, rules::MAX_COMMENT, "body", false)?,
            None => current.body.clone(),
        };
        sqlx::query(
            "UPDATE reading_comments SET body = $2, spoiler = $3, edited_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(&body)
        .bind(patch.spoiler.unwrap_or(current.spoiler))
        .execute(&self.pool)
        .await?;
        self.comment_by_id(v.user_id, id).await
    }

    /// Delete one's own comment (its replies go with it).
    pub async fn delete_comment(&self, v: Viewer, id: i64) -> SocialResult<()> {
        let n = sqlx::query("DELETE FROM reading_comments WHERE id = $1 AND user_id = $2")
            .bind(id)
            .bind(v.user_id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if n == 0 {
            return Err(SocialError::NotFound("comment"));
        }
        Ok(())
    }

    /// Like a comment, or take the like back. Returns the count.
    pub async fn set_comment_like(&self, v: Viewer, id: i64, on: bool) -> SocialResult<i32> {
        let c = self.visible_comment(v.user_id, id).await?;
        self.actionable_series(v, c.series_id).await?;
        let mut tx = self.pool.begin().await?;
        let changed = if on {
            sqlx::query(
                "INSERT INTO reading_comment_likes (comment_id, user_id) VALUES ($1, $2) \
                 ON CONFLICT DO NOTHING",
            )
        } else {
            sqlx::query("DELETE FROM reading_comment_likes WHERE comment_id = $1 AND user_id = $2")
        }
        .bind(id)
        .bind(v.user_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if changed > 0 {
            sqlx::query(
                "UPDATE reading_comments SET like_count = \
                   (SELECT count(*) FROM reading_comment_likes WHERE comment_id = $1) WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        let count: i32 =
            sqlx::query_scalar("SELECT like_count FROM reading_comments WHERE id = $1")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(count)
    }

    /// Report a comment for the curators. One report per reporter and comment.
    pub async fn report_comment(&self, v: Viewer, id: i64, reason: &str) -> SocialResult<()> {
        let c = self.visible_comment(v.user_id, id).await?;
        if c.author_id == v.user_id {
            return Err(SocialError::Conflict(
                "you cannot report your own comment".into(),
            ));
        }
        let reason = rules::clean_text(reason, 500, "reason", false)?;
        sqlx::query(
            "INSERT INTO reading_comment_reports (id, reporter_id, comment_id, reason) \
             VALUES ($1, $2, $3, $4) ON CONFLICT (reporter_id, comment_id) DO NOTHING",
        )
        .bind((self.ids)())
        .bind(v.user_id)
        .bind(id)
        .bind(&reason)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Open reports, oldest first (curators).
    pub async fn comment_reports(&self, limit: Option<u32>) -> SocialResult<Vec<CommentReport>> {
        let rows = sqlx::query(
            "SELECT id, reporter_id, comment_id, reason, status, \
                    (extract(epoch FROM created_at) * 1000)::bigint AS created_ms \
               FROM reading_comment_reports WHERE status = 'open' ORDER BY created_at, id LIMIT $1",
        )
        .bind(i64::from(rules::limit(limit)))
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in &rows {
            let cid: i64 = r.try_get("comment_id")?;
            out.push(CommentReport {
                id: r.try_get("id")?,
                reporter_id: r.try_get("reporter_id")?,
                reason: r.try_get("reason")?,
                status: r.try_get("status")?,
                created_at_ms: r.try_get("created_ms")?,
                comment: self.comment_by_id(0, cid).await.ok(),
            });
        }
        Ok(out)
    }

    /// Resolve a report: `hide` hides the comment (and actions every open report on it), otherwise
    /// the report is dismissed. Returns the comment id.
    pub async fn resolve_comment_report(
        &self,
        staff: i64,
        report: i64,
        hide: bool,
    ) -> SocialResult<i64> {
        let mut tx = self.pool.begin().await?;
        let cid: i64 = sqlx::query_scalar(
            "SELECT comment_id FROM reading_comment_reports \
             WHERE id = $1 AND status = 'open' FOR UPDATE",
        )
        .bind(report)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(SocialError::NotFound("report"))?;
        if hide {
            sqlx::query("UPDATE reading_comments SET hidden = TRUE WHERE id = $1")
                .bind(cid)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "UPDATE reading_comment_reports SET status = 'actioned', resolved_by = $2, \
                   resolved_at = now() WHERE comment_id = $1 AND status = 'open'",
            )
            .bind(cid)
            .bind(staff)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                "UPDATE reading_comment_reports SET status = 'dismissed', resolved_by = $2, \
                   resolved_at = now() WHERE id = $1",
            )
            .bind(report)
            .bind(staff)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(cid)
    }
}
