//! Postgres side of the voice jukebox (MUS-JB-002, migration 1570). A unit of work locks the channel's
//! `jukebox_sessions` row (`FOR UPDATE`), loads the whole channel as a [`Doc`] and writes back only what
//! changed, so a heartbeat is one row and a track change is a handful.
//!
//! The store keeps rows only. Who may read a queued track's mount is decided per listener at play time by
//! the storage scopes in the routes; nothing here signs or reads a file.

use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Row, Transaction};
use sr_chat::jukebox::model::{Doc, Item, ItemState, JukeboxError, Mode, Session};
use sr_chat::jukebox::store::{JukeboxStore, JukeboxTxn};
use std::collections::{HashMap, HashSet};

fn se(e: sqlx::Error) -> JukeboxError {
    JukeboxError::Store(e.to_string())
}

/// Postgres-backed [`JukeboxStore`].
#[derive(Clone)]
pub struct PgJukebox {
    pool: PgPool,
}

impl PgJukebox {
    /// Wrap a pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const SESSION_SELECT: &str = "SELECT guild_id, mode, dj_user_id, current_item_id, \
     round(extract(epoch FROM started_at) * 1000)::bigint AS started_ms, position_ms, paused, version \
     FROM jukebox_sessions WHERE channel_id = $1";

const ITEM_SELECT: &str = "SELECT id, seq, track_id, mount_id, key, title, artist, duration_ms, added_by, state, \
     round(extract(epoch FROM added_at) * 1000)::bigint AS added_ms \
     FROM jukebox_queue WHERE channel_id = $1 ORDER BY seq, id";

async fn read_doc(
    conn: &mut sqlx::PgConnection,
    channel: i64,
) -> Result<Option<Doc>, JukeboxError> {
    let Some(row) = sqlx::query(SESSION_SELECT)
        .bind(channel)
        .fetch_optional(&mut *conn)
        .await
        .map_err(se)?
    else {
        return Ok(None);
    };
    let mode: String = row.try_get("mode").map_err(se)?;
    let position: i32 = row.try_get("position_ms").map_err(se)?;
    let session = Session {
        channel_id: channel,
        guild_id: row.try_get("guild_id").map_err(se)?,
        mode: Mode::parse(&mode).unwrap_or(Mode::Synced),
        dj: row.try_get("dj_user_id").map_err(se)?,
        current: row.try_get("current_item_id").map_err(se)?,
        started_ms: row.try_get("started_ms").map_err(se)?,
        position_ms: i64::from(position),
        paused: row.try_get("paused").map_err(se)?,
        version: row.try_get("version").map_err(se)?,
    };
    let mut items = Vec::new();
    for r in sqlx::query(ITEM_SELECT)
        .bind(channel)
        .fetch_all(&mut *conn)
        .await
        .map_err(se)?
    {
        let state: String = r.try_get("state").map_err(se)?;
        let dur: Option<i32> = r.try_get("duration_ms").map_err(se)?;
        items.push(Item {
            id: r.try_get("id").map_err(se)?,
            seq: r.try_get("seq").map_err(se)?,
            track_id: r.try_get("track_id").map_err(se)?,
            mount_id: r.try_get("mount_id").map_err(se)?,
            key: r.try_get("key").map_err(se)?,
            title: r.try_get("title").map_err(se)?,
            artist: r.try_get("artist").map_err(se)?,
            duration_ms: dur.map(i64::from),
            added_by: r.try_get("added_by").map_err(se)?,
            state: ItemState::parse(&state).unwrap_or(ItemState::Played),
            added_ms: r.try_get("added_ms").map_err(se)?,
        });
    }
    let votes = sqlx::query(
        "SELECT v.item_id, v.user_id FROM jukebox_votes v \
         JOIN jukebox_queue q ON q.id = v.item_id WHERE q.channel_id = $1 ORDER BY v.item_id, v.user_id",
    )
    .bind(channel)
    .fetch_all(&mut *conn)
    .await
    .map_err(se)?
    .iter()
    .map(|r| Ok((r.try_get("item_id").map_err(se)?, r.try_get("user_id").map_err(se)?)))
    .collect::<Result<Vec<(i64, i64)>, JukeboxError>>()?;
    let listeners = sqlx::query(
        "SELECT user_id, round(extract(epoch FROM seen_at) * 1000)::bigint AS seen_ms \
         FROM jukebox_listeners WHERE channel_id = $1 ORDER BY user_id",
    )
    .bind(channel)
    .fetch_all(&mut *conn)
    .await
    .map_err(se)?
    .iter()
    .map(|r| {
        Ok((
            r.try_get("user_id").map_err(se)?,
            r.try_get("seen_ms").map_err(se)?,
        ))
    })
    .collect::<Result<Vec<(i64, i64)>, JukeboxError>>()?;
    Ok(Some(Doc {
        session,
        items,
        votes,
        listeners,
    }))
}

struct PgTxn {
    tx: Transaction<'static, Postgres>,
    channel: i64,
    base: Option<Doc>,
}

#[async_trait]
impl JukeboxTxn for PgTxn {
    async fn load(&mut self) -> Result<Doc, JukeboxError> {
        let doc = read_doc(&mut self.tx, self.channel)
            .await?
            .ok_or_else(|| JukeboxError::Store("session row vanished".into()))?;
        self.base = Some(doc.clone());
        Ok(doc)
    }

