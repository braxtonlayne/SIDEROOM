//! Postgres side of the catalog indexer (BYOS-CAT-001, migration 1450): `storage_objects`, the run ledger and the
//! catalog rows built from a mount (`source = 'mount'`).
//!
//! Rules this file keeps:
//! - Rows are written with `published = FALSE`, so no existing member query sees them; the shelf queries
//!   (`shelf_repo`) read them by mount.
//! - A field named in the row's `overridden` array is never rewritten by a re-index (an edit is an override).
//! - Series are anchored by `(mount_id, shelf_key)`, episodes, chapters, tracks and films by `mount_object_id`, so
//!   a rename in the editor never makes a duplicate.
//! - A run that listed the whole mount hides what it did not see (`shelf_state = 'vanished'`); a partial run hides
//!   nothing. A row whose object comes back is listed again.

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::json;
use sqlx::{PgPool, Postgres, Row, Transaction};
use sr_storage::index::{
    Class, Classified, IndexControl, IndexStore, JOB_KIND, RunReport, RunStatus, StoreError,
};

/// Source of snowflake ids.
pub type IdSource = std::sync::Arc<dyn Fn() -> i64 + Send + Sync>;

/// Postgres-backed [`IndexStore`].
#[derive(Clone)]
pub struct PgIndexStore {
    pool: PgPool,
    ids: IdSource,
}

impl PgIndexStore {
    /// Wrap a pool and an id source.
    pub fn new(pool: PgPool, ids: IdSource) -> Self {
        Self { pool, ids }
    }
}

fn se(e: sqlx::Error) -> StoreError {
    StoreError(e.to_string())
}

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// The title sort key: lower-case without a leading article (the same rule as `sr_cinema::rules::sort_title`).
fn sort_title(title: &str) -> String {
    let low = title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    for article in ["the ", "an ", "a "] {
        if let Some(rest) = low.strip_prefix(article)
            && !rest.is_empty()
        {
            return rest.to_owned();
        }
    }
    low
}

fn video_year(y: Option<u16>) -> Option<i16> {
    y.filter(|y| (1880..=2200).contains(y))
        .and_then(|y| i16::try_from(y).ok())
}

fn reading_year(y: Option<u16>) -> Option<i16> {
    y.filter(|y| (1800..=2200).contains(y))
        .and_then(|y| i16::try_from(y).ok())
}

fn parsed_json(c: &Classified) -> serde_json::Value {
    json!({
        "title": c.title, "year": c.year, "series": c.series, "shelf_key": c.shelf_key,
        "season": c.season, "episodes": c.episodes, "episode_title": c.episode_title,
        "absolute": c.absolute, "special": c.special, "variant": c.variant, "number": c.number,
        "artist": c.artist, "album": c.album, "track_no": c.track_no, "disc_no": c.disc_no,
        "anime": c.anime, "tags": c.tags, "cover": c.cover_key, "backdrop": c.backdrop_key,
    })
}

