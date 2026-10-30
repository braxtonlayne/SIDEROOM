//! Postgres side of the music library (MUS-IDX-001, migration 1540): the track listing the library and the
//! player read, and the tag writes the browser's lazy tagger makes.
//!
//! The rows are the ones the indexer builds from file names (`index_repo`); a write here records the fields it
//! sets in `overridden`, so a re-index leaves them alone, and in `tagged` when they came from the file's own tags,
//! so a rescan may overwrite them while a member's hand edit stays.

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, QueryBuilder, Row, postgres::PgRow};
use sr_storage::index::StoreError;
use sr_storage::index::audio::{
    LibraryFilter, MAX_PAGE, MusicStore, TagSource, TagWrite, TagsState, Track, TrackPage,
    field_names,
};
use sr_storage::index::shelf::PatchError;

/// Postgres-backed [`MusicStore`].
#[derive(Clone)]
pub struct PgMusicStore {
    pool: PgPool,
}

impl PgMusicStore {
    /// Wrap a pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn se(e: sqlx::Error) -> StoreError {
    StoreError(e.to_string())
}

fn pe(e: sqlx::Error) -> PatchError {
    PatchError::Store(se(e))
}

const SELECT: &str = "SELECT m.id, m.mount_id, o.key, o.size_bytes AS size, COALESCE(o.container, '') AS container, \
     COALESCE(o.web_native, FALSE) AS web_native, m.title, m.artist, m.album, m.album_artist, m.track_no, \
     m.disc_no, m.year::int4 AS year, m.duration_ms, m.genres, m.cover_key, m.overridden, m.tags_state, \
     (extract(epoch FROM m.tags_read_at) * 1000)::bigint AS tags_read_ms \
     FROM music_tracks m JOIN storage_objects o ON o.id = m.mount_object_id AND o.state = 'indexed' \
     WHERE m.shelf_state = 'listed'";

fn track_from(r: &PgRow) -> Result<Track, StoreError> {
    let year: Option<i32> = r.try_get("year").map_err(se)?;
    Ok(Track {
        id: r.try_get("id").map_err(se)?,
        mount_id: r.try_get("mount_id").map_err(se)?,
        key: r.try_get("key").map_err(se)?,
        size: r.try_get("size").map_err(se)?,
        container: r.try_get("container").map_err(se)?,
        web_native: r.try_get("web_native").map_err(se)?,
        title: r.try_get("title").map_err(se)?,
        artist: r.try_get("artist").map_err(se)?,
        album: r.try_get("album").map_err(se)?,
        album_artist: r.try_get("album_artist").map_err(se)?,
        track_no: r.try_get("track_no").map_err(se)?,
        disc_no: r.try_get("disc_no").map_err(se)?,
        year: year.and_then(|y| i16::try_from(y).ok()),
        duration_ms: r.try_get("duration_ms").map_err(se)?,
        genres: r.try_get("genres").map_err(se)?,
        cover_key: r.try_get("cover_key").map_err(se)?,
        overridden: r.try_get("overridden").map_err(se)?,
        tags_state: r.try_get("tags_state").map_err(se)?,
        tags_read_ms: r.try_get("tags_read_ms").map_err(se)?,
    })
}

fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn push_filters(qb: &mut QueryBuilder<Postgres>, mounts: &[i64], f: &LibraryFilter) {
    qb.push(" AND m.mount_id = ANY(")
        .push_bind(mounts.to_vec())
        .push(")");
    if let Some(m) = f.mount {
        qb.push(" AND m.mount_id = ").push_bind(m);
    }
    if let Some(a) = &f.album {
        qb.push(" AND lower(m.album) = lower(")
            .push_bind(a.clone())
            .push(")");
    }
    if let Some(a) = &f.artist {
        qb.push(" AND (lower(m.artist) = lower(")
            .push_bind(a.clone())
            .push(") OR lower(m.album_artist) = lower(")
            .push_bind(a.clone())
            .push("))");
    }
    if let Some(q) = f.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        let pat = format!("%{}%", like_escape(q));
        qb.push(" AND (m.title ILIKE ")
            .push_bind(pat.clone())
            .push(" OR m.artist ILIKE ")
            .push_bind(pat.clone())
            .push(" OR m.album ILIKE ")
            .push_bind(pat)
            .push(")");
    }
    if f.pending {
        qb.push(" AND m.tags_state IN ('pending', 'failed')");
    }
}

/// `col = <new>` unless the write is the tagger's and the member edited `name` by hand.
fn set(
    qb: &mut QueryBuilder<Postgres>,
    source: TagSource,
    name: &str,
    col: &str,
    bind: impl FnOnce(&mut QueryBuilder<Postgres>),
) {
    qb.push(format!(", {col} = "));
    if source == TagSource::File {
        qb.push(format!(
            "CASE WHEN '{name}' = ANY(overridden) AND NOT '{name}' = ANY(tagged) THEN {col} ELSE "
        ));
        bind(qb);
        qb.push(" END");
    } else {
        bind(qb);
    }
}

