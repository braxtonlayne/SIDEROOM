//! Postgres implementation of reading state (progress, read marks, history, bookmarks) and of
//! uploads plus the storage collector queue for [`PgLibraryRepo`]. Tables: `reading_progress`,
//! `reading_bookmarks` (0201), `reading_gc_keys` (0210), `reading_uploads` (0211).

use async_trait::async_trait;
use sqlx::{Row, postgres::PgRow};
use sr_library::error::{LibraryError, LibraryResult};
use sr_library::model::*;
use sr_library::reading::*;
use sr_library::rules;
use sr_library::uploads::UploadRepository;

use crate::library_repo::{
    CHAPTER_FROM, PgLibraryRepo, chapter_cols, chapter_from, db, dynq, ms, series_cols, series_from,
};

const PROGRESS_COLS: &str = "chapter_id, series_id, page_index, percent, cfi, completed, \
     (extract(epoch FROM updated_at) * 1000)::bigint AS updated_ms";

fn progress_from(r: &PgRow) -> LibraryResult<Progress> {
    Ok(Progress {
        chapter_id: r.try_get("chapter_id").map_err(db)?,
        series_id: r.try_get("series_id").map_err(db)?,
        page_index: r.try_get("page_index").map_err(db)?,
        percent: r.try_get("percent").map_err(db)?,
        cfi: r.try_get("cfi").map_err(db)?,
        completed: r.try_get("completed").map_err(db)?,
        updated_at_ms: r.try_get("updated_ms").map_err(db)?,
    })
}

const BOOKMARK_COLS: &str = "b.id, b.chapter_id, c.series_id, b.page_index, b.note, \
     (extract(epoch FROM b.created_at) * 1000)::bigint AS created_ms";

fn bookmark_from(r: &PgRow) -> LibraryResult<Bookmark> {
    Ok(Bookmark {
        id: r.try_get("id").map_err(db)?,
        chapter_id: r.try_get("chapter_id").map_err(db)?,
        series_id: r.try_get("series_id").map_err(db)?,
        page_index: r.try_get("page_index").map_err(db)?,
        note: r.try_get("note").map_err(db)?,
        created_at_ms: r.try_get("created_ms").map_err(db)?,
    })
}