#[async_trait]
impl IndexStore for PgIndexStore {
    async fn begin(&self, mount_id: i64, _now_ms: i64) -> Result<i64, StoreError> {
        // One run per mount at a time; a run that died is taken over after 30 minutes.
        let run: Option<i64> = sqlx::query_scalar(
            "INSERT INTO storage_index_runs (mount_id, run, state, started_at) \
             VALUES ($1, 1, 'running', now()) \
             ON CONFLICT (mount_id) DO UPDATE SET run = storage_index_runs.run + 1, state = 'running', \
                 started_at = now(), finished_at = NULL, error = NULL, objects = 0, added = 0, \
                 vanished = 0, truncated = FALSE \
             WHERE storage_index_runs.state <> 'running' \
                OR storage_index_runs.started_at < now() - interval '30 minutes' \
             RETURNING run",
        )
        .bind(mount_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(se)?;
        run.ok_or_else(|| StoreError("an index run is already in progress for this mount".into()))
    }

    async fn apply(
        &self,
        mount_id: i64,
        run: i64,
        batch: &[Classified],
    ) -> Result<u64, StoreError> {
        let mut tx = self.pool.begin().await.map_err(se)?;
        let keys: Vec<String> = batch.iter().map(|c| c.key.clone()).collect();
        let rows = sqlx::query(
            "SELECT key, id, size_bytes, class, parsed, state FROM storage_objects \
             WHERE mount_id = $1 AND key = ANY($2)",
        )
        .bind(mount_id)
        .bind(&keys)
        .fetch_all(&mut *tx)
        .await
        .map_err(se)?;
        struct Old {
            id: i64,
            size: i64,
            class: String,
            parsed: serde_json::Value,
            state: String,
        }
        let mut old: HashMap<String, Old> = HashMap::new();
        for r in &rows {
            old.insert(
                r.try_get("key").map_err(se)?,
                Old {
                    id: r.try_get("id").map_err(se)?,
                    size: r.try_get("size_bytes").map_err(se)?,
                    class: r.try_get("class").map_err(se)?,
                    parsed: r.try_get("parsed").map_err(se)?,
                    state: r.try_get("state").map_err(se)?,
                },
            );
        }
        let mut changed_n = 0u64;
        for c in batch {
            let parsed = parsed_json(c);
            let size = i64::try_from(c.size).unwrap_or(i64::MAX);
            let (object_id, changed) = match old.get(&c.key) {
                Some(o) => {
                    let changed = o.size != size
                        || o.class != c.class.as_str()
                        || o.parsed != parsed
                        || o.state != "indexed";
                    (o.id, changed)
                }
                None => ((self.ids)(), true),
            };
            sqlx::query(
                "INSERT INTO storage_objects (id, mount_id, key, size_bytes, kind, class, container, \
                    web_native, parsed, state, seen_run) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'indexed', $10) \
                 ON CONFLICT (mount_id, key) DO UPDATE SET size_bytes = EXCLUDED.size_bytes, \
                    kind = EXCLUDED.kind, class = EXCLUDED.class, container = EXCLUDED.container, \
                    web_native = EXCLUDED.web_native, parsed = EXCLUDED.parsed, state = 'indexed', \
                    seen_run = EXCLUDED.seen_run, vanished_at = NULL, \
                    indexed_at = CASE WHEN $11 THEN now() ELSE storage_objects.indexed_at END",
            )
            .bind(object_id)
            .bind(mount_id)
            .bind(&c.key)
            .bind(size)
            .bind(c.class.kind().map(|k| k.as_str()))
            .bind(c.class.as_str())
            .bind(&c.container)
            .bind(c.web_native)
            .bind(&parsed)
            .bind(run)
            .bind(changed)
            .execute(&mut *tx)
            .await
            .map_err(se)?;
            if changed || !old.contains_key(&c.key) {
                changed_n += 1;
            }
            // The catalog rows are rebuilt for every seen object: cheap, and it relists rows a returning object
            // had hidden. Overridden fields are protected inside each statement.
            self.catalog(&mut tx, mount_id, object_id, c).await?;
        }
        tx.commit().await.map_err(se)?;
        Ok(changed_n)
    }

    async fn finish(
        &self,
        mount_id: i64,
        run: i64,
        report: &RunReport,
        complete: bool,
        error: Option<&str>,
        _now_ms: i64,
    ) -> Result<u64, StoreError> {
        let mut tx = self.pool.begin().await.map_err(se)?;
        let mut gone = 0u64;
        if complete && error.is_none() {
            let ids: Vec<i64> = sqlx::query_scalar(
                "UPDATE storage_objects SET state = 'vanished', vanished_at = now() \
                 WHERE mount_id = $1 AND state = 'indexed' AND seen_run < $2 RETURNING id",
            )
            .bind(mount_id)
            .bind(run)
            .fetch_all(&mut *tx)
            .await
            .map_err(se)?;
            gone = ids.len() as u64;
            if !ids.is_empty() {
                for t in [
                    "video_titles",
                    "video_episodes",
                    "reading_chapters",
                    "music_tracks",
                ] {
                    let sql = format!(
                        "UPDATE {t} SET shelf_state = 'vanished' WHERE mount_object_id = ANY($1)"
                    );
                    sqlx::query(sqlx::AssertSqlSafe(sql))
                        .bind(&ids)
                        .execute(&mut *tx)
                        .await
                        .map_err(se)?;
                }
            }
            // A series with nothing left listed is hidden with its last episode or chapter.
            sqlx::query(
                "UPDATE video_titles t SET shelf_state = CASE WHEN EXISTS ( \
                     SELECT 1 FROM video_episodes e WHERE e.title_id = t.id AND e.shelf_state = 'listed') \
                     THEN 'listed' ELSE 'vanished' END \
                 WHERE t.mount_id = $1 AND t.source = 'mount' AND t.kind IN ('series', 'anime') \
                   AND t.mount_object_id IS NULL",
            )
            .bind(mount_id)
            .execute(&mut *tx)
            .await
            .map_err(se)?;
            sqlx::query(
                "UPDATE reading_series s SET shelf_state = CASE WHEN EXISTS ( \
                     SELECT 1 FROM reading_chapters c WHERE c.series_id = s.id AND c.shelf_state = 'listed') \
                     THEN 'listed' ELSE 'vanished' END \
                 WHERE s.mount_id = $1 AND s.source = 'mount'",
            )
            .bind(mount_id)
            .execute(&mut *tx)
            .await
            .map_err(se)?;
        }
        sqlx::query(
            "UPDATE storage_index_runs SET state = $3, finished_at = now(), objects = $4, added = $5, \
                 vanished = $6, truncated = $7, error = $8 \
             WHERE mount_id = $1 AND run = $2",
        )
        .bind(mount_id)
        .bind(run)
        .bind(if error.is_some() { "failed" } else { "done" })
        .bind(i32::try_from(report.objects).unwrap_or(i32::MAX))
        .bind(i32::try_from(report.added).unwrap_or(i32::MAX))
        .bind(i32::try_from(gone).unwrap_or(i32::MAX))
        .bind(report.truncated)
        .bind(error.map(|e| clip(e, 500)))
        .execute(&mut *tx)
        .await
        .map_err(se)?;
        tx.commit().await.map_err(se)?;
        Ok(gone)
    }
}

impl PgIndexStore {
    async fn catalog(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        mount_id: i64,
        object_id: i64,
        c: &Classified,
    ) -> Result<(), StoreError> {
        match c.class {
            Class::Movie => self.movie(tx, mount_id, object_id, c).await,
            Class::Episode => self.episode(tx, mount_id, object_id, c).await,
            Class::Book | Class::Comic | Class::Manga => {
                self.chapter(tx, mount_id, object_id, c).await
            }
            Class::Track => self.track(tx, mount_id, object_id, c).await,
            _ => Ok(()),
        }
    }

