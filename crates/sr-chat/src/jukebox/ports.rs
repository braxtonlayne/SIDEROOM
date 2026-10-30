//! What the jukebox needs from the rest of the server: the caller's standing in a voice channel and the
//! people to tell when it changes.

use async_trait::async_trait;

/// What a member may do in a voice channel, as far as the jukebox goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceAccess {
    /// The channel's guild.
    pub guild_id: i64,
    /// `VIEW_CHANNEL` and `CONNECT`: may see the jukebox and listen.
    pub listen: bool,
    /// `SPEAK`: may queue tracks and pick the mode.
    pub speak: bool,
    /// `MANAGE_CHANNELS` (or administrator): may remove, skip, pause and hand the decks over.
    pub manage: bool,
}

/// Channel standing, computed from guild roles and overwrites by the chat system.
#[async_trait]
pub trait JukeboxAccess: Send + Sync {
    /// `None` when the channel is not a voice channel of a guild the member belongs to and can see.
    async fn access(&self, user: i64, channel: i64) -> Result<Option<VoiceAccess>, String>;
    /// Members who can see and connect to the channel (capped by the implementation).
    async fn audience(&self, channel: i64) -> Result<Vec<i64>, String>;
}

/// No channels: every call is refused (tests without a chat stack).
pub struct NoAccess;

#[async_trait]
impl JukeboxAccess for NoAccess {
    async fn access(&self, _user: i64, _channel: i64) -> Result<Option<VoiceAccess>, String> {
        Ok(None)
    }
    async fn audience(&self, _channel: i64) -> Result<Vec<i64>, String> {
        Ok(Vec::new())
    }
}

/// Dispatched to the audience when the jukebox changes. The payload carries ids and the version only; a
/// client reads `GET /channels/{id}/jukebox` for the state, which is filtered per viewer.
pub const EVENT_JUKEBOX_UPDATE: &str = "JUKEBOX_UPDATE";
/// Dispatched by the 24/7 listening-room job (MUS-JB-003); registered here so both waves name it once.
pub const EVENT_LISTENING_ROOM_UPDATE: &str = "LISTENING_ROOM_UPDATE";

/// Delivers user-scoped gateway events.
#[async_trait]
pub trait JukeboxEvents: Send + Sync {
    /// Send `payload` as `name` to each of `users`.
    async fn emit(&self, users: Vec<i64>, name: &'static str, payload: serde_json::Value);
}

/// Drops every event (tests).
pub struct NoEvents;

#[async_trait]
impl JukeboxEvents for NoEvents {
    async fn emit(&self, _users: Vec<i64>, _name: &'static str, _payload: serde_json::Value) {}
}

/// Why a listening-room call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomError {
    /// No such server playlist in this server.
    UnknownPlaylist,
    /// The store failed.
    Store(String),
}

/// The 24/7 listening-room flag of a voice channel (MUS-JB-003 owns the cursor job; these are its REST doors).
#[async_trait]
pub trait ListeningRooms: Send + Sync {
    /// Make `channel` a listening room on a server playlist of `guild` (or change its playlist; the cursor
    /// restarts at the top).
    async fn enable(
        &self,
        channel: i64,
        guild: i64,
        playlist: i64,
        by: i64,
    ) -> Result<(), RoomError>;
    /// Stop being a room. Returns whether it was one.
    async fn disable(&self, channel: i64) -> Result<bool, RoomError>;
    /// The room's cursor as `LISTENING_ROOM_UPDATE` carries it, read now; `None` when it is not a room.
    async fn cursor(&self, channel: i64) -> Result<Option<serde_json::Value>, RoomError>;
}

/// No rooms (tests).
pub struct NoRooms;

#[async_trait]
impl ListeningRooms for NoRooms {
    async fn enable(&self, _: i64, _: i64, _: i64, _: i64) -> Result<(), RoomError> {
        Err(RoomError::UnknownPlaylist)
    }
    async fn disable(&self, _: i64) -> Result<bool, RoomError> {
        Ok(false)
    }
    async fn cursor(&self, _: i64) -> Result<Option<serde_json::Value>, RoomError> {
        Ok(None)
    }
}