    async fn save(&mut self, doc: &Doc) -> Result<(), JukeboxError> {
        let base = match self.base.take() {
            Some(b) => b,
            None => read_doc(&mut self.tx, self.channel)
                .await?
                .ok_or_else(|| JukeboxError::Store("session row vanished".into()))?,
        };
        let s = &doc.session;
        sqlx::query(
            "UPDATE jukebox_sessions SET mode = $2, dj_user_id = $3, current_item_id = $4, \
             started_at = CASE WHEN $5::bigint IS NULL THEN NULL ELSE to_timestamp($5::bigint / 1000.0) END, \
             position_ms = $6, paused = $7, version = $8, updated_at = now() WHERE channel_id = $1",
        )
        .bind(self.channel)
        .bind(s.mode.as_str())
        .bind(s.dj)
        .bind(s.current)
        .bind(s.started_ms)
        .bind(i32::try_from(s.position_ms).unwrap_or(i32::MAX))
        .bind(s.paused)
        .bind(s.version)
        .execute(&mut *self.tx)
        .await
        .map_err(se)?;

        // Items: drop what is gone, upsert what is new or changed.
        let have: HashMap<i64, &Item> = base.items.iter().map(|i| (i.id, i)).collect();
        let now_ids: HashSet<i64> = doc.items.iter().map(|i| i.id).collect();
        let gone: Vec<i64> = base
            .items
            .iter()
            .map(|i| i.id)
            .filter(|id| !now_ids.contains(id))
            .collect();
        if !gone.is_empty() {
            sqlx::query("DELETE FROM jukebox_queue WHERE channel_id = $1 AND id = ANY($2)")
                .bind(self.channel)
                .bind(&gone)
                .execute(&mut *self.tx)
                .await
                .map_err(se)?;
        }
        let changed: Vec<&Item> = doc
            .items
            .iter()
            .filter(|i| have.get(&i.id).is_none_or(|b| *b != *i))
            .collect();
        if !changed.is_empty() {
            sqlx::query(
                "INSERT INTO jukebox_queue (id, channel_id, seq, track_id, mount_id, key, title, artist, \
                 duration_ms, added_by, state, added_at) \
                 SELECT u.id, $1, u.seq, u.track_id, u.mount_id, u.key, u.title, u.artist, u.duration_ms, \
                 u.added_by, u.state, to_timestamp(u.added_ms / 1000.0) \
                 FROM UNNEST($2::bigint[], $3::bigint[], $4::bigint[], $5::bigint[], $6::text[], $7::text[], \
                 $8::text[], $9::int[], $10::bigint[], $11::text[], $12::bigint[]) \
                 AS u(id, seq, track_id, mount_id, key, title, artist, duration_ms, added_by, state, added_ms) \
                 ON CONFLICT (id) DO UPDATE SET seq = EXCLUDED.seq, state = EXCLUDED.state, \
                 duration_ms = EXCLUDED.duration_ms, title = EXCLUDED.title, artist = EXCLUDED.artist",
            )
            .bind(self.channel)
            .bind(changed.iter().map(|i| i.id).collect::<Vec<_>>())
            .bind(changed.iter().map(|i| i.seq).collect::<Vec<_>>())
            .bind(changed.iter().map(|i| i.track_id).collect::<Vec<_>>())
            .bind(changed.iter().map(|i| i.mount_id).collect::<Vec<_>>())
            .bind(changed.iter().map(|i| i.key.clone()).collect::<Vec<_>>())
            .bind(changed.iter().map(|i| i.title.clone()).collect::<Vec<_>>())
            .bind(changed.iter().map(|i| i.artist.clone()).collect::<Vec<_>>())
            .bind(
                changed
                    .iter()
                    .map(|i| i.duration_ms.map(|d| i32::try_from(d).unwrap_or(i32::MAX)))
                    .collect::<Vec<_>>(),
            )
            .bind(changed.iter().map(|i| i.added_by).collect::<Vec<_>>())
            .bind(changed.iter().map(|i| i.state.as_str().to_owned()).collect::<Vec<_>>())
            .bind(changed.iter().map(|i| i.added_ms).collect::<Vec<_>>())
            .execute(&mut *self.tx)
            .await
            .map_err(se)?;
        }

        // Votes: the rows are a set of (item, user).
        let was: HashSet<(i64, i64)> = base.votes.iter().copied().collect();
        let is: HashSet<(i64, i64)> = doc.votes.iter().copied().collect();
        let removed: Vec<(i64, i64)> = was
            .difference(&is)
            .copied()
            // A vote on a deleted item went with the row (ON DELETE CASCADE).
            .filter(|(item, _)| now_ids.contains(item))
            .collect();
        if !removed.is_empty() {
            sqlx::query(
                "DELETE FROM jukebox_votes v USING UNNEST($1::bigint[], $2::bigint[]) AS d(item_id, user_id) \
                 WHERE v.item_id = d.item_id AND v.user_id = d.user_id",
            )
            .bind(removed.iter().map(|r| r.0).collect::<Vec<_>>())
            .bind(removed.iter().map(|r| r.1).collect::<Vec<_>>())
            .execute(&mut *self.tx)
            .await
            .map_err(se)?;
        }
        let added: Vec<(i64, i64)> = is.difference(&was).copied().collect();
        if !added.is_empty() {
            sqlx::query(
                "INSERT INTO jukebox_votes (item_id, user_id) \
                 SELECT * FROM UNNEST($1::bigint[], $2::bigint[]) ON CONFLICT DO NOTHING",
            )
            .bind(added.iter().map(|r| r.0).collect::<Vec<_>>())
            .bind(added.iter().map(|r| r.1).collect::<Vec<_>>())
            .execute(&mut *self.tx)
            .await
            .map_err(se)?;
        }

        // Listeners: new or newer heartbeats are upserted, the missing are deleted.
        let seen_before: HashMap<i64, i64> = base.listeners.iter().copied().collect();
        let seen_now: HashMap<i64, i64> = doc.listeners.iter().copied().collect();
        let left: Vec<i64> = seen_before
            .keys()
            .filter(|u| !seen_now.contains_key(u))
            .copied()
            .collect();
        if !left.is_empty() {
            sqlx::query(
                "DELETE FROM jukebox_listeners WHERE channel_id = $1 AND user_id = ANY($2)",
            )
            .bind(self.channel)
            .bind(&left)
            .execute(&mut *self.tx)
            .await
            .map_err(se)?;
        }
        let beats: Vec<(i64, i64)> = doc
            .listeners
            .iter()
            .filter(|(u, seen)| seen_before.get(u) != Some(seen))
            .copied()
            .collect();
        if !beats.is_empty() {
            sqlx::query(
                "INSERT INTO jukebox_listeners (channel_id, user_id, seen_at) \
                 SELECT $1, u.user_id, to_timestamp(u.seen_ms / 1000.0) \
                 FROM UNNEST($2::bigint[], $3::bigint[]) AS u(user_id, seen_ms) \
                 ON CONFLICT (channel_id, user_id) DO UPDATE SET seen_at = EXCLUDED.seen_at",
            )
            .bind(self.channel)
            .bind(beats.iter().map(|b| b.0).collect::<Vec<_>>())
            .bind(beats.iter().map(|b| b.1).collect::<Vec<_>>())
            .execute(&mut *self.tx)
            .await
            .map_err(se)?;
        }
        self.base = Some(doc.clone());
        Ok(())
    }