    /// Insert or update a title row; returns its id. `anchor` is how an existing row is found.
    #[allow(clippy::too_many_arguments)]
    async fn title(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        mount_id: i64,
        kind: &str,
        c: &Classified,
        shelf_key: &str,
        object_id: Option<i64>,
    ) -> Result<i64, StoreError> {
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM video_titles WHERE source = 'mount' AND mount_id = $1 AND shelf_key = $2",
        )
        .bind(mount_id)
        .bind(shelf_key)
        .fetch_optional(&mut **tx)
        .await
        .map_err(se)?;
        let title = clip(c.series.as_deref().unwrap_or(&c.title), 300);
        let tags: Vec<String> = c.tags.clone();
        if let Some(id) = existing {
            sqlx::query(
                "UPDATE video_titles SET \
                   title = CASE WHEN 'title' = ANY(overridden) THEN title ELSE $2 END, \
                   sort_title = CASE WHEN 'title' = ANY(overridden) THEN sort_title ELSE $3 END, \
                   year = CASE WHEN 'year' = ANY(overridden) THEN year ELSE $4 END, \
                   tags = CASE WHEN 'tags' = ANY(overridden) THEN tags ELSE $5 END, \
                   kind = CASE WHEN 'kind' = ANY(overridden) THEN kind ELSE $6 END, \
                   poster_key = CASE WHEN 'poster' = ANY(overridden) THEN poster_key \
                                     ELSE COALESCE($7, poster_key) END, \
                   backdrop_key = CASE WHEN 'backdrop' = ANY(overridden) THEN backdrop_key \
                                       ELSE COALESCE($8, backdrop_key) END, \
                   shelf_state = 'listed', updated_at = now() \
                 WHERE id = $1",
            )
            .bind(id)
            .bind(&title)
            .bind(sort_title(&title))
            .bind(video_year(c.year))
            .bind(&tags)
            .bind(kind)
            .bind(c.cover_key.as_deref())
            .bind(c.backdrop_key.as_deref())
            .execute(&mut **tx)
            .await
            .map_err(se)?;
            return Ok(id);
        }
        let id = (self.ids)();
        sqlx::query(
            "INSERT INTO video_titles (id, kind, title, sort_title, year, tags, poster_key, backdrop_key, \
                published, mount_id, source, mount_object_id, shelf_key, shelf_state, last_content_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, FALSE, $9, 'mount', $10, $11, 'listed', now())",
        )
        .bind(id)
        .bind(kind)
        .bind(&title)
        .bind(sort_title(&title))
        .bind(video_year(c.year))
        .bind(&tags)
        .bind(c.cover_key.as_deref())
        .bind(c.backdrop_key.as_deref())
        .bind(mount_id)
        .bind(object_id)
        .bind(shelf_key)
        .execute(&mut **tx)
        .await
        .map_err(se)?;
        Ok(id)
    }

