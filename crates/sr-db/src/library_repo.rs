//! Postgres implementation of [`sr_library::LibraryRepository`] (tables in migrations
//! 0200 and 0201). Visibility is part of every read query: a chapter is readable only
//! when it and its series are published. Age-restricted series are visible to everyone;
//! the service gates opening their pages on age confirmation (D-184).
//! Multi-row changes (merge, page replacement, publish) run in one transaction.
//! Tests: `tests/library_repo.rs` (needs `SR_TEST_DATABASE_URL`).

use async_trait::async_trait;
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder, Row, postgres::PgRow};
use sr_library::error::{LibraryError, LibraryResult};
use sr_library::model::*;
use sr_library::repo::{LibraryRepository, Neighbours, NewSeries, Rows, UpdateRecord};

pub struct PgLibraryRepo {
    pool: PgPool,
}

impl PgLibraryRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn db(e: sqlx::Error) -> LibraryError {
    LibraryError::Storage(e.to_string())
}

fn code(e: &sqlx::Error) -> Option<String> {
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| c.into_owned())
}

fn ms(col: &str, name: &str) -> String {
    format!("(extract(epoch FROM {col}) * 1000)::bigint AS {name}")
}

/// Columns of a series row, read by [`series_from`].
fn series_cols(a: &str) -> String {
    format!(
        "{a}.id, {a}.kind, {a}.title, {a}.alt_titles, {a}.authors, {a}.artists, {a}.status, \
         {a}.year, {a}.synopsis, {a}.genres, {a}.tags, {a}.reading_direction, {a}.age_restricted, \
         {a}.cover_key, {a}.published, {a}.merged_into, {a}.chapter_count, {a}.follower_count, {}, {}, {}, {}",
        ms(&format!("{a}.published_at"), "published_at_ms"),
        ms(&format!("{a}.last_chapter_at"), "last_chapter_at_ms"),
        ms(&format!("{a}.created_at"), "created_at_ms"),
        ms(&format!("{a}.updated_at"), "updated_at_ms"),
    )
}

fn bad<T>(what: &str, v: &str) -> LibraryResult<T> {
    Err(LibraryError::Storage(format!("unexpected {what} {v:?}")))
}

fn series_from(r: &PgRow) -> LibraryResult<SeriesRecord> {
    let g = |e: sqlx::Error| db(e);
    let kind: String = r.try_get("kind").map_err(g)?;
    let status: String = r.try_get("status").map_err(g)?;
    let dir: String = r.try_get("reading_direction").map_err(g)?;
    let (Some(kind), Some(status), Some(dir)) = (
        SeriesKind::parse(&kind),
        SeriesStatus::parse(&status),
        ReadingDirection::parse(&dir),
    ) else {
        return bad("series enum", &format!("{kind}/{status}/{dir}"));
    };
    Ok(SeriesRecord {
        id: r.try_get("id").map_err(g)?,
        kind,
        title: r.try_get("title").map_err(g)?,
        alt_titles: r.try_get("alt_titles").map_err(g)?,
        authors: r.try_get("authors").map_err(g)?,
        artists: r.try_get("artists").map_err(g)?,
        status,
        year: r.try_get("year").map_err(g)?,
        synopsis: r.try_get("synopsis").map_err(g)?,
        genres: r.try_get("genres").map_err(g)?,
        tags: r.try_get("tags").map_err(g)?,
        reading_direction: dir,
        age_restricted: r.try_get("age_restricted").map_err(g)?,
        cover_key: r.try_get("cover_key").map_err(g)?,
        published: r.try_get("published").map_err(g)?,
        published_at_ms: r.try_get("published_at_ms").map_err(g)?,
        merged_into: r.try_get("merged_into").map_err(g)?,
        chapter_count: r.try_get("chapter_count").map_err(g)?,
        last_chapter_at_ms: r.try_get("last_chapter_at_ms").map_err(g)?,
        follower_count: r.try_get("follower_count").map_err(g)?,
        created_at_ms: r.try_get("created_at_ms").map_err(g)?,
        updated_at_ms: r.try_get("updated_at_ms").map_err(g)?,
    })
}

fn chapter_cols() -> String {
    format!(
        "c.id, c.series_id, c.edition_id, c.volume_id, c.number::float8 AS number, c.title, \
         c.page_count, c.format, c.source_key, c.release_group, c.published, {}, \
         s.published AS series_published, s.age_restricted AS series_age_restricted, \
         s.reading_direction AS series_direction, (s.merged_into IS NOT NULL) AS series_merged, \
         p.page_index AS p_page, p.percent AS p_percent, p.completed AS p_completed",
        ms("c.published_at", "published_at_ms"),
    )
}

const CHAPTER_FROM: &str = "FROM reading_chapters c JOIN reading_series s ON s.id = c.series_id \
     LEFT JOIN reading_progress p ON p.chapter_id = c.id AND p.user_id = ";