#[async_trait]
impl MusicStore for PgMusicStore {
    async fn mounts(&self) -> Result<Vec<i64>, StoreError> {
        sqlx::query_scalar(
            "SELECT DISTINCT mount_id FROM music_tracks WHERE shelf_state = 'listed'",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(se)
    }

    async fn list(&self, mounts: &[i64], f: &LibraryFilter) -> Result<TrackPage, StoreError> {
        if mounts.is_empty() {
            return Ok(TrackPage::default());
        }
        let mut qb = QueryBuilder::<Postgres>::new(SELECT);
        push_filters(&mut qb, mounts, f);
        qb.push(
            " ORDER BY lower(m.album), m.disc_no NULLS LAST, m.track_no NULLS LAST, lower(m.title), m.id LIMIT ",
        )
        .push_bind(i64::from(f.limit.clamp(1, MAX_PAGE)))
        .push(" OFFSET ")
        .push_bind(i64::from(f.offset));
        let rows = qb.build().fetch_all(&self.pool).await.map_err(se)?;
        let items = rows.iter().map(track_from).collect::<Result<Vec<_>, _>>()?;
        let mut cb = QueryBuilder::<Postgres>::new(
            "SELECT count(*) FROM music_tracks m JOIN storage_objects o ON o.id = m.mount_object_id \
             AND o.state = 'indexed' WHERE m.shelf_state = 'listed'",
        );
        push_filters(&mut cb, mounts, f);
        let total: i64 = cb
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await
            .map_err(se)?;
        Ok(TrackPage { items, total })
    }

    async fn get(&self, id: i64, mounts: &[i64]) -> Result<Option<Track>, StoreError> {
        if mounts.is_empty() {
            return Ok(None);
        }
        let mut qb = QueryBuilder::<Postgres>::new(SELECT);
        qb.push(" AND m.id = ").push_bind(id);
        qb.push(" AND m.mount_id = ANY(")
            .push_bind(mounts.to_vec())
            .push(")");
        let row = qb.build().fetch_optional(&self.pool).await.map_err(se)?;
        row.as_ref().map(track_from).transpose()
    }

    async fn mount_of(&self, id: i64) -> Result<Option<i64>, StoreError> {
        sqlx::query_scalar(
            "SELECT mount_id FROM music_tracks WHERE id = $1 AND shelf_state = 'listed'",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(se)
    }

    async fn write(&self, id: i64, w: &TagWrite, source: TagSource) -> Result<(), PatchError> {
        let mut qb = QueryBuilder::<Postgres>::new("UPDATE music_tracks SET updated_at = now()");
        if let Some(v) = &w.title {
            set(&mut qb, source, "title", "title", |q| {
                q.push_bind(v.clone());
            });
        }
        if let Some(v) = &w.artist {
            set(&mut qb, source, "artist", "artist", |q| {
                q.push_bind(v.clone());
            });
        }
        if let Some(v) = &w.album {
            set(&mut qb, source, "album", "album", |q| {
                q.push_bind(v.clone());
            });
        }
        if let Some(v) = &w.album_artist {
            set(&mut qb, source, "album_artist", "album_artist", |q| {
                q.push_bind(v.clone());
            });
        }
        if let Some(v) = w.track_no {
            set(&mut qb, source, "track", "track_no", |q| {
                q.push_bind(v);
            });
        }
        if let Some(v) = w.disc_no {
            set(&mut qb, source, "track", "disc_no", |q| {
                q.push_bind(v);
            });
        }
        if let Some(v) = w.year {
            set(&mut qb, source, "year", "year", |q| {
                q.push_bind(v);
            });
        }
        if let Some(v) = w.duration_ms {
            qb.push(", duration_ms = ").push_bind(v);
        }
        if let Some(v) = &w.genres {
            set(&mut qb, source, "genres", "genres", |q| {
                q.push_bind(v.clone());
            });
        }
        if let Some(c) = &w.cover {
            let key = c.as_ref().map(|(k, _)| k.clone());
            let hash = c.as_ref().map(|(_, h)| h.clone());
            set(&mut qb, source, "poster", "cover_key", |q| {
                q.push_bind(key);
            });
            set(&mut qb, source, "poster", "cover_hash", |q| {
                q.push_bind(hash);
            });
        }
        if let Some(s) = w.state {
            qb.push(", tags_state = ").push_bind(s.as_str());
            if matches!(s, TagsState::Read | TagsState::None) {
                qb.push(", tags_read_at = now()");
            }
        }
        let names: Vec<String> = field_names(w).into_iter().map(str::to_owned).collect();
        // `overridden` gains every name; `tagged` gains the names the file really set (a tagger write that was
        // held back by a hand edit adds nothing) and loses the names a member edited.
        qb.push(", tagged = ");
        match source {
            TagSource::File => {
                qb.push("ARRAY(SELECT DISTINCT unnest(tagged || ARRAY(SELECT n FROM unnest(")
                    .push_bind(names.clone())
                    .push("::text[]) n WHERE NOT (n = ANY(overridden) AND NOT n = ANY(tagged)))))");
            }
            TagSource::Edit => {
                qb.push("ARRAY(SELECT t FROM unnest(tagged) t WHERE NOT t = ANY(")
                    .push_bind(names.clone())
                    .push("::text[]))");
            }
        }
        qb.push(", overridden = ARRAY(SELECT DISTINCT unnest(overridden || ")
            .push_bind(names)
            .push("::text[]))");
        qb.push(" WHERE id = ").push_bind(id);
        let r = qb.build().execute(&self.pool).await.map_err(pe)?;
        if r.rows_affected() == 0 {
            return Err(PatchError::NotFound);
        }
        Ok(())
    }

    async fn rescan(&self, mount_id: i64) -> Result<u64, StoreError> {
        let r = sqlx::query(
            "UPDATE music_tracks SET tags_state = 'pending', updated_at = now() \
             WHERE mount_id = $1 AND shelf_state = 'listed' AND tags_state <> 'pending'",
        )
        .bind(mount_id)
        .execute(&self.pool)
        .await
        .map_err(se)?;
        Ok(r.rows_affected())
    }
}