    async fn movie(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        mount_id: i64,
        object_id: i64,
        c: &Classified,
    ) -> Result<(), StoreError> {
        let kind = if c.anime { "anime" } else { "movie" };
        // A film is its own group: the shelf key is its object key.
        let mut m = c.clone();
        m.series = None;
        self.title(tx, mount_id, kind, &m, &c.key, Some(object_id))
            .await?;
        Ok(())
    }

    async fn episode(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        mount_id: i64,
        object_id: i64,
        c: &Classified,
    ) -> Result<(), StoreError> {
        let kind = if c.anime { "anime" } else { "series" };
        let key = c
            .shelf_key
            .clone()
            .unwrap_or_else(|| format!("series/{}", c.title.to_lowercase()));
        let title_id = self.title(tx, mount_id, kind, c, &key, None).await?;
        let season_no =
            i32::try_from(if c.special { 0 } else { c.season.unwrap_or(1) }).unwrap_or(1);
        let season_id: i64 = match sqlx::query_scalar::<_, i64>(
            "SELECT id FROM video_seasons WHERE title_id = $1 AND number = $2",
        )
        .bind(title_id)
        .bind(season_no)
        .fetch_optional(&mut **tx)
        .await
        .map_err(se)?
        {
            Some(id) => id,
            None => {
                let id = (self.ids)();
                sqlx::query("INSERT INTO video_seasons (id, title_id, number) VALUES ($1, $2, $3)")
                    .bind(id)
                    .bind(title_id)
                    .bind(season_no)
                    .execute(&mut **tx)
                    .await
                    .map_err(se)?;
                id
            }
        };
        let number = i32::try_from(c.episodes.first().copied().unwrap_or(0)).unwrap_or(0);
        let existing: Option<(i64, i64)> =
            sqlx::query_as("SELECT id, season_id FROM video_episodes WHERE mount_object_id = $1")
                .bind(object_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(se)?;
        let ep_title = clip(c.episode_title.as_deref().unwrap_or(""), 300);
        let absolute = c.absolute.then_some(number);
        if let Some((id, _)) = existing {
            sqlx::query(
                "UPDATE video_episodes SET title_id = $2, season_id = $3, \
                   number = CASE WHEN 'number' = ANY(overridden) THEN number ELSE $4 END, \
                   absolute_number = CASE WHEN 'number' = ANY(overridden) THEN absolute_number ELSE $5 END, \
                   title = CASE WHEN 'title' = ANY(overridden) THEN title ELSE $6 END, \
                   shelf_state = 'listed', updated_at = now() WHERE id = $1",
            )
            .bind(id)
            .bind(title_id)
            .bind(season_id)
            .bind(number)
            .bind(absolute)
            .bind(&ep_title)
            .execute(&mut **tx)
            .await
            .map_err(se)?;
            return Ok(());
        }
        // Two files for one episode number (two qualities): the first indexed keeps the slot.
        let taken: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM video_episodes WHERE season_id = $1 AND number = $2)",
        )
        .bind(season_id)
        .bind(number)
        .fetch_one(&mut **tx)
        .await
        .map_err(se)?;
        if taken {
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO video_episodes (id, title_id, season_id, number, absolute_number, title, published, \
                mount_id, source, mount_object_id, shelf_state) \
             VALUES ($1, $2, $3, $4, $5, $6, FALSE, $7, 'mount', $8, 'listed')",
        )
        .bind((self.ids)())
        .bind(title_id)
        .bind(season_id)
        .bind(number)
        .bind(absolute)
        .bind(&ep_title)
        .bind(mount_id)
        .bind(object_id)
        .execute(&mut **tx)
        .await
        .map_err(se)?;
        Ok(())
    }

    async fn chapter(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        mount_id: i64,
        object_id: i64,
        c: &Classified,
    ) -> Result<(), StoreError> {
        let kind = match c.class {
            Class::Manga => "manga",
            Class::Comic => "comic",
            _ => "book",
        };
        let shelf_key = c
            .shelf_key
            .clone()
            .unwrap_or_else(|| format!("{kind}/{}", c.title.to_lowercase()));
        let series_title = clip(c.series.as_deref().unwrap_or(&c.title), 300);
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM reading_series WHERE source = 'mount' AND mount_id = $1 AND shelf_key = $2",
        )
        .bind(mount_id)
        .bind(&shelf_key)
        .fetch_optional(&mut **tx)
        .await
        .map_err(se)?;
        let series_id = match existing {
            Some(id) => {
                sqlx::query(
                    "UPDATE reading_series SET \
                       title = CASE WHEN 'title' = ANY(overridden) THEN title ELSE $2 END, \
                       year = CASE WHEN 'year' = ANY(overridden) THEN year ELSE $3 END, \
                       kind = CASE WHEN 'kind' = ANY(overridden) THEN kind ELSE $4 END, \
                       cover_key = CASE WHEN 'poster' = ANY(overridden) THEN cover_key \
                                        ELSE COALESCE($5, cover_key) END, \
                       shelf_state = 'listed', updated_at = now() WHERE id = $1",
                )
                .bind(id)
                .bind(&series_title)
                .bind(reading_year(c.year))
                .bind(kind)
                .bind(c.cover_key.as_deref())
                .execute(&mut **tx)
                .await
                .map_err(se)?;
                id
            }
            None => {
                let id = (self.ids)();
                sqlx::query(
                    "INSERT INTO reading_series (id, kind, title, year, cover_key, reading_direction, published, \
                        mount_id, source, shelf_key, shelf_state) \
                     VALUES ($1, $2, $3, $4, $5, $6, FALSE, $7, 'mount', $8, 'listed')",
                )
                .bind(id)
                .bind(kind)
                .bind(&series_title)
                .bind(reading_year(c.year))
                .bind(c.cover_key.as_deref())
                .bind(if kind == "manga" { "rtl" } else { "ltr" })
                .bind(mount_id)
                .bind(&shelf_key)
                .execute(&mut **tx)
                .await
                .map_err(se)?;
                id
            }
        };
        // One default edition per shelf series.
        let edition_id: i64 = match sqlx::query_scalar::<_, i64>(
            "SELECT id FROM reading_editions WHERE series_id = $1 AND is_default",
        )
        .bind(series_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(se)?
        {
            Some(id) => id,
            None => {
                let id = (self.ids)();
                sqlx::query(
                    "INSERT INTO reading_editions (id, series_id, language, label, is_default) \
                     VALUES ($1, $2, 'en', 'Shelf', TRUE)",
                )
                .bind(id)
                .bind(series_id)
                .execute(&mut **tx)
                .await
                .map_err(se)?;
                id
            }
        };
        let format = match c.container.as_str() {
            "pdf" => "pdf",
            "epub" => "epub",
            _ => "images",
        };
        let chapter_title = clip(&c.title, 300);
        let existing_ch: Option<i64> =
            sqlx::query_scalar("SELECT id FROM reading_chapters WHERE mount_object_id = $1")
                .bind(object_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(se)?;
        if let Some(id) = existing_ch {
            sqlx::query(
                "UPDATE reading_chapters SET series_id = $2, edition_id = $3, \
                   title = CASE WHEN 'title' = ANY(overridden) THEN title ELSE $4 END, \
                   source_key = $5, format = $6, shelf_state = 'listed', updated_at = now() WHERE id = $1",
            )
            .bind(id)
            .bind(series_id)
            .bind(edition_id)
            .bind(&chapter_title)
            .bind(&c.key)
            .bind(format)
            .execute(&mut **tx)
            .await
            .map_err(se)?;
            return Ok(());
        }
        // A number already taken by another file moves to the next free one (the editor can fix it).
        let wanted = c.number.unwrap_or(1.0).clamp(0.0, 999_999.0);
        let clash: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM reading_chapters WHERE edition_id = $1 AND number = $2::numeric)",
        )
        .bind(edition_id)
        .bind(wanted)
        .fetch_one(&mut **tx)
        .await
        .map_err(se)?;
        let number: f64 = if clash {
            let max: Option<f64> = sqlx::query_scalar(
                "SELECT max(number)::float8 FROM reading_chapters WHERE edition_id = $1",
            )
            .bind(edition_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(se)?;
            (max.unwrap_or(0.0).floor() + 1.0).min(999_999.0)
        } else {
            wanted
        };
        sqlx::query(
            "INSERT INTO reading_chapters (id, series_id, edition_id, number, title, format, source_key, published, \
                mount_id, source, mount_object_id, shelf_state) \
             VALUES ($1, $2, $3, $4::float8::numeric, $5, $6, $7, FALSE, $8, 'mount', $9, 'listed')",
        )
        .bind((self.ids)())
        .bind(series_id)
        .bind(edition_id)
        .bind(number)
        .bind(&chapter_title)
        .bind(format)
        .bind(&c.key)
        .bind(mount_id)
        .bind(object_id)
        .execute(&mut **tx)
        .await
        .map_err(se)?;
        Ok(())
    }

    async fn track(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        mount_id: i64,
        object_id: i64,
        c: &Classified,
    ) -> Result<(), StoreError> {
        let id = (self.ids)();
        let year = c
            .year
            .filter(|y| (1000..=2200).contains(y))
            .and_then(|y| i16::try_from(y).ok());
        sqlx::query(
            "INSERT INTO music_tracks (id, mount_id, mount_object_id, title, artist, album, track_no, disc_no, \
                year, cover_key) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
             ON CONFLICT (mount_object_id) DO UPDATE SET \
               title = CASE WHEN 'title' = ANY(music_tracks.overridden) THEN music_tracks.title ELSE EXCLUDED.title END, \
               artist = CASE WHEN 'artist' = ANY(music_tracks.overridden) THEN music_tracks.artist ELSE EXCLUDED.artist END, \
               album = CASE WHEN 'album' = ANY(music_tracks.overridden) THEN music_tracks.album ELSE EXCLUDED.album END, \
               track_no = CASE WHEN 'track' = ANY(music_tracks.overridden) THEN music_tracks.track_no ELSE EXCLUDED.track_no END, \
               disc_no = CASE WHEN 'track' = ANY(music_tracks.overridden) THEN music_tracks.disc_no ELSE EXCLUDED.disc_no END, \
               year = CASE WHEN 'year' = ANY(music_tracks.overridden) THEN music_tracks.year ELSE EXCLUDED.year END, \
               cover_key = CASE WHEN 'poster' = ANY(music_tracks.overridden) THEN music_tracks.cover_key \
                                ELSE COALESCE(EXCLUDED.cover_key, music_tracks.cover_key) END, \
               shelf_state = 'listed', updated_at = now()",
        )
        .bind(id)
        .bind(mount_id)
        .bind(object_id)
        .bind(clip(&c.title, 300))
        .bind(clip(&c.artist, 300))
        .bind(clip(&c.album, 300))
        .bind(c.track_no.and_then(|n| i32::try_from(n).ok()).filter(|n| (0..=9999).contains(n)))
        .bind(c.disc_no.and_then(|n| i32::try_from(n).ok()).filter(|n| (0..=999).contains(n)))
        .bind(year)
        .bind(c.cover_key.as_deref())
        .execute(&mut **tx)
        .await
        .map_err(se)?;
        Ok(())
    }
}