fn chapter_from(r: &PgRow) -> LibraryResult<ChapterRecord> {
    let g = |e: sqlx::Error| db(e);
    let format: String = r.try_get("format").map_err(g)?;
    let dir: String = r.try_get("series_direction").map_err(g)?;
    let (Some(format), Some(dir)) = (ChapterFormat::parse(&format), ReadingDirection::parse(&dir))
    else {
        return bad("chapter enum", &format!("{format}/{dir}"));
    };
    let page: Option<i32> = r.try_get("p_page").map_err(g)?;
    let progress = match page {
        Some(page_index) => Some(ChapterProgress {
            page_index,
            percent: r.try_get("p_percent").map_err(g)?,
            completed: r.try_get("p_completed").map_err(g)?,
        }),
        None => None,
    };
    Ok(ChapterRecord {
        id: r.try_get("id").map_err(g)?,
        series_id: r.try_get("series_id").map_err(g)?,
        edition_id: r.try_get("edition_id").map_err(g)?,
        volume_id: r.try_get("volume_id").map_err(g)?,
        number: r.try_get("number").map_err(g)?,
        title: r.try_get("title").map_err(g)?,
        page_count: r.try_get("page_count").map_err(g)?,
        format,
        source_key: r.try_get("source_key").map_err(g)?,
        release_group: r.try_get("release_group").map_err(g)?,
        published: r.try_get("published").map_err(g)?,
        published_at_ms: r.try_get("published_at_ms").map_err(g)?,
        series_published: r.try_get("series_published").map_err(g)?,
        series_age_restricted: r.try_get("series_age_restricted").map_err(g)?,
        series_direction: dir,
        series_merged: r.try_get("series_merged").map_err(g)?,
        progress,
    })
}

fn edition_from(r: &PgRow) -> LibraryResult<Edition> {
    let g = |e: sqlx::Error| db(e);
    Ok(Edition {
        id: r.try_get("id").map_err(g)?,
        series_id: r.try_get("series_id").map_err(g)?,
        language: r.try_get("language").map_err(g)?,
        label: r.try_get("label").map_err(g)?,
        is_default: r.try_get("is_default").map_err(g)?,
    })
}

fn follow_from(r: &PgRow) -> LibraryResult<Follow> {
    let g = |e: sqlx::Error| db(e);
    let shelf: String = r.try_get("shelf").map_err(g)?;
    Ok(Follow {
        shelf: Shelf::parse(&shelf)
            .ok_or_else(|| LibraryError::Storage(format!("shelf {shelf}")))?,
        notify: r.try_get("notify").map_err(g)?,
        created_at_ms: r.try_get("follow_created_ms").map_err(g)?,
    })
}

/// Series visibility for `Scope`, appended to a query on alias `s`.
fn push_scope(qb: &mut QueryBuilder<Postgres>, scope: Scope) {
    if scope == Scope::Public {
        qb.push(" AND s.published AND s.merged_into IS NULL");
    }
}

/// Recompute the counters kept on a series row.
async fn refresh_stats(c: &mut PgConnection, series: i64) -> LibraryResult<()> {
    sqlx::query(
        "UPDATE reading_series s SET \
           chapter_count = (SELECT count(*) FROM reading_chapters c WHERE c.series_id = s.id AND c.published), \
           last_chapter_at = (SELECT max(c.published_at) FROM reading_chapters c WHERE c.series_id = s.id AND c.published), \
           follower_count = (SELECT count(*) FROM reading_follows f WHERE f.series_id = s.id) \
         WHERE s.id = $1",
    )
    .bind(series)
    .execute(&mut *c)
    .await
    .map_err(db)?;
    Ok(())
}

async fn fetch_chapter(
    c: &mut PgConnection,
    id: i64,
    viewer: Option<i64>,
) -> LibraryResult<Option<ChapterRecord>> {
    let sql = format!("SELECT {} {CHAPTER_FROM}$2 WHERE c.id = $1", chapter_cols());
    let row = dynq(sql)
        .bind(id)
        .bind(viewer)
        .fetch_optional(&mut *c)
        .await
        .map_err(db)?;
    row.as_ref().map(chapter_from).transpose()
}

async fn fetch_series(c: &mut PgConnection, id: i64) -> LibraryResult<Option<SeriesRecord>> {
    let sql = format!(
        "SELECT {} FROM reading_series s WHERE s.id = $1",
        series_cols("s")
    );
    let row = dynq(sql)
        .bind(id)
        .fetch_optional(&mut *c)
        .await
        .map_err(db)?;
    row.as_ref().map(series_from).transpose()
}

/// Keep exactly one default edition when there is any edition.
async fn ensure_default_edition(c: &mut PgConnection, series: i64) -> LibraryResult<()> {
    sqlx::query(
        "UPDATE reading_editions SET is_default = TRUE WHERE id = ( \
           SELECT id FROM reading_editions WHERE series_id = $1 ORDER BY created_at, id LIMIT 1) \
         AND NOT EXISTS (SELECT 1 FROM reading_editions WHERE series_id = $1 AND is_default)",
    )
    .bind(series)
    .execute(&mut *c)
    .await
    .map_err(db)?;
    Ok(())
}

fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