#[async_trait]
impl ReadingStateRepository for PgLibraryRepo {
    async fn get_progress(&self, user: i64, chapter: i64) -> LibraryResult<Option<Progress>> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {PROGRESS_COLS} FROM reading_progress WHERE user_id = $1 AND chapter_id = $2"
        )))
        .bind(user)
        .bind(chapter)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(progress_from).transpose()
    }

    async fn put_progress(
        &self,
        user: i64,
        series: i64,
        chapter: i64,
        w: &ProgressWrite,
        force: bool,
    ) -> LibraryResult<(Progress, bool)> {
        // The WHERE of the upsert is the conflict guard: with a base and no force, a row that
        // was saved after the base is left alone and nothing is returned.
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO reading_progress \
               (user_id, chapter_id, series_id, page_index, percent, cfi, completed, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, now()) \
             ON CONFLICT (user_id, chapter_id) DO UPDATE SET \
               series_id = EXCLUDED.series_id, page_index = EXCLUDED.page_index, \
               percent = EXCLUDED.percent, cfi = EXCLUDED.cfi, \
               completed = EXCLUDED.completed, updated_at = now() \
             WHERE $8::bigint IS NULL OR $9::bool \
                OR (extract(epoch FROM reading_progress.updated_at) * 1000)::bigint <= $8 \
             RETURNING {PROGRESS_COLS}"
        )))
        .bind(user)
        .bind(chapter)
        .bind(series)
        .bind(w.page_index)
        .bind(w.percent)
        .bind(&w.cfi)
        .bind(w.completed)
        .bind(w.base_updated_at_ms)
        .bind(force)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        if let Some(r) = row {
            return Ok((progress_from(&r)?, true));
        }
        let current = self
            .get_progress(user, chapter)
            .await?
            .ok_or(LibraryError::NotFound("progress"))?;
        Ok((current, false))
    }

    async fn clear_progress(&self, user: i64, chapter: i64) -> LibraryResult<bool> {
        let n = sqlx::query("DELETE FROM reading_progress WHERE user_id = $1 AND chapter_id = $2")
            .bind(user)
            .bind(chapter)
            .execute(&self.pool)
            .await
            .map_err(db)?
            .rows_affected();
        Ok(n > 0)
    }

    async fn mark_read(
        &self,
        user: i64,
        chapter: &ChapterRecord,
        read: bool,
        up_to: bool,
    ) -> LibraryResult<u64> {
        // The chapters to mark: this one, and with `up_to` the published ones of its edition
        // that sort before it (number, then id).
        let scope = "c.series_id = $2 AND c.published AND (c.id = $3 OR ($4 AND c.edition_id = $5 \
             AND (round(c.number * 100)::bigint, c.id) <= ($6, $3)))";
        let n = if read {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "INSERT INTO reading_progress \
                   (user_id, chapter_id, series_id, page_index, percent, completed, updated_at) \
                 SELECT $1, c.id, c.series_id, GREATEST(c.page_count - 1, 0), 100, TRUE, now() \
                 FROM reading_chapters c WHERE {scope} \
                 ON CONFLICT (user_id, chapter_id) DO UPDATE SET \
                   completed = TRUE, percent = 100, updated_at = now() \
                 WHERE NOT reading_progress.completed"
            )))
        } else {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "DELETE FROM reading_progress WHERE user_id = $1 AND chapter_id IN ( \
                   SELECT c.id FROM reading_chapters c WHERE {scope})"
            )))
        }
        .bind(user)
        .bind(chapter.series_id)
        .bind(chapter.id)
        .bind(up_to)
        .bind(chapter.edition_id)
        .bind(rules::number_cents(chapter.number))
        .execute(&self.pool)
        .await
        .map_err(db)?
        .rows_affected();
        Ok(n)
    }

    async fn history(
        &self,
        user: i64,
        include_restricted: bool,
        per_series: bool,
        before: Option<(i64, i64)>,
        limit: u32,
    ) -> LibraryResult<Vec<HistoryRecord>> {
        let updated = "(extract(epoch FROM p.updated_at) * 1000)::bigint";
        let inner = format!(
            "SELECT {}{}, {updated} AS sort_ms {CHAPTER_FROM}$1 \
             WHERE p.page_index IS NOT NULL AND c.published AND s.published \
               AND s.merged_into IS NULL AND (NOT s.age_restricted OR $2) ",
            if per_series {
                "DISTINCT ON (c.series_id) "
            } else {
                ""
            },
            chapter_cols(),
        );
        let order_inner = if per_series {
            "ORDER BY c.series_id, p.updated_at DESC, c.id DESC"
        } else {
            ""
        };
        let mut sql = format!("SELECT * FROM ({inner}{order_inner}) h WHERE TRUE");
        if before.is_some() {
            sql.push_str(" AND (h.sort_ms, h.id) < ($3, $4)");
        }
        sql.push_str(" ORDER BY h.sort_ms DESC, h.id DESC LIMIT ");
        sql.push_str(&i64::from(limit).to_string());
        let mut q = dynq(sql).bind(user).bind(include_restricted);
        if let Some((ms_, id)) = before {
            q = q.bind(ms_).bind(id);
        }
        let rows = q.fetch_all(&self.pool).await.map_err(db)?;
        let chapters: Vec<ChapterRecord> = rows
            .iter()
            .map(chapter_from)
            .collect::<LibraryResult<_>>()?;
        let ids: Vec<i64> = chapters.iter().map(|c| c.series_id).collect();
        let series_rows = dynq(format!(
            "SELECT {} FROM reading_series s WHERE s.id = ANY($1)",
            series_cols("s")
        ))
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut by_id = std::collections::HashMap::new();
        for r in &series_rows {
            let s = series_from(r)?;
            by_id.insert(s.id, s);
        }
        Ok(chapters
            .into_iter()
            .filter_map(|chapter| {
                by_id
                    .get(&chapter.series_id)
                    .cloned()
                    .map(|series| HistoryRecord { series, chapter })
            })
            .collect())
    }

    async fn list_bookmarks(&self, user: i64, chapter: i64) -> LibraryResult<Vec<Bookmark>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {BOOKMARK_COLS} FROM reading_bookmarks b \
             JOIN reading_chapters c ON c.id = b.chapter_id \
             WHERE b.user_id = $1 AND b.chapter_id = $2 ORDER BY b.page_index"
        )))
        .bind(user)
        .bind(chapter)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(bookmark_from).collect()
    }

    async fn put_bookmark(
        &self,
        id: i64,
        user: i64,
        chapter: &ChapterRecord,
        page_index: i32,
        note: &str,
    ) -> LibraryResult<(BookmarkOutcome, Option<Bookmark>)> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        // Serialise a member's bookmark writes in one chapter so the cap holds.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("bookmark:{user}:{}", chapter.id))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let (count, exists): (i64, bool) = sqlx::query_as(
            "SELECT count(*), COALESCE(bool_or(page_index = $3), FALSE) FROM reading_bookmarks \
             WHERE user_id = $1 AND chapter_id = $2",
        )
        .bind(user)
        .bind(chapter.id)
        .bind(page_index)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        if !exists && count >= MAX_BOOKMARKS_PER_CHAPTER {
            return Ok((BookmarkOutcome::LimitReached, None));
        }
        sqlx::query(
            "INSERT INTO reading_bookmarks (id, user_id, chapter_id, page_index, note) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (user_id, chapter_id, page_index) DO UPDATE SET note = EXCLUDED.note",
        )
        .bind(id)
        .bind(user)
        .bind(chapter.id)
        .bind(page_index)
        .bind(note)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {BOOKMARK_COLS} FROM reading_bookmarks b \
             JOIN reading_chapters c ON c.id = b.chapter_id \
             WHERE b.user_id = $1 AND b.chapter_id = $2 AND b.page_index = $3"
        )))
        .bind(user)
        .bind(chapter.id)
        .bind(page_index)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok((BookmarkOutcome::Saved, Some(bookmark_from(&row)?)))
    }

    async fn delete_bookmark(&self, user: i64, id: i64) -> LibraryResult<Option<Bookmark>> {
        let row = sqlx::query(
            "WITH gone AS (DELETE FROM reading_bookmarks WHERE id = $1 AND user_id = $2 \
                           RETURNING id, chapter_id, page_index, note, created_at) \
             SELECT b.id, b.chapter_id, c.series_id, b.page_index, b.note, \
                    (extract(epoch FROM b.created_at) * 1000)::bigint AS created_ms \
             FROM gone b JOIN reading_chapters c ON c.id = b.chapter_id",
        )
        .bind(id)
        .bind(user)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(bookmark_from).transpose()
    }

    async fn all_bookmarks(
        &self,
        user: i64,
        include_restricted: bool,
        before: Option<(i64, i64)>,
        limit: u32,
    ) -> LibraryResult<Vec<BookmarkRecord>> {
        let created = "(extract(epoch FROM b.created_at) * 1000)::bigint";
        let mut sql = format!(
            "SELECT {BOOKMARK_COLS}, s.title AS series_title, c.number::float8 AS chapter_number, \
                    c.title AS chapter_title \
             FROM reading_bookmarks b \
             JOIN reading_chapters c ON c.id = b.chapter_id \
             JOIN reading_series s ON s.id = c.series_id \
             WHERE b.user_id = $1 AND c.published AND s.published AND s.merged_into IS NULL \
               AND (NOT s.age_restricted OR $2)"
        );
        if before.is_some() {
            sql.push_str(&format!(" AND ({created}, b.id) < ($3, $4)"));
        }
        sql.push_str(&format!(
            " ORDER BY {created} DESC, b.id DESC LIMIT {}",
            i64::from(limit)
        ));
        let mut q = dynq(sql).bind(user).bind(include_restricted);
        if let Some((t, id)) = before {
            q = q.bind(t).bind(id);
        }
        let rows = q.fetch_all(&self.pool).await.map_err(db)?;
        rows.iter()
            .map(|r| {
                Ok(BookmarkRecord {
                    bookmark: bookmark_from(r)?,
                    series_title: r.try_get("series_title").map_err(db)?,
                    chapter_number: r.try_get("chapter_number").map_err(db)?,
                    chapter_title: r.try_get("chapter_title").map_err(db)?,
                })
            })
            .collect()
    }

    async fn followers_to_notify(
        &self,
        series: i64,
        after: i64,
        limit: u32,
    ) -> LibraryResult<Vec<i64>> {
        sqlx::query_scalar(
            "SELECT user_id FROM reading_follows WHERE series_id = $1 AND notify AND user_id > $2 \
             ORDER BY user_id LIMIT $3",
        )
        .bind(series)
        .bind(after)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(db)
    }
}

