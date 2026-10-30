//! Voice jukebox wiring (MUS-JB-002): the caller's standing in a voice channel from the chat system and the
//! `JUKEBOX_UPDATE` dispatch over the event bus.
//!
//! * [`ChatJukeboxAccess`]: `VIEW_CHANNEL` + `CONNECT` (listen), `SPEAK`, `MANAGE_CHANNELS` in a guild voice
//!   channel, with role and channel overwrites and member timeouts, and the members who may listen. The
//!   audience is cached for a short time so a busy queue does not page the member table on every change.
//! * [`BusJukeboxEvents`]: user-scoped dispatches on each addressed member's topic. The payload names the
//!   channel and the version only; the state itself is read per viewer.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use sr_chat::jukebox::ports::{JukeboxAccess, JukeboxEvents, VoiceAccess};
use sr_chat::repo::ChatRepository;
use sr_chat::threads::access::guild_channel_permissions;
use sr_chat::{ChannelType, perms};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, Clock, UserId};

fn uid(v: i64) -> UserId {
    UserId::new(u64::try_from(v).unwrap_or(0))
}

fn cid(v: i64) -> ChannelId {
    ChannelId::new(u64::try_from(v).unwrap_or(0))
}

/// At most this many members are told about a change.
const AUDIENCE_CAP: usize = 5_000;
/// How long a channel's audience is reused.
const AUDIENCE_TTL: Duration = Duration::from_secs(20);

/// Channel standing over the chat repository.
pub struct ChatJukeboxAccess<R> {
    repo: R,
    clock: Arc<dyn Clock>,
    cache: Mutex<HashMap<i64, (Instant, Vec<i64>)>>,
}

