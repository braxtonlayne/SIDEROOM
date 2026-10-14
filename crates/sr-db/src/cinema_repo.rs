//! Postgres implementation of [`sr_cinema::CinemaRepository`] (tables in migration 0370).
//! Visibility is part of every read query: a title is public when it is published and not merged
//! away, an episode when it is published as well. Age-restricted titles are visible to everyone
//! (D-184); the service gates opening them on age confirmation.
//! Multi-row changes (merge, publish, marker replacement) run in one transaction.
//! Tests: `tests/cinema_repo.rs` (needs `SR_TEST_DATABASE_URL`).

use std::collections::BTreeMap;

use async_trait::async_trait;
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder, Row, postgres::PgRow};
use sr_cinema::error::{CinemaError, CinemaResult};
use sr_cinema::model::*;
use sr_cinema::repo::{CinemaRepository, EpisodeQuery, NewTitle, Rows};

pub struct PgCinemaRepo {
    pool: PgPool,
}

impl PgCinemaRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn db(e: sqlx::Error) -> CinemaError {
    CinemaError::Storage(e.to_string())
}

fn code(e: &sqlx::Error) -> Option<String> {
    e.as_database_error()
        .and_then(|d| d.code())
        .map(|c| c.into_owned())
}

fn ms(col: &str, name: &str) -> String {
    format!("(extract(epoch FROM {col}) * 1000)::bigint AS {name}")
}

fn dynq(
    sql: String,
) -> sqlx::query::Query<'static, Postgres, <Postgres as sqlx::Database>::Arguments> {
    sqlx::query(sqlx::AssertSqlSafe(sql))
}

fn bad<T>(what: &str, v: &str) -> CinemaResult<T> {
    Err(CinemaError::Storage(format!("unexpected {what} {v:?}")))
}

fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

// ---- rows -> records ------------------------------------------------------------------------

fn title_cols(a: &str) -> String {
    format!(
        "{a}.id, {a}.kind, {a}.title, {a}.original_title, {a}.alt_titles, {a}.sort_title, {a}.year, \
         {a}.runtime_min, {a}.content_rating, {a}.synopsis, {a}.tagline, {a}.genres, {a}.tags, \
         {a}.languages, {a}.subtitle_languages, {a}.cast_list, {a}.directors, {a}.writers, \
         {a}.studios, {a}.crew, {a}.age_restricted, {a}.poster_key, {a}.backdrop_key, {a}.logo_key, \
         {a}.external_ids, {a}.asset_id, {a}.published, {a}.merged_into, {a}.season_count, \
         {a}.episode_count, \
         EXISTS (SELECT 1 FROM video_trailers r WHERE r.title_id = {a}.id AND r.asset_id IS NOT NULL) AS has_trailer, \
         {}, {}, {}, {}",
        ms(&format!("{a}.published_at"), "published_at_ms"),
        ms(&format!("{a}.last_content_at"), "last_content_at_ms"),
        ms(&format!("{a}.created_at"), "created_at_ms"),
        ms(&format!("{a}.updated_at"), "updated_at_ms"),
    )
}

fn title_from(r: &PgRow) -> CinemaResult<TitleRecord> {
    let g = db;
    let kind: String = r.try_get("kind").map_err(g)?;
    let Some(kind) = TitleKind::parse(&kind) else {
        return bad("title kind", &kind);
    };
    let ext: serde_json::Value = r.try_get("external_ids").map_err(g)?;
    let external_ids: BTreeMap<String, String> = ext
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default();
    Ok(TitleRecord {
        id: r.try_get("id").map_err(g)?,
        kind,
        title: r.try_get("title").map_err(g)?,
        original_title: r.try_get("original_title").map_err(g)?,
        alt_titles: r.try_get("alt_titles").map_err(g)?,
        sort_title: r.try_get("sort_title").map_err(g)?,
        year: r.try_get("year").map_err(g)?,
        runtime_min: r.try_get("runtime_min").map_err(g)?,
        content_rating: r.try_get("content_rating").map_err(g)?,
        synopsis: r.try_get("synopsis").map_err(g)?,
        tagline: r.try_get("tagline").map_err(g)?,
        genres: r.try_get("genres").map_err(g)?,
        tags: r.try_get("tags").map_err(g)?,
        languages: r.try_get("languages").map_err(g)?,
        subtitle_languages: r.try_get("subtitle_languages").map_err(g)?,
        cast: r.try_get("cast_list").map_err(g)?,
        directors: r.try_get("directors").map_err(g)?,
        writers: r.try_get("writers").map_err(g)?,
        studios: r.try_get("studios").map_err(g)?,
        crew: r.try_get("crew").map_err(g)?,
        age_restricted: r.try_get("age_restricted").map_err(g)?,
        poster_key: r.try_get("poster_key").map_err(g)?,
        backdrop_key: r.try_get("backdrop_key").map_err(g)?,
        logo_key: r.try_get("logo_key").map_err(g)?,
        external_ids,
        asset_id: r.try_get("asset_id").map_err(g)?,
        published: r.try_get("published").map_err(g)?,
        published_at_ms: r.try_get("published_at_ms").map_err(g)?,
        merged_into: r.try_get("merged_into").map_err(g)?,
        season_count: r.try_get("season_count").map_err(g)?,
        episode_count: r.try_get("episode_count").map_err(g)?,
        last_content_at_ms: r.try_get("last_content_at_ms").map_err(g)?,
        has_trailer: r.try_get("has_trailer").map_err(g)?,
        created_at_ms: r.try_get("created_at_ms").map_err(g)?,
        updated_at_ms: r.try_get("updated_at_ms").map_err(g)?,
        sort_value: None,
    })
}

fn season_from(r: &PgRow) -> CinemaResult<SeasonRecord> {
    let g = db;
    Ok(SeasonRecord {
        id: r.try_get("id").map_err(g)?,
        title_id: r.try_get("title_id").map_err(g)?,
        number: r.try_get("number").map_err(g)?,
        name: r.try_get("name").map_err(g)?,
        synopsis: r.try_get("synopsis").map_err(g)?,
        poster_key: r.try_get("poster_key").map_err(g)?,
        episode_count: r.try_get("episode_count").map_err(g)?,
    })
}

const SEASON_COLS: &str = "s.id, s.title_id, s.number, s.name, s.synopsis, s.poster_key, \
     (SELECT count(*) FROM video_episodes e WHERE e.season_id = s.id AND (e.published OR $PUB))::int AS episode_count";

fn season_sql(public: bool) -> String {
    SEASON_COLS.replace("$PUB", if public { "FALSE" } else { "TRUE" })
}

fn episode_cols() -> String {
    format!(
        "e.id, e.title_id, e.season_id, s.number AS season_number, e.number, e.absolute_number, \
         e.title, e.synopsis, e.air_date::text AS air_date, e.runtime_min, e.still_key, e.asset_id, \
         e.published, {}, t.published AS title_published, (t.merged_into IS NOT NULL) AS title_merged, \
         t.age_restricted AS title_age_restricted",
        ms("e.published_at", "published_at_ms"),
    )
}

const EPISODE_FROM: &str = "FROM video_episodes e JOIN video_seasons s ON s.id = e.season_id \
     JOIN video_titles t ON t.id = e.title_id";

