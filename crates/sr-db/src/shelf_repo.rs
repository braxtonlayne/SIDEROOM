//! Postgres side of the shelves (BYOS-CAT-002, migration 1450): reads the catalog rows a mount's index built and
//! applies the metadata editor's overrides.
//!
//! The rows are `published = FALSE`, so none of this is reachable from the instance catalog's queries; here they
//! are read by `source = 'mount'`, `shelf_state = 'listed'` and the mounts the caller already proved reachable
//! (`sr_storage::index::shelf::reachable`). An edit writes the field and adds its name to `overridden`, which the
//! indexer honours (`index_repo`).

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder, Row, postgres::PgRow};
use sr_storage::index::StoreError;
use sr_storage::index::shelf::{
    PartPatch, PatchError, Section, ShelfDetail, ShelfFilter, ShelfItem, ShelfPage, ShelfPart,
    ShelfPatch, ShelfStore,
};

/// Postgres-backed [`ShelfStore`].
#[derive(Clone)]
pub struct PgShelfStore {
    ids: crate::index_repo::IdSource,
    pool: PgPool,
}

impl PgShelfStore {
    /// Wrap a pool.
    pub fn new(pool: PgPool, ids: crate::index_repo::IdSource) -> Self {
        Self { pool, ids }
    }
}

fn se(e: sqlx::Error) -> StoreError {
    StoreError(e.to_string())
}

fn pe(e: sqlx::Error) -> PatchError {
    if let sqlx::Error::Database(d) = &e
        && d.is_unique_violation()
    {
        return PatchError::Invalid("that number is already taken".into());
    }
    PatchError::Store(se(e))
}

const CINEMA_ITEM: &str = "SELECT t.id, t.kind, t.title, ''::text AS subtitle, t.year::int4 AS year, t.mount_id, \
     t.poster_key, t.backdrop_key, t.tags, t.genres, t.synopsis, t.attribution, \
     t.external_ids::text AS external_ids, t.overridden, \
     (SELECT count(*) FROM video_episodes e WHERE e.title_id = t.id AND e.shelf_state = 'listed')::bigint AS parts, \
     o.key AS object_key, COALESCE(o.container, '') AS container, COALESCE(o.web_native, FALSE) AS web_native \
     FROM video_titles t LEFT JOIN storage_objects o ON o.id = t.mount_object_id AND o.state = 'indexed' \
     WHERE t.source = 'mount' AND t.shelf_state = 'listed'";

const LIBRARY_ITEM: &str = "SELECT s.id, s.kind, s.title, ''::text AS subtitle, s.year::int4 AS year, s.mount_id, \
     s.cover_key AS poster_key, NULL::text AS backdrop_key, s.tags, s.genres, s.synopsis, s.attribution, \
     s.external_ids::text AS external_ids, s.overridden, \
     (SELECT count(*) FROM reading_chapters c WHERE c.series_id = s.id AND c.shelf_state = 'listed')::bigint AS parts, \
     CASE WHEN (SELECT count(*) FROM reading_chapters c2 WHERE c2.series_id = s.id AND c2.shelf_state = 'listed') = 1 \
          THEN f.key END AS object_key, \
     COALESCE(f.container, '') AS container, COALESCE(f.web_native, FALSE) AS web_native \
     FROM reading_series s LEFT JOIN LATERAL ( \
        SELECT o.key, o.container, o.web_native FROM reading_chapters c \
        JOIN storage_objects o ON o.id = c.mount_object_id \
        WHERE c.series_id = s.id AND c.shelf_state = 'listed' ORDER BY c.number LIMIT 1) f ON TRUE \
     WHERE s.source = 'mount' AND s.shelf_state = 'listed'";

const MUSIC_ITEM: &str = "SELECT m.id, 'track'::text AS kind, m.title, \
     CASE WHEN m.album = '' THEN m.artist WHEN m.artist = '' THEN m.album ELSE m.artist || ' - ' || m.album END AS subtitle, \
     m.year::int4 AS year, m.mount_id, m.cover_key AS poster_key, NULL::text AS backdrop_key, m.tags, m.genres, \
     m.synopsis, m.attribution, m.external_ids::text AS external_ids, m.overridden, 0::bigint AS parts, \
     o.key AS object_key, COALESCE(o.container, '') AS container, COALESCE(o.web_native, FALSE) AS web_native \
     FROM music_tracks m JOIN storage_objects o ON o.id = m.mount_object_id AND o.state = 'indexed' \
     WHERE m.shelf_state = 'listed'";