#[async_trait]
impl IndexControl for PgIndexStore {
    async fn request(&self, mount_id: i64) -> Result<bool, StoreError> {
        let busy: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM jobs WHERE kind = $1 AND state IN ('queued', 'running') \
                AND payload->>'mount_id' = $2)",
        )
        .bind(JOB_KIND)
        .bind(mount_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(se)?;
        if busy {
            return Ok(false);
        }
        let job = crate::jobs::NewJob::new((self.ids)(), JOB_KIND, json!({ "mount_id": mount_id }));
        crate::jobs::enqueue(&self.pool, &job)
            .await
            .map_err(|e| StoreError(e.to_string()))?;
        Ok(true)
    }

    async fn status(&self, mount_id: i64) -> Result<Option<RunStatus>, StoreError> {
        let r = sqlx::query(
            "SELECT run, state, (extract(epoch FROM started_at) * 1000)::bigint AS started_ms, \
                    (extract(epoch FROM finished_at) * 1000)::bigint AS finished_ms, \
                    objects, added, vanished, truncated, error \
             FROM storage_index_runs WHERE mount_id = $1",
        )
        .bind(mount_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(se)?;
        r.map(|r| {
            Ok(RunStatus {
                run: r.try_get("run").map_err(se)?,
                state: r.try_get("state").map_err(se)?,
                started_ms: r.try_get("started_ms").map_err(se)?,
                finished_ms: r.try_get("finished_ms").map_err(se)?,
                objects: r.try_get("objects").map_err(se)?,
                added: r.try_get("added").map_err(se)?,
                vanished: r.try_get("vanished").map_err(se)?,
                truncated: r.try_get("truncated").map_err(se)?,
                error: r.try_get("error").map_err(se)?,
            })
        })
        .transpose()
    }
}