fn episode_from(r: &PgRow) -> CinemaResult<EpisodeRecord> {
    let g = db;
    Ok(EpisodeRecord {
        id: r.try_get("id").map_err(g)?,
        title_id: r.try_get("title_id").map_err(g)?,
        season_id: r.try_get("season_id").map_err(g)?,
        season_number: r.try_get("season_number").map_err(g)?,
        number: r.try_get("number").map_err(g)?,
        absolute_number: r.try_get("absolute_number").map_err(g)?,
        title: r.try_get("title").map_err(g)?,
        synopsis: r.try_get("synopsis").map_err(g)?,
        air_date: r.try_get("air_date").map_err(g)?,
        runtime_min: r.try_get("runtime_min").map_err(g)?,
        still_key: r.try_get("still_key").map_err(g)?,
        asset_id: r.try_get("asset_id").map_err(g)?,
        published: r.try_get("published").map_err(g)?,
        published_at_ms: r.try_get("published_at_ms").map_err(g)?,
        title_published: r.try_get("title_published").map_err(g)?,
        title_merged: r.try_get("title_merged").map_err(g)?,
        title_age_restricted: r.try_get("title_age_restricted").map_err(g)?,
    })
}

fn variant_from(r: &PgRow) -> CinemaResult<VariantRecord> {
    let g = db;
    Ok(VariantRecord {
        id: r.try_get("id").map_err(g)?,
        title_id: r.try_get("title_id").map_err(g)?,
        label: r.try_get("label").map_err(g)?,
        notes: r.try_get("notes").map_err(g)?,
        runtime_min: r.try_get("runtime_min").map_err(g)?,
        asset_id: r.try_get("asset_id").map_err(g)?,
        position: r.try_get("position").map_err(g)?,
    })
}

const VARIANT_COLS: &str = "id, title_id, label, notes, runtime_min, asset_id, position";

fn trailer_from(r: &PgRow) -> CinemaResult<TrailerRecord> {
    let g = db;
    let kind: String = r.try_get("kind").map_err(g)?;
    let Some(kind) = TrailerKind::parse(&kind) else {
        return bad("trailer kind", &kind);
    };
    Ok(TrailerRecord {
        id: r.try_get("id").map_err(g)?,
        title_id: r.try_get("title_id").map_err(g)?,
        label: r.try_get("label").map_err(g)?,
        kind,
        language: r.try_get("language").map_err(g)?,
        asset_id: r.try_get("asset_id").map_err(g)?,
    })
}

const TRAILER_COLS: &str = "id, title_id, label, kind, language, asset_id";

fn marker_from(r: &PgRow) -> CinemaResult<MarkerRecord> {
    let g = db;
    let kind: String = r.try_get("kind").map_err(g)?;
    let Some(kind) = MarkerKind::parse(&kind) else {
        return bad("marker kind", &kind);
    };
    Ok(MarkerRecord {
        id: r.try_get("id").map_err(g)?,
        episode_id: r.try_get("episode_id").map_err(g)?,
        kind,
        start_ms: r.try_get("start_ms").map_err(g)?,
        end_ms: r.try_get("end_ms").map_err(g)?,
        source: r.try_get("source").map_err(g)?,
    })
}

const MARKER_COLS: &str = "id, episode_id, kind, start_ms, end_ms, source";

fn collection_from(r: &PgRow) -> CinemaResult<CollectionRecord> {
    let g = db;
    let display: String = r.try_get("display").map_err(g)?;
    let Some(display) = CollectionDisplay::parse(&display) else {
        return bad("collection display", &display);
    };
    let surfaces: Vec<String> = r.try_get("surfaces").map_err(g)?;
    Ok(CollectionRecord {
        id: r.try_get("id").map_err(g)?,
        slug: r.try_get("slug").map_err(g)?,
        name: r.try_get("name").map_err(g)?,
        eyebrow: r.try_get("eyebrow").map_err(g)?,
        blurb: r.try_get("blurb").map_err(g)?,
        display,
        surfaces: surfaces.iter().filter_map(|s| Surface::parse(s)).collect(),
        position: r.try_get("position").map_err(g)?,
        published: r.try_get("published").map_err(g)?,
        item_count: r.try_get("item_count").map_err(g)?,
    })
}

fn collection_sql(public: bool) -> String {
    format!(
        "SELECT c.id, c.slug, c.name, c.eyebrow, c.blurb, c.display, c.surfaces, c.position, c.published, \
         (SELECT count(*) FROM video_collection_items ci JOIN video_titles t ON t.id = ci.title_id \
           WHERE ci.collection_id = c.id{})::int AS item_count FROM video_collections c",
        if public {
            " AND t.published AND t.merged_into IS NULL"
        } else {
            ""
        }
    )
}

// ---- connection-level helpers -----------------------------------------------------------------

async fn fetch_title(c: &mut PgConnection, id: i64) -> CinemaResult<Option<TitleRecord>> {
    let row = dynq(format!(
        "SELECT {} FROM video_titles t WHERE t.id = $1",
        title_cols("t")
    ))
    .bind(id)
    .fetch_optional(&mut *c)
    .await
    .map_err(db)?;
    row.as_ref().map(title_from).transpose()
}

async fn fetch_episode(c: &mut PgConnection, id: i64) -> CinemaResult<Option<EpisodeRecord>> {
    let row = dynq(format!(
        "SELECT {} {EPISODE_FROM} WHERE e.id = $1",
        episode_cols()
    ))
    .bind(id)
    .fetch_optional(&mut *c)
    .await
    .map_err(db)?;
    row.as_ref().map(episode_from).transpose()
}

async fn fetch_season(c: &mut PgConnection, id: i64) -> CinemaResult<Option<SeasonRecord>> {
    let row = dynq(format!(
        "SELECT {} FROM video_seasons s WHERE s.id = $1",
        season_sql(false)
    ))
    .bind(id)
    .fetch_optional(&mut *c)
    .await
    .map_err(db)?;
    row.as_ref().map(season_from).transpose()
}

async fn fetch_collection(
    c: &mut PgConnection,
    id: i64,
    public: bool,
) -> CinemaResult<Option<CollectionRecord>> {
    let mut sql = collection_sql(public);
    sql.push_str(" WHERE c.id = $1");
    if public {
        sql.push_str(" AND c.published");
    }
    let row = dynq(sql)
        .bind(id)
        .fetch_optional(&mut *c)
        .await
        .map_err(db)?;
    row.as_ref().map(collection_from).transpose()
}

/// Recompute the counters kept on a title row from its published episodes.
async fn refresh_title(c: &mut PgConnection, title: i64) -> CinemaResult<()> {
    sqlx::query(
        "UPDATE video_titles t SET \
           season_count = (SELECT count(DISTINCT e.season_id) FROM video_episodes e \
                           WHERE e.title_id = t.id AND e.published), \
           episode_count = (SELECT count(*) FROM video_episodes e WHERE e.title_id = t.id AND e.published), \
           last_content_at = greatest(t.published_at, \
                           (SELECT max(e.published_at) FROM video_episodes e \
                            WHERE e.title_id = t.id AND e.published)) \
         WHERE t.id = $1",
    )
    .bind(title)
    .execute(&mut *c)
    .await
    .map_err(db)?;
    Ok(())
}

fn ext_json(m: &BTreeMap<String, String>) -> serde_json::Value {
    serde_json::Value::Object(
        m.iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect(),
    )
}