fn item_from(r: &PgRow, section: Section) -> Result<ShelfItem, StoreError> {
    let year: Option<i32> = r.try_get("year").map_err(se)?;
    Ok(ShelfItem {
        id: r.try_get("id").map_err(se)?,
        section,
        kind: r.try_get("kind").map_err(se)?,
        title: r.try_get("title").map_err(se)?,
        subtitle: r.try_get("subtitle").map_err(se)?,
        year: year.and_then(|y| i16::try_from(y).ok()),
        mount_id: r.try_get("mount_id").map_err(se)?,
        poster_key: r.try_get("poster_key").map_err(se)?,
        backdrop_key: r.try_get("backdrop_key").map_err(se)?,
        tags: r.try_get("tags").map_err(se)?,
        genres: r.try_get("genres").map_err(se)?,
        synopsis: r.try_get("synopsis").map_err(se)?,
        attribution: r.try_get("attribution").map_err(se)?,
        external_ids: r.try_get("external_ids").map_err(se)?,
        overridden: r.try_get("overridden").map_err(se)?,
        parts: r.try_get("parts").map_err(se)?,
        object_key: r.try_get("object_key").map_err(se)?,
        container: r.try_get("container").map_err(se)?,
        web_native: r.try_get("web_native").map_err(se)?,
    })
}

fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn base(section: Section) -> (&'static str, &'static str, &'static str) {
    // (select, alias of the mount column, alias of the kind column, ...) packed as (sql, table alias, title cols)
    match section {
        Section::Cinema => (CINEMA_ITEM, "t", "t.title"),
        Section::Library => (LIBRARY_ITEM, "s", "s.title"),
        Section::Music => (MUSIC_ITEM, "m", "m.title"),
    }
}

fn push_filters(
    qb: &mut QueryBuilder<Postgres>,
    section: Section,
    mounts: &[i64],
    f: &ShelfFilter,
) {
    let (_, a, _) = base(section);
    qb.push(format!(" AND {a}.mount_id = ANY("))
        .push_bind(mounts.to_vec())
        .push(")");
    if let Some(m) = f.mount {
        qb.push(format!(" AND {a}.mount_id = ")).push_bind(m);
    }
    if let Some(k) = &f.kind
        && section != Section::Music
    {
        qb.push(format!(" AND {a}.kind = ")).push_bind(k.clone());
    }
    if let Some(q) = f.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        let pat = format!("%{}%", like_escape(q));
        if section == Section::Music {
            qb.push(" AND (m.title ILIKE ")
                .push_bind(pat.clone())
                .push(" OR m.artist ILIKE ")
                .push_bind(pat.clone())
                .push(" OR m.album ILIKE ")
                .push_bind(pat)
                .push(")");
        } else {
            qb.push(format!(" AND {a}.title ILIKE ")).push_bind(pat);
        }
    }
}

#[async_trait]
impl ShelfStore for PgShelfStore {
    async fn mounts_with(&self, section: Section) -> Result<Vec<i64>, StoreError> {
        let sql = match section {
            Section::Cinema => {
                "SELECT DISTINCT mount_id FROM video_titles WHERE source = 'mount' AND shelf_state = 'listed'"
            }
            Section::Library => {
                "SELECT DISTINCT mount_id FROM reading_series WHERE source = 'mount' AND shelf_state = 'listed'"
            }
            Section::Music => {
                "SELECT DISTINCT mount_id FROM music_tracks WHERE shelf_state = 'listed'"
            }
        };
        sqlx::query_scalar(sql)
            .fetch_all(&self.pool)
            .await
            .map_err(se)
    }