async fn volume_belongs(
    c: &mut PgConnection,
    volume: Option<i64>,
    edition: i64,
) -> LibraryResult<()> {
    let Some(v) = volume else { return Ok(()) };
    let ok: Option<i32> =
        sqlx::query_scalar("SELECT 1 FROM reading_volumes WHERE id = $1 AND edition_id = $2")
            .bind(v)
            .bind(edition)
            .fetch_optional(&mut *c)
            .await
            .map_err(db)?;
    if ok.is_some() {
        Ok(())
    } else {
        Err(LibraryError::Invalid(
            "volume_id does not belong to the chapter's edition".into(),
        ))
    }
}

#[async_trait]
impl LibraryRepository for PgLibraryRepo {
    async fn browse(&self, q: &BrowseQuery, scope: Scope) -> LibraryResult<Rows<SeriesRecord>> {
        let mut qb = QueryBuilder::<Postgres>::new(format!(
            "SELECT {}, count(*) OVER () AS total FROM reading_series s WHERE TRUE",
            series_cols("s")
        ));
        push_scope(&mut qb, scope);
        match q.age {
            AgeFilter::Only => {
                qb.push(" AND s.age_restricted");
            }
            AgeFilter::Include => {}
            AgeFilter::Hide => {
                qb.push(" AND NOT s.age_restricted");
            }
        }
        if let Some(k) = q.kind {
            qb.push(" AND s.kind = ").push_bind(k.as_str());
        }
        if let Some(st) = q.status {
            qb.push(" AND s.status = ").push_bind(st.as_str());
        }
        if !q.genres.is_empty() {
            qb.push(" AND s.genres @> ").push_bind(q.genres.clone());
        }
        if !q.exclude_genres.is_empty() {
            qb.push(" AND NOT (s.genres && ")
                .push_bind(q.exclude_genres.clone())
                .push(")");
        }
        if !q.tags.is_empty() {
            qb.push(" AND s.tags @> ").push_bind(q.tags.clone());
        }
        if let Some(l) = &q.language {
            qb.push(
                " AND EXISTS (SELECT 1 FROM reading_editions e \
                 JOIN reading_chapters c ON c.edition_id = e.id \
                 WHERE e.series_id = s.id AND e.language = ",
            )
            .push_bind(l.clone());
            if scope == Scope::Public {
                qb.push(" AND c.published");
            }
            qb.push(")");
        }
        if let Some(y) = q.year_from {
            qb.push(" AND s.year >= ").push_bind(y);
        }
        if let Some(y) = q.year_to {
            qb.push(" AND s.year <= ").push_bind(y);
        }
        if scope == Scope::Staff {
            if let Some(p) = q.published {
                qb.push(" AND s.published = ").push_bind(p);
            }
            if let Some(t) = &q.title_contains {
                qb.push(" AND s.title ILIKE ")
                    .push_bind(format!("%{}%", like_escape(t)));
            }
        }
        let (expr, natural_desc) = match q.sort {
            SortKey::Newest => ("s.published_at", true),
            SortKey::Updated => ("s.last_chapter_at", true),
            SortKey::Title => ("lower(s.title)", false),
            SortKey::Followers => ("s.follower_count", true),
        };
        let dir = if q.descending.unwrap_or(natural_desc) {
            "DESC"
        } else {
            "ASC"
        };
        qb.push(format!(" ORDER BY {expr} {dir} NULLS LAST, s.id {dir}"));
        qb.push(" LIMIT ").push_bind(i64::from(q.limit));
        qb.push(" OFFSET ").push_bind(i64::from(q.offset));
        let rows = qb.build().fetch_all(&self.pool).await.map_err(db)?;
        let total = match rows.first() {
            Some(r) => r.try_get("total").map_err(db)?,
            None => 0,
        };
        Ok(Rows {
            items: rows.iter().map(series_from).collect::<Result<_, _>>()?,
            total,
        })
    }

    async fn get_series(&self, id: i64, scope: Scope) -> LibraryResult<Option<SeriesRecord>> {
        let mut qb = QueryBuilder::<Postgres>::new(format!(
            "SELECT {} FROM reading_series s WHERE s.id = ",
            series_cols("s")
        ));
        qb.push_bind(id);
        push_scope(&mut qb, scope);
        let row = qb.build().fetch_optional(&self.pool).await.map_err(db)?;
        row.as_ref().map(series_from).transpose()
    }