impl<R: ChatRepository> ChatJukeboxAccess<R> {
    /// Wire the access check.
    pub fn new(repo: R, clock: Arc<dyn Clock>) -> Self {
        Self {
            repo,
            clock,
            cache: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl<R: ChatRepository + 'static> JukeboxAccess for ChatJukeboxAccess<R> {
    async fn access(&self, user: i64, channel: i64) -> Result<Option<VoiceAccess>, String> {
        let Some(ch) = self
            .repo
            .get_channel(cid(channel))
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        // Only a guild's voice channel has a jukebox (a DM call has no queue).
        let (Some(gid), ChannelType::Voice) = (ch.guild_id, ch.kind) else {
            return Ok(None);
        };
        let Some(guild) = self.repo.get_guild(gid).await.map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        let Some(member) = self
            .repo
            .get_member(gid, uid(user))
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let roles = self.repo.list_roles(gid).await.map_err(|e| e.to_string())?;
        let p = guild_channel_permissions(
            &self.repo,
            &guild,
            &member,
            &roles,
            &ch,
            self.clock.now_unix_ms(),
        )
        .await
        .map_err(|e| e.to_string())?;
        if !p.has(Permissions::VIEW_CHANNEL) {
            return Ok(None);
        }
        Ok(Some(VoiceAccess {
            guild_id: i64::try_from(gid.get()).map_err(|e| e.to_string())?,
            listen: p.has(Permissions::CONNECT),
            speak: p.has(Permissions::CONNECT) && p.has(Permissions::SPEAK),
            manage: p.has(Permissions::MANAGE_CHANNELS),
        }))
    }

    async fn audience(&self, channel: i64) -> Result<Vec<i64>, String> {
        if let Ok(c) = self.cache.lock()
            && let Some((at, list)) = c.get(&channel)
            && at.elapsed() < AUDIENCE_TTL
        {
            return Ok(list.clone());
        }
        let Some(ch) = self
            .repo
            .get_channel(cid(channel))
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(Vec::new());
        };
        let Some(gid) = ch.guild_id else {
            return Ok(Vec::new());
        };
        let Some(guild) = self.repo.get_guild(gid).await.map_err(|e| e.to_string())? else {
            return Ok(Vec::new());
        };
        let roles = self.repo.list_roles(gid).await.map_err(|e| e.to_string())?;
        let overwrites = self
            .repo
            .get_overwrites(ch.id)
            .await
            .map_err(|e| e.to_string())?;
        let now = self.clock.now_unix_ms();
        let to_i64 = |u: UserId| i64::try_from(u.get()).unwrap_or(0);
        let mut out = Vec::new();
        let mut after: Option<UserId> = None;
        while out.len() < AUDIENCE_CAP {
            let page = self
                .repo
                .page_members(gid, after, 500)
                .await
                .map_err(|e| e.to_string())?;
            for m in &page {
                let p = perms::channel_permissions(&guild, m, &roles, &overwrites, now);
                if p.has(Permissions::VIEW_CHANNEL) && p.has(Permissions::CONNECT) {
                    out.push(to_i64(m.user_id));
                }
            }
            match page.last() {
                Some(last) if page.len() == 500 => after = Some(last.user_id),
                _ => break,
            }
        }
        if let Ok(mut c) = self.cache.lock() {
            c.retain(|_, (at, _)| at.elapsed() < AUDIENCE_TTL);
            c.insert(channel, (Instant::now(), out.clone()));
        }
        Ok(out)
    }
}

/// Publishes as `DomainEvent::UserScoped` on each addressed user's topic.
pub struct BusJukeboxEvents {
    bus: Arc<dyn sr_events::EventBus>,
}

impl BusJukeboxEvents {
    /// Wire the bus.
    pub fn new(bus: Arc<dyn sr_events::EventBus>) -> Self {
        Self { bus }
    }
}

#[async_trait]
impl JukeboxEvents for BusJukeboxEvents {
    async fn emit(&self, users: Vec<i64>, name: &'static str, payload: serde_json::Value) {
        let users: Vec<UserId> = users
            .into_iter()
            .filter_map(|u| u64::try_from(u).ok().map(UserId::new))
            .collect();
        let ev = sr_events::DomainEvent::UserScoped {
            users: users.clone(),
            name: name.to_owned(),
            payload,
        };
        for u in users {
            if let Err(e) = self
                .bus
                .publish(sr_events::Topic::User(u), ev.clone())
                .await
            {
                tracing::warn!(error = %e, event = name, "publishing a jukebox event failed");
            }
        }
    }
}

/// The listening-room flag over Postgres (`listening_rooms`, migration 1580). The cursor job
/// (`jukebox_jobs`) advances the rows; this only creates, removes and reads them.
pub struct PgListeningRooms {
    pool: sqlx::PgPool,
}

impl PgListeningRooms {
    /// Wrap a pool.
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

fn room_err(e: sqlx::Error) -> sr_chat::jukebox::ports::RoomError {
    sr_chat::jukebox::ports::RoomError::Store(e.to_string())
}

#[async_trait]
impl sr_chat::jukebox::ports::ListeningRooms for PgListeningRooms {
    async fn enable(
        &self,
        channel: i64,
        guild: i64,
        playlist: i64,
        by: i64,
    ) -> Result<(), sr_chat::jukebox::ports::RoomError> {
        // Only a playlist that belongs to this very server: a member's own list or another server's is
        // "unknown", the same answer.
        let r = sqlx::query(
            "INSERT INTO listening_rooms (channel_id, guild_id, playlist_id, created_by) \
             SELECT $1, $2, p.id, $4 FROM music_playlists p WHERE p.id = $3 AND p.owner_guild_id = $2 \
             ON CONFLICT (channel_id) DO UPDATE SET playlist_id = EXCLUDED.playlist_id, item_id = NULL, \
               position_ms = 0, cursor_at = now(), seq = listening_rooms.seq + 1, next_due_at = now(), \
               last_published_at = NULL",
        )
        .bind(channel)
        .bind(guild)
        .bind(playlist)
        .bind(by)
        .execute(&self.pool)
        .await
        .map_err(room_err)?;
        if r.rows_affected() == 0 {
            return Err(sr_chat::jukebox::ports::RoomError::UnknownPlaylist);
        }
        Ok(())
    }

    async fn disable(&self, channel: i64) -> Result<bool, sr_chat::jukebox::ports::RoomError> {
        let r = sqlx::query("DELETE FROM listening_rooms WHERE channel_id = $1")
            .bind(channel)
            .execute(&self.pool)
            .await
            .map_err(room_err)?;
        Ok(r.rows_affected() > 0)
    }

    async fn cursor(
        &self,
        channel: i64,
    ) -> Result<Option<serde_json::Value>, sr_chat::jukebox::ports::RoomError> {
        crate::jukebox_jobs::read_room(&self.pool, channel)
            .await
            .map_err(room_err)
    }
}