    async fn list(
        &self,
        section: Section,
        mounts: &[i64],
        f: &ShelfFilter,
    ) -> Result<ShelfPage, StoreError> {
        if mounts.is_empty() {
            return Ok(ShelfPage::default());
        }
        let (select, a, title) = base(section);
        let mut qb = QueryBuilder::<Postgres>::new(select);
        push_filters(&mut qb, section, mounts, f);
        qb.push(format!(" ORDER BY lower({title}), {a}.id LIMIT "))
            .push_bind(i64::from(f.limit.clamp(1, 50)))
            .push(" OFFSET ")
            .push_bind(i64::from(f.offset));
        let rows = qb.build().fetch_all(&self.pool).await.map_err(se)?;
        let items = rows
            .iter()
            .map(|r| item_from(r, section))
            .collect::<Result<Vec<_>, _>>()?;
        let table = match section {
            Section::Cinema => "video_titles t",
            Section::Library => "reading_series s",
            Section::Music => "music_tracks m",
        };
        let state = if section == Section::Music {
            "m.shelf_state = 'listed'".to_owned()
        } else {
            format!("{a}.source = 'mount' AND {a}.shelf_state = 'listed'")
        };
        let mut cb =
            QueryBuilder::<Postgres>::new(format!("SELECT count(*) FROM {table} WHERE {state}"));
        push_filters(&mut cb, section, mounts, f);
        let total: i64 = cb
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await
            .map_err(se)?;
        Ok(ShelfPage { items, total })
    }

    async fn get(
        &self,
        section: Section,
        id: i64,
        mounts: &[i64],
    ) -> Result<Option<ShelfDetail>, StoreError> {
        if mounts.is_empty() {
            return Ok(None);
        }
        let (select, a, _) = base(section);
        let mut qb = QueryBuilder::<Postgres>::new(select);
        qb.push(format!(" AND {a}.id = ")).push_bind(id);
        qb.push(format!(" AND {a}.mount_id = ANY("))
            .push_bind(mounts.to_vec())
            .push(")");
        let Some(row) = qb.build().fetch_optional(&self.pool).await.map_err(se)? else {
            return Ok(None);
        };
        let item = item_from(&row, section)?;
        let parts = match section {
            Section::Cinema => {
                let rows = sqlx::query(
                    "SELECT e.id, s.number AS season, e.number::float8 AS number, e.title, o.key, o.container, \
                            o.web_native, o.size_bytes \
                     FROM video_episodes e JOIN video_seasons s ON s.id = e.season_id \
                     JOIN storage_objects o ON o.id = e.mount_object_id AND o.state = 'indexed' \
                     WHERE e.title_id = $1 AND e.shelf_state = 'listed' ORDER BY s.number, e.number, e.id",
                )
                .bind(id)
                .fetch_all(&self.pool)
                .await
                .map_err(se)?;
                rows.iter()
                    .map(|r| part_from(r, true))
                    .collect::<Result<Vec<_>, _>>()?
            }
            Section::Library => {
                let rows = sqlx::query(
                    "SELECT c.id, NULL::int4 AS season, c.number::float8 AS number, COALESCE(c.title, '') AS title, \
                            o.key, o.container, o.web_native, o.size_bytes \
                     FROM reading_chapters c \
                     JOIN storage_objects o ON o.id = c.mount_object_id AND o.state = 'indexed' \
                     WHERE c.series_id = $1 AND c.shelf_state = 'listed' ORDER BY c.number, c.id",
                )
                .bind(id)
                .fetch_all(&self.pool)
                .await
                .map_err(se)?;
                rows.iter()
                    .map(|r| part_from(r, false))
                    .collect::<Result<Vec<_>, _>>()?
            }
            Section::Music => Vec::new(),
        };
        Ok(Some(ShelfDetail { item, parts }))
    }