    async fn commit(self: Box<Self>) -> Result<(), JukeboxError> {
        self.tx.commit().await.map_err(se)
    }
}

#[async_trait]
impl JukeboxStore for PgJukebox {
    async fn peek(&self, channel: i64) -> Result<Option<Doc>, JukeboxError> {
        let mut conn = self.pool.acquire().await.map_err(se)?;
        read_doc(&mut conn, channel).await
    }

    async fn begin(&self, channel: i64, guild: i64) -> Result<Box<dyn JukeboxTxn>, JukeboxError> {
        let mut tx = self.pool.begin().await.map_err(se)?;
        // A channel that does not exist (or is in another guild) fails the foreign keys: no row is made.
        sqlx::query(
            "INSERT INTO jukebox_sessions (channel_id, guild_id) \
             SELECT c.id, c.guild_id FROM channels c WHERE c.id = $1 AND c.guild_id = $2 \
             ON CONFLICT (channel_id) DO NOTHING",
        )
        .bind(channel)
        .bind(guild)
        .execute(&mut *tx)
        .await
        .map_err(se)?;
        let locked = sqlx::query("SELECT 1 FROM jukebox_sessions WHERE channel_id = $1 FOR UPDATE")
            .bind(channel)
            .fetch_optional(&mut *tx)
            .await
            .map_err(se)?;
        if locked.is_none() {
            return Err(JukeboxError::NotFound);
        }
        Ok(Box::new(PgTxn {
            tx,
            channel,
            base: None,
        }))
    }
}