const UPLOAD_COLS: &str = "id, chapter_id, series_id, user_id, filename, kind, size_bytes, \
     object_key, status, error, warnings, page_count, metadata";

fn upload_cols() -> String {
    format!(
        "{UPLOAD_COLS}, {}, {}, {}",
        ms("created_at", "created_ms"),
        ms("updated_at", "updated_ms"),
        ms("finished_at", "finished_ms"),
    )
}

fn upload_from(r: &PgRow) -> LibraryResult<UploadRecord> {
    let kind: String = r.try_get("kind").map_err(db)?;
    let status: String = r.try_get("status").map_err(db)?;
    let (Some(kind), Some(status)) = (UploadKind::parse(&kind), UploadStatus::parse(&status))
    else {
        return Err(LibraryError::Storage(format!(
            "unexpected upload enum {kind}/{status}"
        )));
    };
    Ok(UploadRecord {
        id: r.try_get("id").map_err(db)?,
        chapter_id: r.try_get("chapter_id").map_err(db)?,
        series_id: r.try_get("series_id").map_err(db)?,
        user_id: r.try_get("user_id").map_err(db)?,
        filename: r.try_get("filename").map_err(db)?,
        kind,
        size_bytes: r.try_get("size_bytes").map_err(db)?,
        object_key: r.try_get("object_key").map_err(db)?,
        status,
        error: r.try_get("error").map_err(db)?,
        warnings: r.try_get("warnings").map_err(db)?,
        page_count: r.try_get("page_count").map_err(db)?,
        metadata: r.try_get("metadata").map_err(db)?,
        created_at_ms: r.try_get("created_ms").map_err(db)?,
        updated_at_ms: r.try_get("updated_ms").map_err(db)?,
        finished_at_ms: r.try_get("finished_ms").map_err(db)?,
    })
}