    async fn series_by_ids(&self, ids: &[i64], scope: Scope) -> LibraryResult<Vec<SeriesRecord>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut qb = QueryBuilder::<Postgres>::new(format!(
            "SELECT {} FROM reading_series s WHERE s.id = ANY(",
            series_cols("s")
        ));
        qb.push_bind(ids.to_vec()).push(")");
        push_scope(&mut qb, scope);
        let rows = qb.build().fetch_all(&self.pool).await.map_err(db)?;
        let mut recs: Vec<SeriesRecord> = rows.iter().map(series_from).collect::<Result<_, _>>()?;
        recs.sort_by_key(|r| ids.iter().position(|i| *i == r.id).unwrap_or(usize::MAX));
        Ok(recs)
    }

    async fn list_editions(&self, series_id: i64) -> LibraryResult<Vec<Edition>> {
        let rows = sqlx::query(
            "SELECT id, series_id, language, label, is_default FROM reading_editions \
             WHERE series_id = $1 ORDER BY is_default DESC, created_at, id",
        )
        .bind(series_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(edition_from).collect()
    }

    async fn list_volumes(&self, series_id: i64) -> LibraryResult<Vec<Volume>> {
        let rows = sqlx::query(
            "SELECT v.id, v.edition_id, v.number::float8 AS number, v.title FROM reading_volumes v \
             JOIN reading_editions e ON e.id = v.edition_id WHERE e.series_id = $1 \
             ORDER BY v.edition_id, v.number",
        )
        .bind(series_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|r| {
                Ok(Volume {
                    id: r.try_get("id").map_err(db)?,
                    edition_id: r.try_get("edition_id").map_err(db)?,
                    number: r.try_get("number").map_err(db)?,
                    title: r.try_get("title").map_err(db)?,
                })
            })
            .collect()
    }

    async fn list_chapters(
        &self,
        series_id: i64,
        edition: Option<i64>,
        viewer: Option<i64>,
        scope: Scope,
    ) -> LibraryResult<Vec<ChapterRecord>> {
        let mut qb =
            QueryBuilder::<Postgres>::new(format!("SELECT {} {CHAPTER_FROM}", chapter_cols()));
        qb.push_bind(viewer)
            .push(" WHERE c.series_id = ")
            .push_bind(series_id);
        if let Some(e) = edition {
            qb.push(" AND c.edition_id = ").push_bind(e);
        }
        if scope == Scope::Public {
            qb.push(" AND c.published");
        }
        push_scope(&mut qb, scope);
        qb.push(" ORDER BY c.edition_id, c.number, c.id");
        let rows = qb.build().fetch_all(&self.pool).await.map_err(db)?;
        rows.iter().map(chapter_from).collect()
    }

    async fn get_chapter(
        &self,
        id: i64,
        viewer: Option<i64>,
    ) -> LibraryResult<Option<ChapterRecord>> {
        let mut c = self.pool.acquire().await.map_err(db)?;
        fetch_chapter(&mut c, id, viewer).await
    }

    async fn list_pages(&self, chapter_id: i64) -> LibraryResult<Vec<PageRecord>> {
        let rows = sqlx::query(
            "SELECT idx, key, variants, width, height, is_double_spread, blurhash \
             FROM reading_pages WHERE chapter_id = $1 ORDER BY idx",
        )
        .bind(chapter_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|r| {
                let v: serde_json::Value = r.try_get("variants").map_err(db)?;
                let mut variants: Vec<(u32, String)> = v
                    .as_object()
                    .map(|m| {
                        m.iter()
                            .filter_map(|(w, k)| Some((w.parse().ok()?, k.as_str()?.to_owned())))
                            .collect()
                    })
                    .unwrap_or_default();
                variants.sort_unstable();
                Ok(PageRecord {
                    index: r.try_get("idx").map_err(db)?,
                    key: r.try_get("key").map_err(db)?,
                    variants,
                    width: r.try_get("width").map_err(db)?,
                    height: r.try_get("height").map_err(db)?,
                    is_double_spread: r.try_get("is_double_spread").map_err(db)?,
                    blurhash: r.try_get("blurhash").map_err(db)?,
                })
            })
            .collect()
    }

    async fn neighbours(&self, chapter_id: i64) -> LibraryResult<Neighbours> {
        let row = sqlx::query(
            "SELECT \
               (SELECT p.id FROM reading_chapters p WHERE p.edition_id = c.edition_id AND p.published \
                  AND p.number < c.number ORDER BY p.number DESC LIMIT 1) AS prev, \
               (SELECT n.id FROM reading_chapters n WHERE n.edition_id = c.edition_id AND n.published \
                  AND n.number > c.number ORDER BY n.number ASC LIMIT 1) AS next \
             FROM reading_chapters c WHERE c.id = $1",
        )
        .bind(chapter_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        match row {
            Some(r) => Ok(Neighbours {
                prev: r.try_get("prev").map_err(db)?,
                next: r.try_get("next").map_err(db)?,
            }),
            None => Ok(Neighbours::default()),
        }
    }

    async fn latest_updates(
        &self,
        include_restricted: bool,
        followed_by: Option<i64>,
        before: Option<(i64, i64)>,
        limit: u32,
    ) -> LibraryResult<Vec<UpdateRecord>> {
        let mut qb = QueryBuilder::<Postgres>::new(format!(
            "SELECT c.id, c.number::float8 AS number, c.title, {}, c.release_group, e.language, \
             s.id AS series_id, s.title AS series_title, s.kind, s.age_restricted, s.cover_key \
             FROM reading_chapters c JOIN reading_series s ON s.id = c.series_id \
             JOIN reading_editions e ON e.id = c.edition_id \
             WHERE c.published AND s.published AND s.merged_into IS NULL",
            ms("c.published_at", "published_at_ms"),
        ));
        if !include_restricted {
            qb.push(" AND NOT s.age_restricted");
        }
        if let Some(u) = followed_by {
            qb.push(" AND EXISTS (SELECT 1 FROM reading_follows f WHERE f.series_id = s.id AND f.user_id = ")
                .push_bind(u)
                .push(")");
        }
        if let Some((at, id)) = before {
            qb.push(" AND (c.published_at, c.id) < (timestamptz 'epoch' + ")
                .push_bind(at)
                .push(" * interval '1 millisecond', ")
                .push_bind(id)
                .push(")");
        }
        qb.push(" ORDER BY c.published_at DESC, c.id DESC LIMIT ")
            .push_bind(i64::from(limit));
        let rows = qb.build().fetch_all(&self.pool).await.map_err(db)?;
        rows.iter()
            .map(|r| {
                let kind: String = r.try_get("kind").map_err(db)?;
                Ok(UpdateRecord {
                    chapter_id: r.try_get("id").map_err(db)?,
                    chapter_number: r.try_get("number").map_err(db)?,
                    chapter_title: r.try_get("title").map_err(db)?,
                    published_at_ms: r.try_get("published_at_ms").map_err(db)?,
                    release_group: r.try_get("release_group").map_err(db)?,
                    language: r.try_get("language").map_err(db)?,
                    series_id: r.try_get("series_id").map_err(db)?,
                    series_title: r.try_get("series_title").map_err(db)?,
                    kind: SeriesKind::parse(&kind)
                        .ok_or_else(|| LibraryError::Storage(format!("kind {kind}")))?,
                    age_restricted: r.try_get("age_restricted").map_err(db)?,
                    cover_key: r.try_get("cover_key").map_err(db)?,
                })
            })
            .collect()
    }

    async fn get_follow(&self, user: i64, series: i64) -> LibraryResult<Option<Follow>> {
        let row = dynq(format!(
            "SELECT shelf, notify, {} FROM reading_follows WHERE user_id = $1 AND series_id = $2",
            ms("created_at", "follow_created_ms")
        ))
        .bind(user)
        .bind(series)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(follow_from).transpose()
    }

    async fn set_follow(
        &self,
        user: i64,
        series: i64,
        shelf: Shelf,
        notify: bool,
    ) -> LibraryResult<Follow> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row = dynq(format!(
            "INSERT INTO reading_follows (user_id, series_id, shelf, notify) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (user_id, series_id) DO UPDATE SET shelf = EXCLUDED.shelf, notify = EXCLUDED.notify \
             RETURNING shelf, notify, {}",
            ms("created_at", "follow_created_ms")
        ))
        .bind(user)
        .bind(series)
        .bind(shelf.as_str())
        .bind(notify)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| match code(&e).as_deref() {
            Some("23503") => LibraryError::NotFound("series"),
            _ => db(e),
        })?;
        refresh_stats(&mut tx, series).await?;
        tx.commit().await.map_err(db)?;
        follow_from(&row)
    }

    async fn remove_follow(&self, user: i64, series: i64) -> LibraryResult<bool> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let n = sqlx::query("DELETE FROM reading_follows WHERE user_id = $1 AND series_id = $2")
            .bind(user)
            .bind(series)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();
        if n > 0 {
            refresh_stats(&mut tx, series).await?;
        }
        tx.commit().await.map_err(db)?;
        Ok(n > 0)
    }

    async fn list_follows(
        &self,
        user: i64,
        shelf: Option<Shelf>,
        include_restricted: bool,
        limit: u32,
        offset: u32,
    ) -> LibraryResult<Rows<(SeriesRecord, Follow)>> {
        let mut qb = QueryBuilder::<Postgres>::new(format!(
            "SELECT {}, f.shelf, f.notify, {}, count(*) OVER () AS total \
             FROM reading_follows f JOIN reading_series s ON s.id = f.series_id WHERE f.user_id = ",
            series_cols("s"),
            ms("f.created_at", "follow_created_ms"),
        ));
        qb.push_bind(user);
        push_scope(&mut qb, Scope::Public);
        if !include_restricted {
            qb.push(" AND NOT s.age_restricted");
        }
        if let Some(sh) = shelf {
            qb.push(" AND f.shelf = ").push_bind(sh.as_str());
        }
        qb.push(" ORDER BY f.created_at DESC, s.id DESC LIMIT ")
            .push_bind(i64::from(limit))
            .push(" OFFSET ")
            .push_bind(i64::from(offset));
        let rows = qb.build().fetch_all(&self.pool).await.map_err(db)?;
        let total = match rows.first() {
            Some(r) => r.try_get("total").map_err(db)?,
            None => 0,
        };
        let items = rows
            .iter()
            .map(|r| Ok((series_from(r)?, follow_from(r)?)))
            .collect::<LibraryResult<Vec<_>>>()?;
        Ok(Rows { items, total })
    }

    async fn create_series(&self, s: &NewSeries) -> LibraryResult<SeriesRecord> {
        let i = &s.input;
        let sql = format!(
            "INSERT INTO reading_series AS s (id, kind, title, alt_titles, authors, artists, status, \
             year, synopsis, genres, tags, reading_direction, age_restricted, cover_key, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) RETURNING {}",
            series_cols("s")
        );
        let row = dynq(sql)
            .bind(s.id)
            .bind(i.kind.as_str())
            .bind(&i.title)
            .bind(&i.alt_titles)
            .bind(&i.authors)
            .bind(&i.artists)
            .bind(i.status.as_str())
            .bind(i.year)
            .bind(&i.synopsis)
            .bind(&i.genres)
            .bind(&i.tags)
            .bind(s.direction.as_str())
            .bind(i.age_restricted)
            .bind(&i.cover_key)
            .bind(s.created_by)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        series_from(&row)
    }

    async fn update_series(
        &self,
        id: i64,
        i: &SeriesInput,
        direction: ReadingDirection,
    ) -> LibraryResult<Option<SeriesRecord>> {
        let sql = format!(
            "UPDATE reading_series AS s SET kind = $2, title = $3, alt_titles = $4, authors = $5, \
             artists = $6, status = $7, year = $8, synopsis = $9, genres = $10, tags = $11, \
             reading_direction = $12, age_restricted = $13, cover_key = $14, updated_at = now() \
             WHERE s.id = $1 RETURNING {}",
            series_cols("s")
        );
        let row = dynq(sql)
            .bind(id)
            .bind(i.kind.as_str())
            .bind(&i.title)
            .bind(&i.alt_titles)
            .bind(&i.authors)
            .bind(&i.artists)
            .bind(i.status.as_str())
            .bind(i.year)
            .bind(&i.synopsis)
            .bind(&i.genres)
            .bind(&i.tags)
            .bind(direction.as_str())
            .bind(i.age_restricted)
            .bind(&i.cover_key)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        row.as_ref().map(series_from).transpose()
    }

    async fn set_series_published(
        &self,
        id: i64,
        published: bool,
    ) -> LibraryResult<Option<SeriesRecord>> {
        let sql = format!(
            "UPDATE reading_series AS s SET published = $2, updated_at = now(), \
             published_at = CASE WHEN $2 THEN COALESCE(s.published_at, date_trunc('milliseconds', now())) \
                                 ELSE s.published_at END \
             WHERE s.id = $1 AND s.merged_into IS NULL RETURNING {}",
            series_cols("s")
        );
        let row = dynq(sql)
            .bind(id)
            .bind(published)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        row.as_ref().map(series_from).transpose()
    }

    async fn delete_series(&self, id: i64) -> LibraryResult<bool> {
        let n = sqlx::query("DELETE FROM reading_series WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db)?
            .rows_affected();
        Ok(n > 0)
    }

    async fn merge_series(&self, source: i64, target: i64) -> LibraryResult<SeriesRecord> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        // Lock in id order so two concurrent merges cannot deadlock.
        let rows = sqlx::query(
            "SELECT id, merged_into FROM reading_series WHERE id = ANY($1) ORDER BY id FOR UPDATE",
        )
        .bind(vec![source, target])
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        if rows.len() != 2 {
            return Err(LibraryError::NotFound("series"));
        }
        for r in &rows {
            let merged: Option<i64> = r.try_get("merged_into").map_err(db)?;
            if merged.is_some() {
                return Err(LibraryError::Conflict(
                    "a series that was already merged cannot take part in a merge".into(),
                ));
            }
        }
        sqlx::query(
            "UPDATE reading_editions e SET series_id = $2, is_default = FALSE, \
               label = CASE WHEN EXISTS (SELECT 1 FROM reading_editions x \
                  WHERE x.series_id = $2 AND x.language = e.language AND x.label = e.label) \
                  THEN left(e.label, 100) || ' (' || e.id || ')' ELSE e.label END \
             WHERE e.series_id = $1",
        )
        .bind(source)
        .bind(target)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        ensure_default_edition(&mut tx, target).await?;
        sqlx::query("UPDATE reading_chapters SET series_id = $2 WHERE series_id = $1")
            .bind(source)
            .bind(target)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query(
            "INSERT INTO reading_follows (user_id, series_id, shelf, notify, created_at) \
             SELECT user_id, $2, shelf, notify, created_at FROM reading_follows WHERE series_id = $1 \
             ON CONFLICT DO NOTHING",
        )
        .bind(source)
        .bind(target)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query("DELETE FROM reading_follows WHERE series_id = $1")
            .bind(source)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("UPDATE reading_progress SET series_id = $2 WHERE series_id = $1")
            .bind(source)
            .bind(target)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query(
            "UPDATE reading_series SET published = FALSE, merged_into = $2, updated_at = now() WHERE id = $1",
        )
        .bind(source)
        .bind(target)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        refresh_stats(&mut tx, source).await?;
        refresh_stats(&mut tx, target).await?;
        let rec = fetch_series(&mut tx, target)
            .await?
            .ok_or(LibraryError::NotFound("series"))?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn create_edition(
        &self,
        id: i64,
        series: i64,
        input: &EditionInput,
    ) -> LibraryResult<Edition> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        if input.is_default {
            sqlx::query("UPDATE reading_editions SET is_default = FALSE WHERE series_id = $1")
                .bind(series)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        let row = sqlx::query(
            "INSERT INTO reading_editions (id, series_id, language, label, is_default) \
             VALUES ($1, $2, $3, $4, $5) RETURNING id, series_id, language, label, is_default",
        )
        .bind(id)
        .bind(series)
        .bind(&input.language)
        .bind(&input.label)
        .bind(input.is_default)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| match code(&e).as_deref() {
            Some("23503") => LibraryError::NotFound("series"),
            Some("23505") => LibraryError::Conflict(
                "the series already has an edition with this language and label".into(),
            ),
            _ => db(e),
        })?;
        ensure_default_edition(&mut tx, series).await?;
        tx.commit().await.map_err(db)?;
        // `is_default` may have been forced on by `ensure_default_edition`.
        self.get_edition(id)
            .await?
            .map_or_else(|| edition_from(&row), Ok)
    }

    async fn update_edition(
        &self,
        id: i64,
        input: &EditionInput,
    ) -> LibraryResult<Option<Edition>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let series: Option<i64> =
            sqlx::query_scalar("SELECT series_id FROM reading_editions WHERE id = $1 FOR UPDATE")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        let Some(series) = series else {
            return Ok(None);
        };
        if input.is_default {
            sqlx::query(
                "UPDATE reading_editions SET is_default = FALSE WHERE series_id = $1 AND id <> $2",
            )
            .bind(series)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        sqlx::query(
            "UPDATE reading_editions SET language = $2, label = $3, \
             is_default = ($4 OR is_default), updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(&input.language)
        .bind(&input.label)
        .bind(input.is_default)
        .execute(&mut *tx)
        .await
        .map_err(|e| match code(&e).as_deref() {
            Some("23505") => LibraryError::Conflict(
                "the series already has an edition with this language and label".into(),
            ),
            _ => db(e),
        })?;
        ensure_default_edition(&mut tx, series).await?;
        tx.commit().await.map_err(db)?;
        self.get_edition(id).await
    }

    async fn get_edition(&self, id: i64) -> LibraryResult<Option<Edition>> {
        let row = sqlx::query(
            "SELECT id, series_id, language, label, is_default FROM reading_editions WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(edition_from).transpose()
    }

    async fn delete_edition(&self, id: i64) -> LibraryResult<bool> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let series: Option<i64> =
            sqlx::query_scalar("DELETE FROM reading_editions WHERE id = $1 RETURNING series_id")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        let Some(series) = series else {
            return Ok(false);
        };
        ensure_default_edition(&mut tx, series).await?;
        refresh_stats(&mut tx, series).await?;
        tx.commit().await.map_err(db)?;
        Ok(true)
    }

    async fn create_volume(
        &self,
        id: i64,
        edition: i64,
        input: &VolumeInput,
    ) -> LibraryResult<Volume> {
        let row = sqlx::query(
            "INSERT INTO reading_volumes (id, edition_id, number, title, cover_key) \
             VALUES ($1, $2, $3::float8, $4, $5) \
             RETURNING id, edition_id, number::float8 AS number, title",
        )
        .bind(id)
        .bind(edition)
        .bind(input.number)
        .bind(&input.title)
        .bind(&input.cover_key)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match code(&e).as_deref() {
            Some("23503") => LibraryError::NotFound("edition"),
            Some("23505") => LibraryError::Conflict("the edition already has this volume".into()),
            _ => db(e),
        })?;
        Ok(Volume {
            id: row.try_get("id").map_err(db)?,
            edition_id: row.try_get("edition_id").map_err(db)?,
            number: row.try_get("number").map_err(db)?,
            title: row.try_get("title").map_err(db)?,
        })
    }

    async fn delete_volume(&self, id: i64) -> LibraryResult<bool> {
        let n = sqlx::query("DELETE FROM reading_volumes WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db)?
            .rows_affected();
        Ok(n > 0)
    }

    async fn create_chapter(
        &self,
        id: i64,
        edition: i64,
        input: &ChapterInput,
        created_by: i64,
    ) -> LibraryResult<ChapterRecord> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let series: Option<i64> =
            sqlx::query_scalar("SELECT series_id FROM reading_editions WHERE id = $1")
                .bind(edition)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        let series = series.ok_or(LibraryError::NotFound("edition"))?;
        volume_belongs(&mut tx, input.volume_id, edition).await?;
        sqlx::query(
            "INSERT INTO reading_chapters (id, series_id, edition_id, volume_id, number, title, format, \
             source_key, release_group, created_by) \
             VALUES ($1, $2, $3, $4, $5::float8, $6, $7, $8, $9, $10)",
        )
        .bind(id)
        .bind(series)
        .bind(edition)
        .bind(input.volume_id)
        .bind(input.number)
        .bind(&input.title)
        .bind(input.format.as_str())
        .bind(&input.source_key)
        .bind(&input.release_group)
        .bind(created_by)
        .execute(&mut *tx)
        .await
        .map_err(|e| match code(&e).as_deref() {
            Some("23505") => {
                LibraryError::Conflict("the edition already has a chapter with this number".into())
            }
            _ => db(e),
        })?;
        let rec = fetch_chapter(&mut tx, id, None)
            .await?
            .ok_or(LibraryError::NotFound("chapter"))?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn update_chapter(
        &self,
        id: i64,
        input: &ChapterInput,
    ) -> LibraryResult<Option<ChapterRecord>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let edition: Option<i64> =
            sqlx::query_scalar("SELECT edition_id FROM reading_chapters WHERE id = $1 FOR UPDATE")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        let Some(edition) = edition else {
            return Ok(None);
        };
        volume_belongs(&mut tx, input.volume_id, edition).await?;
        sqlx::query(
            "UPDATE reading_chapters SET number = $2::float8, title = $3, volume_id = $4, format = $5, \
             source_key = $6, release_group = $7, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(input.number)
        .bind(&input.title)
        .bind(input.volume_id)
        .bind(input.format.as_str())
        .bind(&input.source_key)
        .bind(&input.release_group)
        .execute(&mut *tx)
        .await
        .map_err(|e| match code(&e).as_deref() {
            Some("23505") => {
                LibraryError::Conflict("the edition already has a chapter with this number".into())
            }
            _ => db(e),
        })?;
        let rec = fetch_chapter(&mut tx, id, None).await?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn set_chapter_published(
        &self,
        id: i64,
        published: bool,
    ) -> LibraryResult<Option<ChapterRecord>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let series: Option<i64> = sqlx::query_scalar(
            "UPDATE reading_chapters SET published = $2, updated_at = now(), \
             published_at = CASE WHEN $2 THEN COALESCE(published_at, date_trunc('milliseconds', now())) \
                                 ELSE published_at END \
             WHERE id = $1 RETURNING series_id",
        )
        .bind(id)
        .bind(published)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let Some(series) = series else {
            return Ok(None);
        };
        refresh_stats(&mut tx, series).await?;
        let rec = fetch_chapter(&mut tx, id, None).await?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn delete_chapter(&self, id: i64) -> LibraryResult<bool> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let series: Option<i64> =
            sqlx::query_scalar("DELETE FROM reading_chapters WHERE id = $1 RETURNING series_id")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        let Some(series) = series else {
            return Ok(false);
        };
        refresh_stats(&mut tx, series).await?;
        tx.commit().await.map_err(db)?;
        Ok(true)
    }

    async fn replace_pages(
        &self,
        chapter: i64,
        pages: &[PageRecord],
    ) -> LibraryResult<Option<ChapterRecord>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let locked: Option<i64> =
            sqlx::query_scalar("SELECT id FROM reading_chapters WHERE id = $1 FOR UPDATE")
                .bind(chapter)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        if locked.is_none() {
            return Ok(None);
        }
        sqlx::query("DELETE FROM reading_pages WHERE chapter_id = $1")
            .bind(chapter)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        if !pages.is_empty() {
            let idx: Vec<i32> = pages.iter().map(|p| p.index).collect();
            let keys: Vec<String> = pages.iter().map(|p| p.key.clone()).collect();
            let variants: Vec<String> = pages
                .iter()
                .map(|p| {
                    let m: serde_json::Map<String, serde_json::Value> = p
                        .variants
                        .iter()
                        .map(|(w, k)| (w.to_string(), serde_json::Value::String(k.clone())))
                        .collect();
                    serde_json::Value::Object(m).to_string()
                })
                .collect();
            let widths: Vec<i32> = pages.iter().map(|p| p.width).collect();
            let heights: Vec<i32> = pages.iter().map(|p| p.height).collect();
            let spreads: Vec<bool> = pages.iter().map(|p| p.is_double_spread).collect();
            let hashes: Vec<Option<String>> = pages.iter().map(|p| p.blurhash.clone()).collect();
            sqlx::query(
                "INSERT INTO reading_pages (chapter_id, idx, key, variants, width, height, is_double_spread, blurhash) \
                 SELECT $1, t.idx, t.key, t.variants::jsonb, t.width, t.height, t.spread, t.blurhash \
                 FROM UNNEST($2::int[], $3::text[], $4::text[], $5::int[], $6::int[], $7::bool[], $8::text[]) \
                   AS t(idx, key, variants, width, height, spread, blurhash)",
            )
            .bind(chapter)
            .bind(idx)
            .bind(keys)
            .bind(variants)
            .bind(widths)
            .bind(heights)
            .bind(spreads)
            .bind(hashes)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        sqlx::query(
            "UPDATE reading_chapters SET page_count = $2, updated_at = now() WHERE id = $1",
        )
        .bind(chapter)
        .bind(pages.len() as i32)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        let rec = fetch_chapter(&mut tx, chapter, None).await?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn index_batch(&self, after: i64, limit: u32) -> LibraryResult<Vec<SeriesRecord>> {
        let sql = format!(
            "SELECT {} FROM reading_series s WHERE s.published AND s.merged_into IS NULL AND s.id > $1 \
             ORDER BY s.id LIMIT $2",
            series_cols("s")
        );
        let rows = dynq(sql)
            .bind(after)
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(series_from).collect()
    }
}

/// A query built from a string assembled in this file out of constants and column lists;
/// every value still travels as a bind parameter.
fn dynq(
    sql: String,
) -> sqlx::query::Query<'static, Postgres, <Postgres as sqlx::Database>::Arguments> {
    sqlx::query(sqlx::AssertSqlSafe(sql))
}
