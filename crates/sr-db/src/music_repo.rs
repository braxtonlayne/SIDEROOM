//! Postgres side of the member's music library (MUS-LIB-001, migration 1550): playlists, favorites and the
//! recently played list. The store knows rows and caps; who may see or edit what is decided by the routes
//! (`sr-api` `routes::music`) with the storage scopes, never here.
//!
//! Tracks come from `music_tracks` (migration 1450, built by the index). Only `listed` tracks are returned.

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use sr_storage::music_lib::{
    ItemRow, LibError, MusicLibStore, PlaylistOwner, PlaylistRow, RECENT_KEEP, StampedTrack,
    TrackRow,
};

fn se(e: sqlx::Error) -> LibError {
    LibError::Store(e.to_string())
}

/// Postgres-backed [`MusicLibStore`].
#[derive(Clone)]
pub struct PgMusicLib {
    pool: PgPool,
}

impl PgMusicLib {
    /// Wrap a pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const TRACK_COLS: &str = "t.id AS t_id, t.mount_id AS t_mount_id, t.title AS t_title, t.artist AS t_artist, \
     t.album AS t_album, t.album_artist AS t_album_artist, t.track_no AS t_track_no, t.disc_no AS t_disc_no, \
     t.year AS t_year, t.duration_ms AS t_duration_ms, t.genres AS t_genres, t.tags AS t_tags, \
     t.cover_key AS t_cover_key";

const PLAYLIST_SELECT: &str = "SELECT p.id, p.owner_user_id, p.owner_guild_id, p.name, p.description, \
     p.created_by, (SELECT count(*) FROM music_playlist_items i WHERE i.playlist_id = p.id)::bigint AS item_count, \
     (extract(epoch FROM p.created_at) * 1000)::bigint AS created_ms, \
     (extract(epoch FROM p.updated_at) * 1000)::bigint AS updated_ms \
     FROM music_playlists p";

fn track_from(r: &PgRow) -> Result<TrackRow, LibError> {
    Ok(TrackRow {
        id: r.try_get("t_id").map_err(se)?,
        mount_id: r.try_get("t_mount_id").map_err(se)?,
        title: r.try_get("t_title").map_err(se)?,
        artist: r.try_get("t_artist").map_err(se)?,
        album: r.try_get("t_album").map_err(se)?,
        album_artist: r.try_get("t_album_artist").map_err(se)?,
        track_no: r.try_get("t_track_no").map_err(se)?,
        disc_no: r.try_get("t_disc_no").map_err(se)?,
        year: r.try_get("t_year").map_err(se)?,
        duration_ms: r.try_get("t_duration_ms").map_err(se)?,
        genres: r.try_get("t_genres").map_err(se)?,
        tags: r.try_get("t_tags").map_err(se)?,
        cover_key: r.try_get("t_cover_key").map_err(se)?,
    })
}

fn playlist_from(r: &PgRow) -> Result<PlaylistRow, LibError> {
    let user: Option<i64> = r.try_get("owner_user_id").map_err(se)?;
    let guild: Option<i64> = r.try_get("owner_guild_id").map_err(se)?;
    let owner = match (user, guild) {
        (Some(u), None) => PlaylistOwner::User(u),
        (None, Some(g)) => PlaylistOwner::Guild(g),
        _ => return Err(LibError::Store("playlist without one owner".into())),
    };
    Ok(PlaylistRow {
        id: r.try_get("id").map_err(se)?,
        owner,
        name: r.try_get("name").map_err(se)?,
        description: r.try_get("description").map_err(se)?,
        created_by: r.try_get("created_by").map_err(se)?,
        item_count: r.try_get("item_count").map_err(se)?,
        created_ms: r.try_get("created_ms").map_err(se)?,
        updated_ms: r.try_get("updated_ms").map_err(se)?,
    })
}

fn owner_cols(o: PlaylistOwner) -> (Option<i64>, Option<i64>) {
    match o {
        PlaylistOwner::User(u) => (Some(u), None),
        PlaylistOwner::Guild(g) => (None, Some(g)),
    }
}

/// A 32-bit key for `pg_advisory_xact_lock(class, key)` from an id.
fn lock_key(id: i64) -> i32 {
    i32::try_from(id.rem_euclid(i64::from(i32::MAX))).unwrap_or(0)
}

async fn playlist_in(
    tx: &mut Transaction<'_, Postgres>,
    id: i64,
) -> Result<Option<PlaylistRow>, LibError> {
    let sql = format!("{PLAYLIST_SELECT} WHERE p.id = $1");
    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(se)?;
    row.as_ref().map(playlist_from).transpose()
}

/// Rewrite the positions of a playlist to the dense order of `ids`.
async fn renumber(
    tx: &mut Transaction<'_, Postgres>,
    playlist_id: i64,
    ids: &[i64],
) -> Result<(), LibError> {
    let positions: Vec<i32> = (0..i32::try_from(ids.len()).unwrap_or(i32::MAX)).collect();
    sqlx::query(
        "UPDATE music_playlist_items i SET position = v.pos \
         FROM unnest($2::bigint[], $3::int[]) AS v(id, pos) \
         WHERE i.playlist_id = $1 AND i.id = v.id",
    )
    .bind(playlist_id)
    .bind(ids)
    .bind(&positions)
    .execute(&mut **tx)
    .await
    .map_err(se)?;
    Ok(())
}

async fn item_ids(
    tx: &mut Transaction<'_, Postgres>,
    playlist_id: i64,
) -> Result<Vec<i64>, LibError> {
    let rows = sqlx::query(
        "SELECT id FROM music_playlist_items WHERE playlist_id = $1 ORDER BY position, id",
    )
    .bind(playlist_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(se)?;
    rows.iter()
        .map(|r| r.try_get::<i64, _>("id").map_err(se))
        .collect()
}

/// Lock the playlist row (serialises writers of one playlist) or say it is gone.
async fn lock_playlist(tx: &mut Transaction<'_, Postgres>, id: i64) -> Result<(), LibError> {
    let found = sqlx::query("SELECT id FROM music_playlists WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(se)?;
    found.map(|_| ()).ok_or(LibError::NotFound)
}

async fn touch(tx: &mut Transaction<'_, Postgres>, id: i64) -> Result<(), LibError> {
    sqlx::query("UPDATE music_playlists SET updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(se)?;
    Ok(())
}

#[async_trait]
impl MusicLibStore for PgMusicLib {
    async fn create_playlist(
        &self,
        id: i64,
        owner: PlaylistOwner,
        name: &str,
        description: &str,
        created_by: i64,
        cap: i64,
    ) -> Result<PlaylistRow, LibError> {
        let (user, guild) = owner_cols(owner);
        let mut tx = self.pool.begin().await.map_err(se)?;
        // Serialise creators of one owner so the cap holds under concurrency.
        sqlx::query("SELECT pg_advisory_xact_lock(1550, $1)")
            .bind(lock_key(user.or(guild).unwrap_or(0)))
            .execute(&mut *tx)
            .await
            .map_err(se)?;
        let held: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM music_playlists \
             WHERE owner_user_id IS NOT DISTINCT FROM $1 AND owner_guild_id IS NOT DISTINCT FROM $2",
        )
        .bind(user)
        .bind(guild)
        .fetch_one(&mut *tx)
        .await
        .map_err(se)?;
        if held >= cap {
            return Err(LibError::Cap);
        }
        sqlx::query(
            "INSERT INTO music_playlists (id, owner_user_id, owner_guild_id, name, description, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(id)
        .bind(user)
        .bind(guild)
        .bind(name)
        .bind(description)
        .bind(created_by)
        .execute(&mut *tx)
        .await
        .map_err(se)?;
        let row = playlist_in(&mut tx, id).await?.ok_or(LibError::NotFound)?;
        tx.commit().await.map_err(se)?;
        Ok(row)
    }

    async fn playlist(&self, id: i64) -> Result<Option<PlaylistRow>, LibError> {
        let sql = format!("{PLAYLIST_SELECT} WHERE p.id = $1");
        let row = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(se)?;
        row.as_ref().map(playlist_from).transpose()
    }

    async fn playlists_of(&self, owner: PlaylistOwner) -> Result<Vec<PlaylistRow>, LibError> {
        let (user, guild) = owner_cols(owner);
        let sql = format!(
            "{PLAYLIST_SELECT} WHERE p.owner_user_id IS NOT DISTINCT FROM $1 \
             AND p.owner_guild_id IS NOT DISTINCT FROM $2 ORDER BY p.id DESC LIMIT 500"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(user)
            .bind(guild)
            .fetch_all(&self.pool)
            .await
            .map_err(se)?;
        rows.iter().map(playlist_from).collect()
    }

    async fn update_playlist(
        &self,
        id: i64,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<PlaylistRow, LibError> {
        let mut tx = self.pool.begin().await.map_err(se)?;
        let n = sqlx::query(
            "UPDATE music_playlists SET name = COALESCE($2, name), description = COALESCE($3, description), \
             updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .execute(&mut *tx)
        .await
        .map_err(se)?
        .rows_affected();
        if n == 0 {
            return Err(LibError::NotFound);
        }
        let row = playlist_in(&mut tx, id).await?.ok_or(LibError::NotFound)?;
        tx.commit().await.map_err(se)?;
        Ok(row)
    }

    async fn delete_playlist(&self, id: i64) -> Result<(), LibError> {
        let n = sqlx::query("DELETE FROM music_playlists WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(se)?
            .rows_affected();
        if n == 0 {
            return Err(LibError::NotFound);
        }
        Ok(())
    }

    async fn items(&self, playlist_id: i64) -> Result<Vec<ItemRow>, LibError> {
        let sql = format!(
            "SELECT i.id AS item_id, i.position, {TRACK_COLS} FROM music_playlist_items i \
             JOIN music_tracks t ON t.id = i.track_id AND t.shelf_state = 'listed' \
             WHERE i.playlist_id = $1 ORDER BY i.position, i.id"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(playlist_id)
            .fetch_all(&self.pool)
            .await
            .map_err(se)?;
        rows.iter()
            .map(|r| {
                Ok(ItemRow {
                    item_id: r.try_get("item_id").map_err(se)?,
                    position: r.try_get("position").map_err(se)?,
                    track: track_from(r)?,
                })
            })
            .collect()
    }

    async fn add_items(
        &self,
        playlist_id: i64,
        items: &[(i64, i64)],
        added_by: i64,
        cap: i64,
    ) -> Result<i64, LibError> {
        let mut tx = self.pool.begin().await.map_err(se)?;
        lock_playlist(&mut tx, playlist_id).await?;
        let mut distinct: Vec<i64> = items.iter().map(|(_, t)| *t).collect();
        distinct.sort_unstable();
        distinct.dedup();
        let known: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM music_tracks WHERE id = ANY($1) AND shelf_state = 'listed'",
        )
        .bind(&distinct)
        .fetch_one(&mut *tx)
        .await
        .map_err(se)?;
        if known != i64::try_from(distinct.len()).unwrap_or(i64::MAX) {
            return Err(LibError::NotFound);
        }
        let held: i64 =
            sqlx::query_scalar("SELECT count(*) FROM music_playlist_items WHERE playlist_id = $1")
                .bind(playlist_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(se)?;
        let added = i64::try_from(items.len()).unwrap_or(i64::MAX);
        if held.saturating_add(added) > cap {
            return Err(LibError::Cap);
        }
        for (n, (item_id, track_id)) in items.iter().enumerate() {
            let position = held.saturating_add(i64::try_from(n).unwrap_or(0));
            sqlx::query(
                "INSERT INTO music_playlist_items (id, playlist_id, track_id, position, added_by) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(item_id)
            .bind(playlist_id)
            .bind(track_id)
            .bind(i32::try_from(position).unwrap_or(i32::MAX))
            .bind(added_by)
            .execute(&mut *tx)
            .await
            .map_err(se)?;
        }
        touch(&mut tx, playlist_id).await?;
        tx.commit().await.map_err(se)?;
        Ok(held + added)
    }

    async fn remove_item(&self, playlist_id: i64, item_id: i64) -> Result<(), LibError> {
        let mut tx = self.pool.begin().await.map_err(se)?;
        lock_playlist(&mut tx, playlist_id).await?;
        let n = sqlx::query("DELETE FROM music_playlist_items WHERE playlist_id = $1 AND id = $2")
            .bind(playlist_id)
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .map_err(se)?
            .rows_affected();
        if n == 0 {
            return Err(LibError::NotFound);
        }
        let ids = item_ids(&mut tx, playlist_id).await?;
        renumber(&mut tx, playlist_id, &ids).await?;
        touch(&mut tx, playlist_id).await?;
        tx.commit().await.map_err(se)
    }

    async fn move_item(
        &self,
        playlist_id: i64,
        item_id: i64,
        position: i32,
    ) -> Result<(), LibError> {
        let mut tx = self.pool.begin().await.map_err(se)?;
        lock_playlist(&mut tx, playlist_id).await?;
        let mut ids = item_ids(&mut tx, playlist_id).await?;
        let from = ids
            .iter()
            .position(|i| *i == item_id)
            .ok_or(LibError::NotFound)?;
        ids.remove(from);
        let to = usize::try_from(position.max(0)).unwrap_or(0).min(ids.len());
        ids.insert(to, item_id);
        renumber(&mut tx, playlist_id, &ids).await?;
        touch(&mut tx, playlist_id).await?;
        tx.commit().await.map_err(se)
    }

    async fn tracks(&self, ids: &[i64]) -> Result<Vec<TrackRow>, LibError> {
        let sql = format!(
            "SELECT {TRACK_COLS} FROM music_tracks t WHERE t.id = ANY($1) AND t.shelf_state = 'listed'"
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(ids)
            .fetch_all(&self.pool)
            .await
            .map_err(se)?;
        rows.iter().map(track_from).collect()
    }

    async fn favorites(&self, user: i64) -> Result<Vec<StampedTrack>, LibError> {
        let sql = format!(
            "SELECT (extract(epoch FROM f.created_at) * 1000)::bigint AS at_ms, {TRACK_COLS} \
             FROM music_favorites f JOIN music_tracks t ON t.id = f.track_id AND t.shelf_state = 'listed' \
             WHERE f.user_id = $1 ORDER BY f.created_at DESC, t.id LIMIT 5000"
        );
        stamped(&self.pool, sql, user, None).await
    }

    async fn add_favorite(&self, user: i64, track: i64, cap: i64) -> Result<(), LibError> {
        let mut tx = self.pool.begin().await.map_err(se)?;
        sqlx::query("SELECT pg_advisory_xact_lock(1551, $1)")
            .bind(lock_key(user))
            .execute(&mut *tx)
            .await
            .map_err(se)?;
        let listed: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM music_tracks WHERE id = $1 AND shelf_state = 'listed'",
        )
        .bind(track)
        .fetch_one(&mut *tx)
        .await
        .map_err(se)?;
        if listed == 0 {
            return Err(LibError::NotFound);
        }
        let held: i64 =
            sqlx::query_scalar("SELECT count(*) FROM music_favorites WHERE user_id = $1")
                .bind(user)
                .fetch_one(&mut *tx)
                .await
                .map_err(se)?;
        let have: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM music_favorites WHERE user_id = $1 AND track_id = $2",
        )
        .bind(user)
        .bind(track)
        .fetch_one(&mut *tx)
        .await
        .map_err(se)?;
        if have == 0 && held >= cap {
            return Err(LibError::Cap);
        }
        sqlx::query(
            "INSERT INTO music_favorites (user_id, track_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(user)
        .bind(track)
        .execute(&mut *tx)
        .await
        .map_err(se)?;
        tx.commit().await.map_err(se)
    }

    async fn remove_favorite(&self, user: i64, track: i64) -> Result<(), LibError> {
        sqlx::query("DELETE FROM music_favorites WHERE user_id = $1 AND track_id = $2")
            .bind(user)
            .bind(track)
            .execute(&self.pool)
            .await
            .map_err(se)?;
        Ok(())
    }

    async fn recent(&self, user: i64, limit: i64) -> Result<Vec<StampedTrack>, LibError> {
        let sql = format!(
            "SELECT (extract(epoch FROM r.played_at) * 1000)::bigint AS at_ms, {TRACK_COLS} \
             FROM music_recent r JOIN music_tracks t ON t.id = r.track_id AND t.shelf_state = 'listed' \
             WHERE r.user_id = $1 ORDER BY r.played_at DESC, t.id LIMIT $2"
        );
        stamped(&self.pool, sql, user, Some(limit.clamp(1, RECENT_KEEP))).await
    }

    async fn record_recent(&self, user: i64, track: i64) -> Result<(), LibError> {
        let mut tx = self.pool.begin().await.map_err(se)?;
        let n = sqlx::query(
            "INSERT INTO music_recent (user_id, track_id) \
             SELECT $1, id FROM music_tracks WHERE id = $2 AND shelf_state = 'listed' \
             ON CONFLICT (user_id, track_id) DO UPDATE SET played_at = now()",
        )
        .bind(user)
        .bind(track)
        .execute(&mut *tx)
        .await
        .map_err(se)?
        .rows_affected();
        if n == 0 {
            return Err(LibError::NotFound);
        }
        sqlx::query(
            "DELETE FROM music_recent WHERE user_id = $1 AND track_id NOT IN \
             (SELECT track_id FROM music_recent WHERE user_id = $1 ORDER BY played_at DESC, track_id LIMIT $2)",
        )
        .bind(user)
        .bind(RECENT_KEEP)
        .execute(&mut *tx)
        .await
        .map_err(se)?;
        tx.commit().await.map_err(se)
    }

    async fn clear_recent(&self, user: i64) -> Result<(), LibError> {
        sqlx::query("DELETE FROM music_recent WHERE user_id = $1")
            .bind(user)
            .execute(&self.pool)
            .await
            .map_err(se)?;
        Ok(())
    }
}

async fn stamped(
    pool: &PgPool,
    sql: String,
    user: i64,
    limit: Option<i64>,
) -> Result<Vec<StampedTrack>, LibError> {
    let mut q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(user);
    if let Some(l) = limit {
        q = q.bind(l);
    }
    let rows = q.fetch_all(pool).await.map_err(se)?;
    rows.iter()
        .map(|r| {
            Ok(StampedTrack {
                at_ms: r.try_get("at_ms").map_err(se)?,
                track: track_from(r)?,
            })
        })
        .collect()
}