    async fn mount_of(&self, section: Section, id: i64) -> Result<Option<i64>, StoreError> {
        let sql = match section {
            Section::Cinema => {
                "SELECT mount_id FROM video_titles WHERE id = $1 AND source = 'mount'"
            }
            Section::Library => {
                "SELECT mount_id FROM reading_series WHERE id = $1 AND source = 'mount'"
            }
            Section::Music => "SELECT mount_id FROM music_tracks WHERE id = $1",
        };
        sqlx::query_scalar(sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(se)
    }

    async fn patch(&self, section: Section, id: i64, p: &ShelfPatch) -> Result<(), PatchError> {
        let (table, poster, backdrop) = match section {
            Section::Cinema => ("video_titles", "poster_key", Some("backdrop_key")),
            Section::Library => ("reading_series", "cover_key", None),
            Section::Music => ("music_tracks", "cover_key", None),
        };
        let mut qb =
            QueryBuilder::<Postgres>::new(format!("UPDATE {table} SET updated_at = now()"));
        let mut names: Vec<String> = Vec::new();
        if let Some(t) = &p.title {
            qb.push(", title = ").push_bind(t.clone());
            if section == Section::Cinema {
                qb.push(", sort_title = ").push_bind(sort_title(t));
            }
            names.push("title".into());
        }
        if let Some(y) = p.year {
            qb.push(", year = ").push_bind(y);
            names.push("year".into());
        }
        if let Some(k) = &p.kind
            && section != Section::Music
        {
            qb.push(", kind = ").push_bind(k.clone());
            names.push("kind".into());
        }
        if let Some(t) = &p.tags {
            qb.push(", tags = ").push_bind(t.clone());
            names.push("tags".into());
        }
        if let Some(g) = &p.genres {
            qb.push(", genres = ").push_bind(g.clone());
            names.push("genres".into());
        }
        if let Some(s) = &p.synopsis {
            qb.push(", synopsis = ").push_bind(s.clone());
            names.push("synopsis".into());
        }
        if let Some(k) = &p.poster_key {
            qb.push(format!(", {poster} = ")).push_bind(k.clone());
            names.push("poster".into());
        }
        if let (Some(k), Some(col)) = (&p.backdrop_key, backdrop) {
            qb.push(format!(", {col} = ")).push_bind(k.clone());
            names.push("backdrop".into());
        }
        if section == Section::Music {
            if let Some(a) = &p.artist {
                qb.push(", artist = ").push_bind(a.clone());
                names.push("artist".into());
            }
            if let Some(a) = &p.album {
                qb.push(", album = ").push_bind(a.clone());
                names.push("album".into());
            }
            if let Some(n) = p.track_no {
                qb.push(", track_no = ").push_bind(n);
                names.push("track".into());
            }
        }
        if p.external_ids.is_some() || p.attribution.is_some() {
            if let Some(e) = &p.external_ids {
                qb.push(", external_ids = ")
                    .push_bind(e.clone())
                    .push("::jsonb");
            }
            if let Some(a) = &p.attribution {
                qb.push(", attribution = ").push_bind(a.clone());
            }
            names.push("details".into());
        }
        qb.push(", overridden = ARRAY(SELECT DISTINCT unnest(overridden || ")
            .push_bind(names)
            .push("::text[]))");
        qb.push(" WHERE id = ").push_bind(id);
        if section != Section::Music {
            qb.push(" AND source = 'mount'");
        }
        let r = qb.build().execute(&self.pool).await.map_err(pe)?;
        if r.rows_affected() == 0 {
            return Err(PatchError::NotFound);
        }
        Ok(())
    }

    async fn patch_part(
        &self,
        section: Section,
        id: i64,
        part: i64,
        mount_id: i64,
        p: &PartPatch,
    ) -> Result<(), PatchError> {
        let mut tx = self.pool.begin().await.map_err(pe)?;
        match section {
            Section::Cinema => {
                let row = sqlx::query(
                    "SELECT e.season_id FROM video_episodes e JOIN video_titles t ON t.id = e.title_id \
                     WHERE e.id = $1 AND t.id = $2 AND t.mount_id = $3 AND t.source = 'mount'",
                )
                .bind(part)
                .bind(id)
                .bind(mount_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(pe)?;
                let Some(row) = row else {
                    return Err(PatchError::NotFound);
                };
                let mut season_id: i64 = row
                    .try_get("season_id")
                    .map_err(|e| PatchError::Store(se(e)))?;
                let mut names: Vec<&str> = Vec::new();
                if let Some(s) = p.season {
                    let found: Option<i64> = sqlx::query_scalar(
                        "SELECT id FROM video_seasons WHERE title_id = $1 AND number = $2",
                    )
                    .bind(id)
                    .bind(s)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(pe)?;
                    season_id = match found {
                        Some(x) => x,
                        None => {
                            let new_id = (self.ids)();
                            sqlx::query("INSERT INTO video_seasons (id, title_id, number) VALUES ($1, $2, $3)")
                                .bind(new_id)
                                .bind(id)
                                .bind(s)
                                .execute(&mut *tx)
                                .await
                                .map_err(pe)?;
                            new_id
                        }
                    };
                    names.push("number");
                }
                if p.number.is_some() {
                    names.push("number");
                }
                if p.title.is_some() {
                    names.push("title");
                }
                let number = p.number.map(|n| n.round() as i32);
                sqlx::query(
                    "UPDATE video_episodes SET season_id = $2, number = COALESCE($3, number), \
                       title = COALESCE($4, title), \
                       overridden = ARRAY(SELECT DISTINCT unnest(overridden || $5::text[])), updated_at = now() \
                     WHERE id = $1",
                )
                .bind(part)
                .bind(season_id)
                .bind(number)
                .bind(p.title.clone())
                .bind(names.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
                .execute(&mut *tx)
                .await
                .map_err(pe)?;
            }
            Section::Library => {
                let mut names: Vec<String> = Vec::new();
                if p.number.is_some() {
                    names.push("number".into());
                }
                if p.title.is_some() {
                    names.push("title".into());
                }
                let r = sqlx::query(
                    "UPDATE reading_chapters c SET number = COALESCE($3::float8::numeric, c.number), \
                       title = COALESCE($4, c.title), \
                       overridden = ARRAY(SELECT DISTINCT unnest(c.overridden || $5::text[])), updated_at = now() \
                     FROM reading_series s \
                     WHERE c.id = $1 AND s.id = c.series_id AND s.id = $2 AND s.mount_id = $6 AND s.source = 'mount'",
                )
                .bind(part)
                .bind(id)
                .bind(p.number)
                .bind(p.title.clone())
                .bind(names)
                .bind(mount_id)
                .execute(&mut *tx)
                .await
                .map_err(pe)?;
                if r.rows_affected() == 0 {
                    return Err(PatchError::NotFound);
                }
            }
            Section::Music => return Err(PatchError::NotFound),
        }
        tx.commit().await.map_err(pe)?;
        Ok(())
    }

    async fn object_keys(
        &self,
        section: Section,
        ids: &[i64],
    ) -> Result<Vec<(i64, String)>, StoreError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let sql = match section {
            Section::Cinema => {
                "SELECT t.id, o.key FROM video_titles t JOIN storage_objects o ON o.id = t.mount_object_id \
                 WHERE t.id = ANY($1)"
            }
            Section::Library => {
                "SELECT s.id, o.key FROM reading_series s JOIN reading_chapters c ON c.series_id = s.id \
                 JOIN storage_objects o ON o.id = c.mount_object_id WHERE s.id = ANY($1) AND s.id IN ( \
                    SELECT series_id FROM reading_chapters GROUP BY series_id HAVING count(*) = 1)"
            }
            Section::Music => {
                "SELECT m.id, o.key FROM music_tracks m JOIN storage_objects o ON o.id = m.mount_object_id \
                 WHERE m.id = ANY($1)"
            }
        };
        let rows = sqlx::query(sql)
            .bind(ids)
            .fetch_all(&self.pool)
            .await
            .map_err(se)?;
        rows.iter()
            .map(|r| Ok((r.try_get("id").map_err(se)?, r.try_get("key").map_err(se)?)))
            .collect()
    }
}

fn part_from(r: &PgRow, season: bool) -> Result<ShelfPart, StoreError> {
    Ok(ShelfPart {
        id: r.try_get("id").map_err(se)?,
        season: if season {
            Some(r.try_get("season").map_err(se)?)
        } else {
            None
        },
        number: r.try_get("number").map_err(se)?,
        title: r.try_get("title").map_err(se)?,
        object_key: r.try_get("key").map_err(se)?,
        container: r.try_get("container").map_err(se)?,
        web_native: r.try_get("web_native").map_err(se)?,
        size: r.try_get("size_bytes").map_err(se)?,
    })
}

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