// ---- browse -------------------------------------------------------------------------------------

/// The ordering expression of a browse sort (always NOT NULL), and whether it is text.
fn sort_expr(s: SortKey) -> (&'static str, bool) {
    match s {
        SortKey::Updated => (
            "COALESCE((extract(epoch FROM t.last_content_at) * 1000)::bigint, 0)",
            false,
        ),
        SortKey::Newest => (
            "COALESCE((extract(epoch FROM t.published_at) * 1000)::bigint, 0)",
            false,
        ),
        SortKey::Title => ("t.sort_title", true),
        SortKey::Year => ("COALESCE(t.year, 0)::bigint", false),
        SortKey::Runtime => ("COALESCE(t.runtime_min, 0)::bigint", false),
        SortKey::Curated => ("ci.position::bigint", false),
    }
}

/// ` FROM ... WHERE ...` with every filter of `q`. The keyset cursor only goes in when asked for
/// (the count ignores it).
fn push_from_where(qb: &mut QueryBuilder<Postgres>, q: &BrowseQuery, scope: Scope, cursor: bool) {
    qb.push(" FROM video_titles t");
    if let Some(c) = q.collection {
        qb.push(" JOIN video_collection_items ci ON ci.title_id = t.id AND ci.collection_id = ")
            .push_bind(c);
        qb.push(" JOIN video_collections vc ON vc.id = ci.collection_id");
        if scope == Scope::Public {
            qb.push(" AND vc.published");
        }
    }
    qb.push(" WHERE TRUE");
    if scope == Scope::Public {
        qb.push(" AND t.published AND t.merged_into IS NULL");
    }
    match q.age {
        AgeFilter::Only => {
            qb.push(" AND t.age_restricted");
        }
        AgeFilter::Include => {}
        AgeFilter::Hide => {
            qb.push(" AND NOT t.age_restricted");
        }
    }
    if let Some(k) = q.kind {
        qb.push(" AND t.kind = ").push_bind(k.as_str());
    }
    if !q.genres.is_empty() {
        qb.push(" AND t.genres @> ").push_bind(q.genres.clone());
    }
    if !q.exclude_genres.is_empty() {
        qb.push(" AND NOT (t.genres && ")
            .push_bind(q.exclude_genres.clone())
            .push(")");
    }
    if !q.tags.is_empty() {
        qb.push(" AND t.tags @> ").push_bind(q.tags.clone());
    }
    if let Some(y) = q.year_from {
        qb.push(" AND t.year >= ").push_bind(y);
    }
    if let Some(y) = q.year_to {
        qb.push(" AND t.year <= ").push_bind(y);
    }
    if !q.ratings.is_empty() {
        qb.push(" AND t.content_rating = ANY(")
            .push_bind(q.ratings.clone())
            .push(")");
    }
    if let Some(r) = q.runtime_min {
        qb.push(" AND t.runtime_min >= ").push_bind(r);
    }
    if let Some(r) = q.runtime_max {
        qb.push(" AND t.runtime_min <= ").push_bind(r);
    }
    if let Some(l) = &q.language {
        qb.push(" AND ")
            .push_bind(l.clone())
            .push(" = ANY(t.languages)");
    }
    if let Some(l) = &q.subtitle_language {
        qb.push(" AND ")
            .push_bind(l.clone())
            .push(" = ANY(t.subtitle_languages)");
    }
    if let Some(since) = q.since_ms {
        qb.push(" AND t.last_content_at >= timestamptz 'epoch' + ")
            .push_bind(since)
            .push(" * interval '1 millisecond'");
    }
    if q.episodic_only {
        qb.push(" AND t.episode_count > 0");
    }
    if q.with_backdrop {
        qb.push(" AND t.backdrop_key IS NOT NULL");
    }
    if scope == Scope::Staff {
        if let Some(p) = q.published {
            qb.push(" AND t.published = ").push_bind(p);
        }
        if let Some(t) = &q.title_contains {
            qb.push(" AND t.title ILIKE ")
                .push_bind(format!("%{}%", like_escape(t)));
        }
    }
    if cursor && let Some(after) = &q.after {
        let (expr, _) = sort_expr(q.sort);
        let cmp = if q.is_descending() { "<" } else { ">" };
        qb.push(format!(" AND ({expr}, t.id) {cmp} ("));
        match &after.value {
            SortValue::Int(n) => qb.push_bind(*n),
            SortValue::Text(s) => qb.push_bind(s.clone()),
        };
        qb.push(", ").push_bind(after.id).push(")");
    }
}

#[async_trait]
impl CinemaRepository for PgCinemaRepo {
    async fn browse(&self, q: &BrowseQuery, scope: Scope) -> CinemaResult<Rows<TitleRecord>> {
        let (expr, is_text) = sort_expr(q.sort);
        let mut qb = QueryBuilder::<Postgres>::new(format!(
            "SELECT {}, {expr} AS sort_value",
            title_cols("t")
        ));
        push_from_where(&mut qb, q, scope, true);
        let dir = if q.is_descending() { "DESC" } else { "ASC" };
        qb.push(format!(" ORDER BY {expr} {dir}, t.id {dir}"));
        qb.push(" LIMIT ").push_bind(i64::from(q.limit));
        if q.after.is_none() && q.offset > 0 {
            qb.push(" OFFSET ").push_bind(i64::from(q.offset));
        }
        let rows = qb.build().fetch_all(&self.pool).await.map_err(db)?;
        let mut items = Vec::with_capacity(rows.len());
        for r in &rows {
            let mut rec = title_from(r)?;
            rec.sort_value = Some(if is_text {
                SortValue::Text(r.try_get("sort_value").map_err(db)?)
            } else {
                SortValue::Int(r.try_get("sort_value").map_err(db)?)
            });
            items.push(rec);
        }
        let mut cb = QueryBuilder::<Postgres>::new("SELECT count(*)");
        push_from_where(&mut cb, q, scope, false);
        let total: i64 = cb
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        Ok(Rows { items, total })
    }

    async fn get_title(&self, id: i64, scope: Scope) -> CinemaResult<Option<TitleRecord>> {
        let mut qb = QueryBuilder::<Postgres>::new(format!(
            "SELECT {} FROM video_titles t WHERE t.id = ",
            title_cols("t")
        ));
        qb.push_bind(id);
        if scope == Scope::Public {
            qb.push(" AND t.published AND t.merged_into IS NULL");
        }
        let row = qb.build().fetch_optional(&self.pool).await.map_err(db)?;
        row.as_ref().map(title_from).transpose()
    }