#[async_trait]
impl UploadRepository for PgLibraryRepo {
    async fn create_upload(&self, u: &UploadRecord) -> LibraryResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        // Serialise uploads of one chapter.
        let locked: Option<i64> =
            sqlx::query_scalar("SELECT id FROM reading_chapters WHERE id = $1 FOR UPDATE")
                .bind(u.chapter_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        if locked.is_none() {
            return Err(LibraryError::NotFound("chapter"));
        }
        let busy: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM reading_uploads WHERE chapter_id = $1 AND status IN ('queued', 'running') \
             LIMIT 1",
        )
        .bind(u.chapter_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        if busy.is_some() {
            return Err(LibraryError::Conflict(
                "this chapter already has an upload being converted".into(),
            ));
        }
        // A slot that was never used is superseded; its object goes to the collector.
        sqlx::query(
            "WITH old AS (UPDATE reading_uploads SET status = 'expired', finished_at = now(), \
                            updated_at = now() \
                          WHERE chapter_id = $1 AND status = 'awaiting_upload' RETURNING object_key) \
             INSERT INTO reading_gc_keys (key) SELECT object_key FROM old \
             ON CONFLICT (key) DO UPDATE SET queued_at = now()",
        )
        .bind(u.chapter_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query(
            "INSERT INTO reading_uploads \
               (id, chapter_id, series_id, user_id, filename, kind, size_bytes, object_key, status) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(u.id)
        .bind(u.chapter_id)
        .bind(u.series_id)
        .bind(u.user_id)
        .bind(&u.filename)
        .bind(u.kind.as_str())
        .bind(u.size_bytes)
        .bind(&u.object_key)
        .bind(u.status.as_str())
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    async fn get_upload(&self, id: i64) -> LibraryResult<Option<UploadRecord>> {
        let row = dynq(format!(
            "SELECT {} FROM reading_uploads WHERE id = $1",
            upload_cols()
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(upload_from).transpose()
    }

    async fn list_uploads(&self, chapter: i64, limit: u32) -> LibraryResult<Vec<UploadRecord>> {
        let rows = dynq(format!(
            "SELECT {} FROM reading_uploads WHERE chapter_id = $1 \
             ORDER BY created_at DESC, id DESC LIMIT $2",
            upload_cols()
        ))
        .bind(chapter)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(upload_from).collect()
    }

    async fn transition_upload(
        &self,
        id: i64,
        from: &[UploadStatus],
        to: UploadStatus,
    ) -> LibraryResult<Option<UploadRecord>> {
        let from: Vec<&str> = from.iter().map(|s| s.as_str()).collect();
        let row = dynq(format!(
            "UPDATE reading_uploads SET status = $3, updated_at = now(), \
               finished_at = CASE WHEN $3 IN ('done', 'failed', 'expired') THEN now() ELSE finished_at END \
             WHERE id = $1 AND status = ANY($2) RETURNING {}",
            upload_cols()
        ))
        .bind(id)
        .bind(&from)
        .bind(to.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(upload_from).transpose()
    }

    async fn finish_upload(
        &self,
        id: i64,
        status: UploadStatus,
        error: Option<&str>,
        warnings: &[String],
        page_count: Option<i32>,
        metadata: &serde_json::Value,
    ) -> LibraryResult<Option<UploadRecord>> {
        let row = dynq(format!(
            "UPDATE reading_uploads SET status = $2, error = $3, warnings = $4, page_count = $5, \
               metadata = $6, updated_at = now(), finished_at = now() \
             WHERE id = $1 AND status IN ('awaiting_upload', 'queued', 'running') RETURNING {}",
            upload_cols()
        ))
        .bind(id)
        .bind(status.as_str())
        .bind(error)
        .bind(warnings)
        .bind(page_count)
        .bind(metadata)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(upload_from).transpose()
    }

    async fn set_chapter_source(
        &self,
        chapter: i64,
        key: &str,
        page_count: i32,
    ) -> LibraryResult<bool> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let old: Option<Option<String>> =
            sqlx::query_scalar("SELECT source_key FROM reading_chapters WHERE id = $1 FOR UPDATE")
                .bind(chapter)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        let Some(old) = old else {
            return Ok(false);
        };
        crate::library_repo::queue_replaced(&mut tx, old, Some(key)).await?;
        sqlx::query(
            "UPDATE reading_chapters SET source_key = $2, page_count = $3, updated_at = now() \
             WHERE id = $1",
        )
        .bind(chapter)
        .bind(key)
        .bind(page_count)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(true)
    }

    async fn expire_uploads(&self, before_ms: i64, limit: u32) -> LibraryResult<Vec<UploadRecord>> {
        let rows = dynq(format!(
            "UPDATE reading_uploads SET status = 'expired', updated_at = now(), finished_at = now() \
             WHERE id IN (SELECT id FROM reading_uploads WHERE status = 'awaiting_upload' \
                            AND created_at < to_timestamp($1::float8 / 1000.0) \
                          ORDER BY created_at LIMIT $2) \
             RETURNING {}",
            upload_cols()
        ))
        .bind(before_ms)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(upload_from).collect()
    }

    async fn set_multipart(
        &self,
        upload: i64,
        multipart_id: &str,
        part_size: u64,
    ) -> LibraryResult<()> {
        sqlx::query(
            "INSERT INTO reading_upload_multipart (upload_id, multipart_id, part_size) \n             VALUES ($1, $2, $3) \n             ON CONFLICT (upload_id) DO UPDATE SET multipart_id = EXCLUDED.multipart_id, \n               part_size = EXCLUDED.part_size",
        )
        .bind(upload)
        .bind(multipart_id)
        .bind(i64::try_from(part_size).unwrap_or(i64::MAX))
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn get_multipart(&self, upload: i64) -> LibraryResult<Option<(String, u64)>> {
        let row: Option<(String, i64)> = sqlx::query_as(
            "SELECT multipart_id, part_size FROM reading_upload_multipart WHERE upload_id = $1",
        )
        .bind(upload)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        Ok(row.map(|(id, size)| (id, u64::try_from(size).unwrap_or(0))))
    }

    async fn queue_gc(&self, keys: &[String], chapter: Option<i64>) -> LibraryResult<()> {
        if keys.is_empty() {
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO reading_gc_keys (key, chapter_id) SELECT k, $2 FROM UNNEST($1::text[]) k \
             ON CONFLICT (key) DO UPDATE SET queued_at = now(), \
               chapter_id = COALESCE(EXCLUDED.chapter_id, reading_gc_keys.chapter_id)",
        )
        .bind(keys)
        .bind(chapter)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn export_touch(&self, chapter: i64, key: &str) -> LibraryResult<()> {
        sqlx::query(
            "INSERT INTO library_export_cache (export_key, chapter_id) VALUES ($1, $2) \
             ON CONFLICT (export_key) DO UPDATE SET last_read_at = now()",
        )
        .bind(key)
        .bind(chapter)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn export_adopt(&self, keys: &[String]) -> LibraryResult<u64> {
        // `library/exports/{chapter}/{fingerprint}.cbz`; a key that does not parse is not ours.
        let mut rows: Vec<(String, i64)> = Vec::new();
        for k in keys {
            if let Some(id) = k
                .strip_prefix("library/exports/")
                .and_then(|r| r.split('/').next())
                .and_then(|c| c.parse::<i64>().ok())
            {
                rows.push((k.clone(), id));
            }
        }
        if rows.is_empty() {
            return Ok(0);
        }
        let (ks, cs): (Vec<String>, Vec<i64>) = rows.into_iter().unzip();
        let done = sqlx::query(
            "INSERT INTO library_export_cache (export_key, chapter_id) \
             SELECT * FROM unnest($1::text[], $2::bigint[]) ON CONFLICT (export_key) DO NOTHING",
        )
        .bind(&ks)
        .bind(&cs)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(done.rows_affected())
    }

    async fn exports_idle(&self, before_ms: i64, limit: u32) -> LibraryResult<Vec<String>> {
        sqlx::query_scalar(
            "SELECT export_key FROM library_export_cache \
             WHERE last_read_at < to_timestamp($1::float8 / 1000.0) ORDER BY last_read_at LIMIT $2",
        )
        .bind(before_ms)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(db)
    }

    async fn exports_forget(&self, keys: &[String]) -> LibraryResult<u64> {
        if keys.is_empty() {
            return Ok(0);
        }
        let done = sqlx::query("DELETE FROM library_export_cache WHERE export_key = ANY($1)")
            .bind(keys)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(done.rows_affected())
    }

    async fn gc_due(&self, before_ms: i64, limit: u32) -> LibraryResult<Vec<String>> {
        // Queued objects that a page, cover, source or live upload still points at are dropped
        // from the queue instead of being deleted (a re-ingest may have reused a key).
        sqlx::query_scalar(
            "WITH due AS ( \
               SELECT key, chapter_id FROM reading_gc_keys \
               WHERE queued_at < to_timestamp($1::float8 / 1000.0) ORDER BY queued_at LIMIT $2), \
             referenced AS ( \
               SELECT d.key FROM due d WHERE \
                 EXISTS (SELECT 1 FROM reading_pages p WHERE d.chapter_id IS NOT NULL \
                           AND p.chapter_id = d.chapter_id \
                           AND (p.key = d.key OR EXISTS ( \
                                 SELECT 1 FROM jsonb_each_text(p.variants) v WHERE v.value = d.key))) \
                 OR EXISTS (SELECT 1 FROM reading_series s WHERE s.cover_key = d.key) \
                 OR EXISTS (SELECT 1 FROM reading_volumes v WHERE v.cover_key = d.key) \
                 OR EXISTS (SELECT 1 FROM reading_chapters c WHERE c.source_key = d.key) \
                 OR EXISTS (SELECT 1 FROM reading_uploads u WHERE u.object_key = d.key \
                              AND u.status IN ('awaiting_upload', 'queued', 'running'))), \
             dropped AS (DELETE FROM reading_gc_keys k USING referenced r WHERE k.key = r.key) \
             SELECT key FROM due WHERE key NOT IN (SELECT key FROM referenced)",
        )
        .bind(before_ms)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(db)
    }

    async fn gc_done(&self, keys: &[String]) -> LibraryResult<u64> {
        if keys.is_empty() {
            return Ok(0);
        }
        let n = sqlx::query("DELETE FROM reading_gc_keys WHERE key = ANY($1)")
            .bind(keys)
            .execute(&self.pool)
            .await
            .map_err(db)?
            .rows_affected();
        Ok(n)
    }
}

#[async_trait]
impl sr_library::CoverHashes for PgLibraryRepo {
    async fn put(&self, series: i64, cover_key: &str, hash: u64) -> LibraryResult<()> {
        sqlx::query(
            "INSERT INTO library_cover_hashes (series_id, cover_key, phash) VALUES ($1, $2, $3) \
             ON CONFLICT (series_id) DO UPDATE SET cover_key = EXCLUDED.cover_key, \
               phash = EXCLUDED.phash, created_at = now()",
        )
        // The bit pattern is stored in a signed column; `as` keeps it.
        .bind(series)
        .bind(cover_key)
        .bind(hash as i64)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn unhashed(&self, limit: u32) -> LibraryResult<Vec<(i64, String)>> {
        sqlx::query_as(
            "SELECT s.id, s.cover_key FROM reading_series s \
             LEFT JOIN library_cover_hashes h ON h.series_id = s.id \
             WHERE s.published AND s.merged_into IS NULL AND s.cover_key IS NOT NULL \
               AND (h.series_id IS NULL OR h.cover_key <> s.cover_key) \
             ORDER BY s.id LIMIT $1",
        )
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(db)
    }

    async fn nearest(
        &self,
        hash: u64,
        max_distance: u32,
        exclude: i64,
        limit: u32,
    ) -> LibraryResult<Vec<(i64, u32)>> {
        let rows: Vec<(i64, i32)> = sqlx::query_as(
            "SELECT h.series_id, bit_count((h.phash # $1)::bit(64))::int AS d \
             FROM library_cover_hashes h JOIN reading_series s ON s.id = h.series_id \
             WHERE s.published AND s.merged_into IS NULL AND h.series_id <> $2 \
               AND h.cover_key = s.cover_key \
               AND bit_count((h.phash # $1)::bit(64)) <= $3 \
             ORDER BY d, h.series_id LIMIT $4",
        )
        .bind(hash as i64)
        .bind(exclude)
        .bind(i64::from(max_distance))
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|(id, d)| (id, u32::try_from(d).unwrap_or(u32::MAX)))
            .collect())
    }
}