    async fn titles_by_ids(
        &self,
        ids: &[i64],
        scope: Scope,
        include_restricted: bool,
    ) -> CinemaResult<Vec<TitleRecord>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut qb = QueryBuilder::<Postgres>::new(format!(
            "SELECT {} FROM video_titles t WHERE t.id = ANY(",
            title_cols("t")
        ));
        qb.push_bind(ids.to_vec()).push(")");
        if scope == Scope::Public {
            qb.push(" AND t.published AND t.merged_into IS NULL");
        }
        if !include_restricted {
            qb.push(" AND NOT t.age_restricted");
        }
        let rows = qb.build().fetch_all(&self.pool).await.map_err(db)?;
        let mut recs: Vec<TitleRecord> = rows.iter().map(title_from).collect::<Result<_, _>>()?;
        recs.sort_by_key(|r| ids.iter().position(|i| *i == r.id).unwrap_or(usize::MAX));
        Ok(recs)
    }

    async fn facets(
        &self,
        kind: Option<TitleKind>,
        include_restricted: bool,
    ) -> CinemaResult<Facets> {
        let filter = format!(
            "t.published AND t.merged_into IS NULL AND ($1::text IS NULL OR t.kind = $1){}",
            if include_restricted {
                ""
            } else {
                " AND NOT t.age_restricted"
            }
        );
        let kind = kind.map(|k| k.as_str());
        let counts = |sql: String| {
            let kind = kind;
            let pool = self.pool.clone();
            async move {
                let rows = dynq(sql).bind(kind).fetch_all(&pool).await.map_err(db)?;
                rows.iter()
                    .map(|r| {
                        Ok(FacetCount {
                            name: r.try_get("name").map_err(db)?,
                            count: r.try_get("n").map_err(db)?,
                        })
                    })
                    .collect::<CinemaResult<Vec<_>>>()
            }
        };
        let array_facet = |col: &str| {
            format!(
                "SELECT x AS name, count(*) AS n FROM video_titles t, unnest(t.{col}) x \
                 WHERE {filter} GROUP BY x ORDER BY n DESC, x LIMIT 200"
            )
        };
        let genres = counts(array_facet("genres")).await?;
        let languages = counts(array_facet("languages")).await?;
        let subtitle_languages = counts(array_facet("subtitle_languages")).await?;
        let ratings = counts(format!(
            "SELECT t.content_rating AS name, count(*) AS n FROM video_titles t \
             WHERE {filter} AND t.content_rating IS NOT NULL GROUP BY 1 ORDER BY n DESC, 1 LIMIT 50"
        ))
        .await?;
        let agg = dynq(format!(
            "SELECT min(t.year) AS year_min, max(t.year) AS year_max, max(t.runtime_min) AS runtime_max, \
             count(*) AS total FROM video_titles t WHERE {filter}"
        ))
        .bind(kind)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        Ok(Facets {
            genres,
            ratings,
            languages,
            subtitle_languages,
            year_min: agg.try_get("year_min").map_err(db)?,
            year_max: agg.try_get("year_max").map_err(db)?,
            runtime_max: agg.try_get("runtime_max").map_err(db)?,
            total: agg.try_get("total").map_err(db)?,
        })
    }

    async fn list_seasons(&self, title_id: i64, scope: Scope) -> CinemaResult<Vec<SeasonRecord>> {
        let public = scope == Scope::Public;
        let mut sql = format!(
            "SELECT {} FROM video_seasons s WHERE s.title_id = $1",
            season_sql(public)
        );
        if public {
            sql.push_str(
                " AND EXISTS (SELECT 1 FROM video_episodes e WHERE e.season_id = s.id AND e.published)",
            );
        }
        sql.push_str(" ORDER BY s.number");
        let rows = dynq(sql)
            .bind(title_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter().map(season_from).collect()
    }

    async fn get_season(&self, id: i64) -> CinemaResult<Option<SeasonRecord>> {
        let mut c = self.pool.acquire().await.map_err(db)?;
        fetch_season(&mut c, id).await
    }

    async fn list_episodes(
        &self,
        title_id: i64,
        q: &EpisodeQuery,
        scope: Scope,
    ) -> CinemaResult<Vec<EpisodeRecord>> {
        let mut qb = QueryBuilder::<Postgres>::new(format!(
            "SELECT {} {EPISODE_FROM} WHERE e.title_id = ",
            episode_cols()
        ));
        qb.push_bind(title_id);
        if let Some(s) = q.season {
            qb.push(" AND e.season_id = ").push_bind(s);
        }
        if scope == Scope::Public {
            qb.push(" AND e.published");
        }
        if let Some((s, n, id)) = q.after {
            qb.push(" AND (s.number, e.number, e.id) > (")
                .push_bind(s)
                .push(", ")
                .push_bind(n)
                .push(", ")
                .push_bind(id)
                .push(")");
        }
        qb.push(" ORDER BY s.number, e.number, e.id");
        if let Some(l) = q.limit {
            qb.push(" LIMIT ").push_bind(i64::from(l));
        }
        let rows = qb.build().fetch_all(&self.pool).await.map_err(db)?;
        rows.iter().map(episode_from).collect()
    }

    async fn get_episode(&self, id: i64) -> CinemaResult<Option<EpisodeRecord>> {
        let mut c = self.pool.acquire().await.map_err(db)?;
        fetch_episode(&mut c, id).await
    }

    async fn episode_neighbours(&self, id: i64) -> CinemaResult<(Option<i64>, Option<i64>)> {
        let row = sqlx::query(
            "WITH cur AS ( \
               SELECT e.id, e.title_id, s.number AS sn, e.number AS en \
               FROM video_episodes e JOIN video_seasons s ON s.id = e.season_id WHERE e.id = $1) \
             SELECT \
               (SELECT e2.id FROM video_episodes e2 JOIN video_seasons s2 ON s2.id = e2.season_id, cur \
                 WHERE e2.title_id = cur.title_id AND e2.published AND e2.asset_id IS NOT NULL \
                   AND (s2.number, e2.number, e2.id) < (cur.sn, cur.en, cur.id) \
                 ORDER BY s2.number DESC, e2.number DESC, e2.id DESC LIMIT 1) AS prev, \
               (SELECT e2.id FROM video_episodes e2 JOIN video_seasons s2 ON s2.id = e2.season_id, cur \
                 WHERE e2.title_id = cur.title_id AND e2.published AND e2.asset_id IS NOT NULL \
                   AND (s2.number, e2.number, e2.id) > (cur.sn, cur.en, cur.id) \
                 ORDER BY s2.number, e2.number, e2.id LIMIT 1) AS next",
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
        .map_err(db)?;
        Ok((
            row.try_get("prev").map_err(db)?,
            row.try_get("next").map_err(db)?,
        ))
    }

    async fn first_playable_episode(&self, title_id: i64) -> CinemaResult<Option<i64>> {
        sqlx::query_scalar(
            "SELECT e.id FROM video_episodes e JOIN video_seasons s ON s.id = e.season_id \
             WHERE e.title_id = $1 AND e.published AND e.asset_id IS NOT NULL \
             ORDER BY (s.number = 0), s.number, e.number, e.id LIMIT 1",
        )
        .bind(title_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)
    }

    async fn list_variants(&self, title_id: i64) -> CinemaResult<Vec<VariantRecord>> {
        let rows = dynq(format!(
            "SELECT {VARIANT_COLS} FROM video_title_variants WHERE title_id = $1 ORDER BY position, id"
        ))
        .bind(title_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(variant_from).collect()
    }

    async fn get_variant(&self, id: i64) -> CinemaResult<Option<VariantRecord>> {
        let row = dynq(format!(
            "SELECT {VARIANT_COLS} FROM video_title_variants WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(variant_from).transpose()
    }

    async fn list_trailers(&self, title_id: i64) -> CinemaResult<Vec<TrailerRecord>> {
        let rows = dynq(format!(
            "SELECT {TRAILER_COLS} FROM video_trailers WHERE title_id = $1 ORDER BY position, id"
        ))
        .bind(title_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(trailer_from).collect()
    }

    async fn list_markers(
        &self,
        title_id: i64,
        episode: Option<i64>,
    ) -> CinemaResult<Vec<MarkerRecord>> {
        let rows = dynq(format!(
            "SELECT {MARKER_COLS} FROM video_markers WHERE title_id = $1 \
             AND episode_id IS NOT DISTINCT FROM $2 ORDER BY start_ms, id"
        ))
        .bind(title_id)
        .bind(episode)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(marker_from).collect()
    }

    async fn list_collections(
        &self,
        surface: Option<Surface>,
        scope: Scope,
    ) -> CinemaResult<Vec<CollectionRecord>> {
        let public = scope == Scope::Public;
        let mut sql = collection_sql(public);
        sql.push_str(" WHERE TRUE");
        if public {
            sql.push_str(" AND c.published");
        }
        if surface.is_some() {
            sql.push_str(" AND $1 = ANY(c.surfaces)");
        }
        sql.push_str(" ORDER BY c.position, c.id");
        let mut q = dynq(sql);
        if let Some(s) = surface {
            q = q.bind(s.as_str());
        }
        let rows = q.fetch_all(&self.pool).await.map_err(db)?;
        rows.iter().map(collection_from).collect()
    }

    async fn get_collection(
        &self,
        id: i64,
        scope: Scope,
    ) -> CinemaResult<Option<CollectionRecord>> {
        let mut c = self.pool.acquire().await.map_err(db)?;
        fetch_collection(&mut c, id, scope == Scope::Public).await
    }

    async fn index_batch(&self, after: i64, limit: u32) -> CinemaResult<Vec<TitleRecord>> {
        let rows = dynq(format!(
            "SELECT {} FROM video_titles t WHERE t.published AND t.merged_into IS NULL AND t.id > $1 \
             ORDER BY t.id LIMIT $2",
            title_cols("t")
        ))
        .bind(after)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter().map(title_from).collect()
    }

    // ---- title writes --------------------------------------------------

    async fn create_title(&self, t: &NewTitle) -> CinemaResult<TitleRecord> {
        let i = &t.input;
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "INSERT INTO video_titles (id, kind, title, original_title, alt_titles, sort_title, year, \
             runtime_min, content_rating, synopsis, tagline, genres, tags, languages, subtitle_languages, \
             cast_list, directors, writers, studios, crew, age_restricted, poster_key, backdrop_key, \
             logo_key, external_ids, created_by) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26)",
        )
        .bind(t.id)
        .bind(i.kind.as_str())
        .bind(&i.title)
        .bind(&i.original_title)
        .bind(&i.alt_titles)
        .bind(&t.sort_title)
        .bind(i.year)
        .bind(i.runtime_min)
        .bind(&i.content_rating)
        .bind(&i.synopsis)
        .bind(&i.tagline)
        .bind(&i.genres)
        .bind(&i.tags)
        .bind(&i.languages)
        .bind(&i.subtitle_languages)
        .bind(&i.cast)
        .bind(&i.directors)
        .bind(&i.writers)
        .bind(&i.studios)
        .bind(&i.crew)
        .bind(i.age_restricted)
        .bind(&i.poster_key)
        .bind(&i.backdrop_key)
        .bind(&i.logo_key)
        .bind(ext_json(&i.external_ids))
        .bind(t.created_by)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        let rec = fetch_title(&mut tx, t.id)
            .await?
            .ok_or(CinemaError::NotFound("title"))?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn update_title(
        &self,
        id: i64,
        i: &TitleInput,
        sort_title: &str,
    ) -> CinemaResult<Option<TitleRecord>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let res = sqlx::query(
            "UPDATE video_titles SET kind=$2, title=$3, original_title=$4, alt_titles=$5, sort_title=$6, \
             year=$7, runtime_min=$8, content_rating=$9, synopsis=$10, tagline=$11, genres=$12, tags=$13, \
             languages=$14, subtitle_languages=$15, cast_list=$16, directors=$17, writers=$18, studios=$19, \
             crew=$20, age_restricted=$21, poster_key=$22, backdrop_key=$23, logo_key=$24, external_ids=$25, \
             updated_at=now() WHERE id=$1",
        )
        .bind(id)
        .bind(i.kind.as_str())
        .bind(&i.title)
        .bind(&i.original_title)
        .bind(&i.alt_titles)
        .bind(sort_title)
        .bind(i.year)
        .bind(i.runtime_min)
        .bind(&i.content_rating)
        .bind(&i.synopsis)
        .bind(&i.tagline)
        .bind(&i.genres)
        .bind(&i.tags)
        .bind(&i.languages)
        .bind(&i.subtitle_languages)
        .bind(&i.cast)
        .bind(&i.directors)
        .bind(&i.writers)
        .bind(&i.studios)
        .bind(&i.crew)
        .bind(i.age_restricted)
        .bind(&i.poster_key)
        .bind(&i.backdrop_key)
        .bind(&i.logo_key)
        .bind(ext_json(&i.external_ids))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        if res.rows_affected() == 0 {
            return Ok(None);
        }
        let rec = fetch_title(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn set_title_published(
        &self,
        id: i64,
        published: bool,
    ) -> CinemaResult<Option<TitleRecord>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let res = sqlx::query(
            "UPDATE video_titles SET published = $2, \
               published_at = CASE WHEN $2 AND published_at IS NULL THEN now() ELSE published_at END, \
               updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(published)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        if res.rows_affected() == 0 {
            return Ok(None);
        }
        refresh_title(&mut tx, id).await?;
        let rec = fetch_title(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn set_title_asset(
        &self,
        id: i64,
        asset: Option<i64>,
    ) -> CinemaResult<Option<TitleRecord>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let res =
            sqlx::query("UPDATE video_titles SET asset_id = $2, updated_at = now() WHERE id = $1")
                .bind(id)
                .bind(asset)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        if res.rows_affected() == 0 {
            return Ok(None);
        }
        let rec = fetch_title(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn delete_title(&self, id: i64) -> CinemaResult<Option<Vec<i64>>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let eps: Vec<i64> = sqlx::query_scalar("SELECT id FROM video_episodes WHERE title_id = $1")
            .bind(id)
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?;
        // Titles merged into this one keep no dangling pointer: the column is SET NULL on delete.
        let res = sqlx::query("DELETE FROM video_titles WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        if res.rows_affected() == 0 {
            return Ok(None);
        }
        tx.commit().await.map_err(db)?;
        Ok(Some(eps))
    }

    async fn merge_titles(&self, source: i64, target: i64) -> CinemaResult<MergeOutcome> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        // Lock in id order so two concurrent merges cannot deadlock.
        let rows = sqlx::query(
            "SELECT id, merged_into FROM video_titles WHERE id = ANY($1) ORDER BY id FOR UPDATE",
        )
        .bind(vec![source, target])
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        if rows.len() != 2 {
            return Err(CinemaError::NotFound("title"));
        }
        for r in &rows {
            let merged: Option<i64> = r.try_get("merged_into").map_err(db)?;
            if merged.is_some() {
                return Err(CinemaError::Conflict(
                    "a title that was already merged cannot take part in a merge".into(),
                ));
            }
        }
        let mut episode_map: EpisodeMap = Vec::new();

        // Seasons and episodes. The target wins a clash and fills its gaps from the source.
        let seasons =
            sqlx::query("SELECT id, number FROM video_seasons WHERE title_id = $1 ORDER BY number")
                .bind(source)
                .fetch_all(&mut *tx)
                .await
                .map_err(db)?;
        for s in &seasons {
            let sid: i64 = s.try_get("id").map_err(db)?;
            let number: i32 = s.try_get("number").map_err(db)?;
            let tsid: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM video_seasons WHERE title_id = $1 AND number = $2",
            )
            .bind(target)
            .bind(number)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
            let Some(tsid) = tsid else {
                sqlx::query("UPDATE video_seasons SET title_id = $2 WHERE id = $1")
                    .bind(sid)
                    .bind(target)
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?;
                sqlx::query("UPDATE video_episodes SET title_id = $2 WHERE season_id = $1")
                    .bind(sid)
                    .bind(target)
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?;
                continue;
            };
            sqlx::query(
                "UPDATE video_seasons ts SET name = COALESCE(ts.name, s.name), \
                   synopsis = COALESCE(ts.synopsis, s.synopsis), poster_key = COALESCE(ts.poster_key, s.poster_key), \
                   updated_at = now() FROM video_seasons s WHERE ts.id = $1 AND s.id = $2",
            )
            .bind(tsid)
            .bind(sid)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
            let eps = sqlx::query(
                "SELECT id, number FROM video_episodes WHERE season_id = $1 ORDER BY number",
            )
            .bind(sid)
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?;
            for e in &eps {
                let eid: i64 = e.try_get("id").map_err(db)?;
                let n: i32 = e.try_get("number").map_err(db)?;
                let te: Option<i64> = sqlx::query_scalar(
                    "SELECT id FROM video_episodes WHERE season_id = $1 AND number = $2",
                )
                .bind(tsid)
                .bind(n)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
                match te {
                    None => {
                        sqlx::query(
                            "UPDATE video_episodes SET season_id = $2, title_id = $3 WHERE id = $1",
                        )
                        .bind(eid)
                        .bind(tsid)
                        .bind(target)
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                    }
                    Some(teid) => {
                        sqlx::query(
                            "UPDATE video_episodes te SET \
                               published = CASE WHEN te.asset_id IS NULL AND e.asset_id IS NOT NULL THEN e.published ELSE te.published END, \
                               published_at = CASE WHEN te.asset_id IS NULL AND e.asset_id IS NOT NULL THEN e.published_at ELSE te.published_at END, \
                               asset_id = COALESCE(te.asset_id, e.asset_id), \
                               still_key = COALESCE(te.still_key, e.still_key), \
                               title = CASE WHEN te.title = '' THEN e.title ELSE te.title END, \
                               synopsis = CASE WHEN te.synopsis = '' THEN e.synopsis ELSE te.synopsis END, \
                               air_date = COALESCE(te.air_date, e.air_date), \
                               absolute_number = COALESCE(te.absolute_number, e.absolute_number), \
                               runtime_min = COALESCE(te.runtime_min, e.runtime_min), \
                               updated_at = now() \
                             FROM video_episodes e WHERE te.id = $1 AND e.id = $2",
                        )
                        .bind(teid)
                        .bind(eid)
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                        sqlx::query(
                            "UPDATE video_markers m SET episode_id = $2, title_id = $3 \
                             WHERE m.episode_id = $1 AND NOT EXISTS ( \
                               SELECT 1 FROM video_markers x WHERE x.episode_id = $2 AND x.kind = m.kind)",
                        )
                        .bind(eid)
                        .bind(teid)
                        .bind(target)
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                        sqlx::query("DELETE FROM video_episodes WHERE id = $1")
                            .bind(eid)
                            .execute(&mut *tx)
                            .await
                            .map_err(db)?;
                        episode_map.push((eid, teid));
                    }
                }
            }
            sqlx::query("DELETE FROM video_seasons WHERE id = $1")
                .bind(sid)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        // Episode markers follow their episode's title; title markers move unless the target has that kind.
        sqlx::query(
            "UPDATE video_markers m SET title_id = e.title_id FROM video_episodes e \
             WHERE m.episode_id = e.id AND m.title_id <> e.title_id",
        )
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query(
            "UPDATE video_markers m SET title_id = $2 WHERE m.title_id = $1 AND m.episode_id IS NULL \
             AND NOT EXISTS (SELECT 1 FROM video_markers x WHERE x.title_id = $2 AND x.episode_id IS NULL AND x.kind = m.kind)",
        )
        .bind(source)
        .bind(target)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query("DELETE FROM video_markers WHERE title_id = $1")
            .bind(source)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        // Variants (a clashing label gets the id appended) and trailers.
        sqlx::query(
            "UPDATE video_title_variants v SET title_id = $2, \
               label = CASE WHEN EXISTS (SELECT 1 FROM video_title_variants x WHERE x.title_id = $2 AND x.label = v.label) \
                       THEN left(v.label, 100) || ' (' || v.id || ')' ELSE v.label END \
             WHERE v.title_id = $1",
        )
        .bind(source)
        .bind(target)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query("UPDATE video_trailers SET title_id = $2 WHERE title_id = $1")
            .bind(source)
            .bind(target)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        // Collection places.
        sqlx::query(
            "INSERT INTO video_collection_items (collection_id, title_id, position) \
             SELECT collection_id, $2, position FROM video_collection_items WHERE title_id = $1 \
             ON CONFLICT DO NOTHING",
        )
        .bind(source)
        .bind(target)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query("DELETE FROM video_collection_items WHERE title_id = $1")
            .bind(source)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        // Fill the target's gaps and union its lists.
        sqlx::query(
            "UPDATE video_titles t SET \
               original_title = COALESCE(t.original_title, s.original_title), \
               alt_titles = ARRAY(SELECT DISTINCT x FROM unnest(t.alt_titles || s.alt_titles || ARRAY[s.title]) x WHERE x <> t.title), \
               year = COALESCE(t.year, s.year), runtime_min = COALESCE(t.runtime_min, s.runtime_min), \
               content_rating = COALESCE(t.content_rating, s.content_rating), \
               synopsis = CASE WHEN t.synopsis = '' THEN s.synopsis ELSE t.synopsis END, \
               tagline = COALESCE(t.tagline, s.tagline), \
               genres = ARRAY(SELECT DISTINCT x FROM unnest(t.genres || s.genres) x), \
               tags = ARRAY(SELECT DISTINCT x FROM unnest(t.tags || s.tags) x), \
               languages = ARRAY(SELECT DISTINCT x FROM unnest(t.languages || s.languages) x), \
               subtitle_languages = ARRAY(SELECT DISTINCT x FROM unnest(t.subtitle_languages || s.subtitle_languages) x), \
               age_restricted = t.age_restricted OR s.age_restricted, \
               poster_key = COALESCE(t.poster_key, s.poster_key), backdrop_key = COALESCE(t.backdrop_key, s.backdrop_key), \
               logo_key = COALESCE(t.logo_key, s.logo_key), external_ids = s.external_ids || t.external_ids, \
               asset_id = COALESCE(t.asset_id, s.asset_id), updated_at = now() \
             FROM video_titles s WHERE t.id = $2 AND s.id = $1",
        )
        .bind(source)
        .bind(target)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query(
            "UPDATE video_titles SET published = FALSE, merged_into = $2, updated_at = now() WHERE id = $1",
        )
        .bind(source)
        .bind(target)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        refresh_title(&mut tx, source).await?;
        refresh_title(&mut tx, target).await?;
        let rec = fetch_title(&mut tx, target)
            .await?
            .ok_or(CinemaError::NotFound("title"))?;
        tx.commit().await.map_err(db)?;
        Ok(MergeOutcome {
            target: rec,
            episode_map,
        })
    }

    // ---- seasons and episodes --------------------------------------------

    async fn create_season(
        &self,
        id: i64,
        title: i64,
        i: &SeasonInput,
    ) -> CinemaResult<SeasonRecord> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM video_titles WHERE id = $1")
            .bind(title)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        if exists.is_none() {
            return Err(CinemaError::NotFound("title"));
        }
        sqlx::query(
            "INSERT INTO video_seasons (id, title_id, number, name, synopsis, poster_key) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(id)
        .bind(title)
        .bind(i.number)
        .bind(&i.name)
        .bind(&i.synopsis)
        .bind(&i.poster_key)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if code(&e).as_deref() == Some("23505") {
                CinemaError::Conflict(format!("season {} already exists", i.number))
            } else {
                db(e)
            }
        })?;
        let rec = fetch_season(&mut tx, id)
            .await?
            .ok_or(CinemaError::NotFound("season"))?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn update_season(&self, id: i64, i: &SeasonInput) -> CinemaResult<Option<SeasonRecord>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let res = sqlx::query(
            "UPDATE video_seasons SET number = $2, name = $3, synopsis = $4, poster_key = $5, \
             updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(i.number)
        .bind(&i.name)
        .bind(&i.synopsis)
        .bind(&i.poster_key)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if code(&e).as_deref() == Some("23505") {
                CinemaError::Conflict(format!("season {} already exists", i.number))
            } else {
                db(e)
            }
        })?;
        if res.rows_affected() == 0 {
            return Ok(None);
        }
        let rec = fetch_season(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn delete_season(&self, id: i64) -> CinemaResult<Option<(i64, Vec<i64>)>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let title: Option<i64> =
            sqlx::query_scalar("SELECT title_id FROM video_seasons WHERE id = $1 FOR UPDATE")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        let Some(title) = title else { return Ok(None) };
        let eps: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM video_episodes WHERE season_id = $1")
                .bind(id)
                .fetch_all(&mut *tx)
                .await
                .map_err(db)?;
        sqlx::query("DELETE FROM video_seasons WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        refresh_title(&mut tx, title).await?;
        tx.commit().await.map_err(db)?;
        Ok(Some((title, eps)))
    }

    async fn create_episode(
        &self,
        id: i64,
        season: i64,
        i: &EpisodeInput,
    ) -> CinemaResult<EpisodeRecord> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let title: Option<i64> =
            sqlx::query_scalar("SELECT title_id FROM video_seasons WHERE id = $1")
                .bind(season)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        let Some(title) = title else {
            return Err(CinemaError::NotFound("season"));
        };
        sqlx::query(
            "INSERT INTO video_episodes (id, title_id, season_id, number, absolute_number, title, synopsis, \
             air_date, runtime_min, still_key) VALUES ($1, $2, $3, $4, $5, $6, $7, $8::text::date, $9, $10)",
        )
        .bind(id)
        .bind(title)
        .bind(season)
        .bind(i.number)
        .bind(i.absolute_number)
        .bind(&i.title)
        .bind(&i.synopsis)
        .bind(&i.air_date)
        .bind(i.runtime_min)
        .bind(&i.still_key)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if code(&e).as_deref() == Some("23505") {
                CinemaError::Conflict(format!("episode {} already exists in this season", i.number))
            } else {
                db(e)
            }
        })?;
        let rec = fetch_episode(&mut tx, id)
            .await?
            .ok_or(CinemaError::NotFound("episode"))?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn update_episode(
        &self,
        id: i64,
        i: &EpisodeInput,
    ) -> CinemaResult<Option<EpisodeRecord>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let res = sqlx::query(
            "UPDATE video_episodes SET number = $2, absolute_number = $3, title = $4, synopsis = $5, \
             air_date = $6::text::date, runtime_min = $7, still_key = $8, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(i.number)
        .bind(i.absolute_number)
        .bind(&i.title)
        .bind(&i.synopsis)
        .bind(&i.air_date)
        .bind(i.runtime_min)
        .bind(&i.still_key)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if code(&e).as_deref() == Some("23505") {
                CinemaError::Conflict(format!("episode {} already exists in this season", i.number))
            } else {
                db(e)
            }
        })?;
        if res.rows_affected() == 0 {
            return Ok(None);
        }
        let rec = fetch_episode(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn set_episode_published(
        &self,
        id: i64,
        published: bool,
    ) -> CinemaResult<Option<EpisodeRecord>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let title: Option<i64> = sqlx::query_scalar(
            "UPDATE video_episodes SET published = $2, \
               published_at = CASE WHEN $2 AND published_at IS NULL THEN now() ELSE published_at END, \
               updated_at = now() WHERE id = $1 RETURNING title_id",
        )
        .bind(id)
        .bind(published)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let Some(title) = title else { return Ok(None) };
        refresh_title(&mut tx, title).await?;
        let rec = fetch_episode(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn set_episode_asset(
        &self,
        id: i64,
        asset: Option<i64>,
    ) -> CinemaResult<Option<EpisodeRecord>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let res = sqlx::query(
            "UPDATE video_episodes SET asset_id = $2, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(asset)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        if res.rows_affected() == 0 {
            return Ok(None);
        }
        let rec = fetch_episode(&mut tx, id).await?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn delete_episode(&self, id: i64) -> CinemaResult<Option<i64>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let title: Option<i64> =
            sqlx::query_scalar("DELETE FROM video_episodes WHERE id = $1 RETURNING title_id")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        let Some(title) = title else { return Ok(None) };
        refresh_title(&mut tx, title).await?;
        tx.commit().await.map_err(db)?;
        Ok(Some(title))
    }

    // ---- variants, trailers, markers ----------------------------------------

    async fn create_variant(
        &self,
        id: i64,
        title: i64,
        i: &VariantInput,
    ) -> CinemaResult<VariantRecord> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM video_titles WHERE id = $1")
            .bind(title)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        if exists.is_none() {
            return Err(CinemaError::NotFound("title"));
        }
        let row = dynq(format!(
            "INSERT INTO video_title_variants (id, title_id, label, notes, runtime_min, asset_id, position) \
             VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, \
               (SELECT COALESCE(max(position) + 1, 0) FROM video_title_variants WHERE title_id = $2))) \
             RETURNING {VARIANT_COLS}"
        ))
        .bind(id)
        .bind(title)
        .bind(&i.label)
        .bind(&i.notes)
        .bind(i.runtime_min)
        .bind(i.asset_id)
        .bind(i.position)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            if code(&e).as_deref() == Some("23505") {
                CinemaError::Conflict(format!("a variant labelled {:?} already exists", i.label))
            } else {
                db(e)
            }
        })?;
        let rec = variant_from(&row)?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn update_variant(
        &self,
        id: i64,
        i: &VariantInput,
    ) -> CinemaResult<Option<VariantRecord>> {
        let row = dynq(format!(
            "UPDATE video_title_variants SET label = $2, notes = $3, runtime_min = $4, asset_id = $5, \
             position = COALESCE($6, position) WHERE id = $1 RETURNING {VARIANT_COLS}"
        ))
        .bind(id)
        .bind(&i.label)
        .bind(&i.notes)
        .bind(i.runtime_min)
        .bind(i.asset_id)
        .bind(i.position)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            if code(&e).as_deref() == Some("23505") {
                CinemaError::Conflict(format!("a variant labelled {:?} already exists", i.label))
            } else {
                db(e)
            }
        })?;
        row.as_ref().map(variant_from).transpose()
    }

    async fn delete_variant(&self, id: i64) -> CinemaResult<bool> {
        let res = sqlx::query("DELETE FROM video_title_variants WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(res.rows_affected() > 0)
    }

    async fn create_trailer(
        &self,
        id: i64,
        title: i64,
        i: &TrailerInput,
    ) -> CinemaResult<TrailerRecord> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM video_titles WHERE id = $1")
            .bind(title)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        if exists.is_none() {
            return Err(CinemaError::NotFound("title"));
        }
        let row = dynq(format!(
            "INSERT INTO video_trailers (id, title_id, label, kind, language, asset_id, position) \
             VALUES ($1, $2, $3, $4, $5, $6, (SELECT COALESCE(max(position) + 1, 0) FROM video_trailers WHERE title_id = $2)) \
             RETURNING {TRAILER_COLS}"
        ))
        .bind(id)
        .bind(title)
        .bind(&i.label)
        .bind(i.kind.as_str())
        .bind(&i.language)
        .bind(i.asset_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let rec = trailer_from(&row)?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn update_trailer(
        &self,
        id: i64,
        i: &TrailerInput,
    ) -> CinemaResult<Option<TrailerRecord>> {
        let row = dynq(format!(
            "UPDATE video_trailers SET label = $2, kind = $3, language = $4, asset_id = $5 \
             WHERE id = $1 RETURNING {TRAILER_COLS}"
        ))
        .bind(id)
        .bind(&i.label)
        .bind(i.kind.as_str())
        .bind(&i.language)
        .bind(i.asset_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.as_ref().map(trailer_from).transpose()
    }

    async fn delete_trailer(&self, id: i64) -> CinemaResult<bool> {
        let res = sqlx::query("DELETE FROM video_trailers WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(res.rows_affected() > 0)
    }

    async fn replace_markers(
        &self,
        title_id: i64,
        episode: Option<i64>,
        markers: &[MarkerInput],
        ids: &[i64],
        created_by: i64,
    ) -> CinemaResult<Option<Vec<MarkerRecord>>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let exists: Option<i32> = match episode {
            Some(ep) => {
                sqlx::query_scalar("SELECT 1 FROM video_episodes WHERE id = $1 AND title_id = $2")
                    .bind(ep)
                    .bind(title_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(db)?
            }
            None => sqlx::query_scalar("SELECT 1 FROM video_titles WHERE id = $1")
                .bind(title_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?,
        };
        if exists.is_none() {
            return Ok(None);
        }
        sqlx::query(
            "DELETE FROM video_markers WHERE title_id = $1 AND episode_id IS NOT DISTINCT FROM $2",
        )
        .bind(title_id)
        .bind(episode)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        let mut out = Vec::with_capacity(markers.len());
        for (m, id) in markers.iter().zip(ids) {
            let row = dynq(format!(
                "INSERT INTO video_markers (id, title_id, episode_id, kind, start_ms, end_ms, created_by) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {MARKER_COLS}"
            ))
            .bind(*id)
            .bind(title_id)
            .bind(episode)
            .bind(m.kind.as_str())
            .bind(m.start_ms)
            .bind(m.end_ms)
            .bind(created_by)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            out.push(marker_from(&row)?);
        }
        tx.commit().await.map_err(db)?;
        Ok(Some(out))
    }

    // ---- collections ---------------------------------------------------------

    async fn create_collection(
        &self,
        id: i64,
        i: &CollectionInput,
        created_by: i64,
    ) -> CinemaResult<CollectionRecord> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let surfaces: Vec<String> = i.surfaces.iter().map(|s| s.as_str().to_owned()).collect();
        sqlx::query(
            "INSERT INTO video_collections (id, slug, name, eyebrow, blurb, display, surfaces, position, published, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(id)
        .bind(&i.slug)
        .bind(&i.name)
        .bind(&i.eyebrow)
        .bind(&i.blurb)
        .bind(i.display.as_str())
        .bind(&surfaces)
        .bind(i.position)
        .bind(i.published)
        .bind(created_by)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if code(&e).as_deref() == Some("23505") {
                CinemaError::Conflict(format!("the slug {:?} is taken", i.slug))
            } else {
                db(e)
            }
        })?;
        let rec = fetch_collection(&mut tx, id, false)
            .await?
            .ok_or(CinemaError::NotFound("collection"))?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn update_collection(
        &self,
        id: i64,
        i: &CollectionInput,
    ) -> CinemaResult<Option<CollectionRecord>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let surfaces: Vec<String> = i.surfaces.iter().map(|s| s.as_str().to_owned()).collect();
        let res = sqlx::query(
            "UPDATE video_collections SET slug = $2, name = $3, eyebrow = $4, blurb = $5, display = $6, \
             surfaces = $7, position = $8, published = $9, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(&i.slug)
        .bind(&i.name)
        .bind(&i.eyebrow)
        .bind(&i.blurb)
        .bind(i.display.as_str())
        .bind(&surfaces)
        .bind(i.position)
        .bind(i.published)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if code(&e).as_deref() == Some("23505") {
                CinemaError::Conflict(format!("the slug {:?} is taken", i.slug))
            } else {
                db(e)
            }
        })?;
        if res.rows_affected() == 0 {
            return Ok(None);
        }
        let rec = fetch_collection(&mut tx, id, false).await?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }

    async fn delete_collection(&self, id: i64) -> CinemaResult<bool> {
        let res = sqlx::query("DELETE FROM video_collections WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(res.rows_affected() > 0)
    }

    async fn set_collection_items(
        &self,
        id: i64,
        titles: &[i64],
    ) -> CinemaResult<Option<CollectionRecord>> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT id FROM video_collections WHERE id = $1 FOR UPDATE")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        if exists.is_none() {
            return Ok(None);
        }
        let known: i64 = sqlx::query_scalar("SELECT count(*) FROM video_titles WHERE id = ANY($1)")
            .bind(titles.to_vec())
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
        if known != titles.len() as i64 {
            return Err(CinemaError::Invalid(
                "title_ids names an unknown title".into(),
            ));
        }
        sqlx::query("DELETE FROM video_collection_items WHERE collection_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query(
            "INSERT INTO video_collection_items (collection_id, title_id, position) \
             SELECT $1, u.t, (u.o - 1)::int FROM unnest($2::bigint[]) WITH ORDINALITY AS u(t, o)",
        )
        .bind(id)
        .bind(titles.to_vec())
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query("UPDATE video_collections SET updated_at = now() WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let rec = fetch_collection(&mut tx, id, false).await?;
        tx.commit().await.map_err(db)?;
        Ok(rec)
    }
}
