//! The jukebox state of one voice channel.

use std::fmt;

/// Most tracks that may wait in one channel's queue.
pub const MAX_QUEUED: usize = 200;
/// Most waiting tracks one member may have at a time (fairness).
pub const MAX_QUEUED_PER_MEMBER: usize = 25;
/// Most tracks one add call may carry.
pub const MAX_ADD_BATCH: usize = 50;
/// Played and skipped rows kept for the "recently played" strip; older ones are pruned.
pub const KEEP_PLAYED: usize = 20;
/// A listener who has not been seen for this long no longer counts toward the vote ratio.
pub const LISTENER_TTL_MS: i64 = 60_000;
/// A client's "track ended" is accepted this early before the known end (client clock skew).
pub const END_SLACK_MS: i64 = 3_000;
/// Default of `jukebox.vote_skip_percent` (the roadmap's `jukebox.vote_skip_ratio` 0.5).
pub const DEFAULT_VOTE_PERCENT: i64 = 50;

/// How the channel hears the music.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// One DJ sends the audio into the call as a music track (MUS-JB-001).
    Stream,
    /// Every listener plays the track from a mount they may read, aligned to the server clock.
    Synced,
}

impl Mode {
    /// Wire and column value.
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Stream => "stream",
            Mode::Synced => "synced",
        }
    }

    /// Parse the wire value.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "stream" => Some(Mode::Stream),
            "synced" => Some(Mode::Synced),
            _ => None,
        }
    }
}

/// Where a queue row stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemState {
    /// Waiting its turn.
    Queued,
    /// The current track.
    Playing,
    /// Played to its end.
    Played,
    /// Skipped by votes or by the DJ.
    Skipped,
}

impl ItemState {
    /// Column value.
    pub fn as_str(self) -> &'static str {
        match self {
            ItemState::Queued => "queued",
            ItemState::Playing => "playing",
            ItemState::Played => "played",
            ItemState::Skipped => "skipped",
        }
    }

    /// Parse the column value.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(ItemState::Queued),
            "playing" => Some(ItemState::Playing),
            "played" => Some(ItemState::Played),
            "skipped" => Some(ItemState::Skipped),
            _ => None,
        }
    }
}

/// One channel's jukebox row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// The voice channel.
    pub channel_id: i64,
    /// Its guild.
    pub guild_id: i64,
    /// Stream or synced.
    pub mode: Mode,
    /// The DJ, when one holds the decks.
    pub dj: Option<i64>,
    /// The queue row that is playing.
    pub current: Option<i64>,
    /// Server clock (unix ms) at which the current track was at `position_ms` (while not paused).
    pub started_ms: Option<i64>,
    /// Offset into the current track at `started_ms`, or the frozen position while paused.
    pub position_ms: i64,
    /// The DJ paused the track.
    pub paused: bool,
    /// Bumped by every change; clients refetch when it moves.
    pub version: i64,
}

impl Session {
    /// A fresh, idle session.
    pub fn new(channel_id: i64, guild_id: i64) -> Self {
        Self {
            channel_id,
            guild_id,
            mode: Mode::Synced,
            dj: None,
            current: None,
            started_ms: None,
            position_ms: 0,
            paused: false,
            version: 1,
        }
    }
}

/// One track in the queue. The mount id and key are what the listener's presign is made from, at play time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Row snowflake.
    pub id: i64,
    /// Order within the channel.
    pub seq: i64,
    /// The index row it came from (may be gone later).
    pub track_id: Option<i64>,
    /// The mount the audio lives in.
    pub mount_id: i64,
    /// Mount-relative key of the audio file.
    pub key: String,
    /// Title snapshot (shown only to listeners who can read the mount).
    pub title: String,
    /// Artist snapshot.
    pub artist: String,
    /// Length, when the index knows it.
    pub duration_ms: Option<i64>,
    /// Who queued it.
    pub added_by: Option<i64>,
    /// Where it stands.
    pub state: ItemState,
    /// When it was queued, unix ms.
    pub added_ms: i64,
}

impl Item {
    /// The length when it is usable for advancing (a zero length counts as unknown).
    pub fn known_len(&self) -> Option<i64> {
        self.duration_ms.filter(|d| *d > 0)
    }
}

/// A track a member asks to queue, already checked against their reach by the routes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTrack {
    /// The index row.
    pub track_id: i64,
    /// The mount.
    pub mount_id: i64,
    /// The key in the mount.
    pub key: String,
    /// Title.
    pub title: String,
    /// Artist.
    pub artist: String,
    /// Length.
    pub duration_ms: Option<i64>,
}

/// Everything about one channel's jukebox, loaded and saved as a unit under the session lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doc {
    /// The session row.
    pub session: Session,
    /// Queue rows (queued, the playing one and the last few played), in `seq` order.
    pub items: Vec<Item>,
    /// `(item_id, user_id)` skip votes.
    pub votes: Vec<(i64, i64)>,
    /// `(user_id, seen_ms)` listeners.
    pub listeners: Vec<(i64, i64)>,
}

impl Doc {
    /// A fresh doc for a channel.
    pub fn new(channel_id: i64, guild_id: i64) -> Self {
        Self {
            session: Session::new(channel_id, guild_id),
            items: Vec::new(),
            votes: Vec::new(),
            listeners: Vec::new(),
        }
    }

    /// The playing item.
    pub fn current(&self) -> Option<&Item> {
        let id = self.session.current?;
        self.items.iter().find(|i| i.id == id)
    }

    /// Waiting items, in play order.
    pub fn queued(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(|i| i.state == ItemState::Queued)
    }
}

/// What a vote did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoteOutcome {
    /// Votes now standing for the current track, from live listeners.
    pub votes: usize,
    /// Votes needed.
    pub needed: usize,
    /// The vote tipped it and the track was skipped.
    pub skipped: bool,
}

/// Why a jukebox call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JukeboxError {
    /// No such queue row (or not in this channel).
    NotFound,
    /// The queue holds [`MAX_QUEUED`] tracks.
    QueueFull,
    /// The member already has [`MAX_QUEUED_PER_MEMBER`] tracks waiting.
    MemberQueueFull,
    /// Nothing is playing.
    NothingPlaying,
    /// The caller may not do that (a DJ-only or moderator-only action).
    NotAllowed,
    /// The named DJ is not listening.
    NotListening,
    /// The store failed.
    Store(String),
}

impl fmt::Display for JukeboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JukeboxError::NotFound => f.write_str("unknown queue entry"),
            JukeboxError::QueueFull => f.write_str("the queue is full"),
            JukeboxError::MemberQueueFull => f.write_str("too many of your tracks are waiting"),
            JukeboxError::NothingPlaying => f.write_str("nothing is playing"),
            JukeboxError::NotAllowed => f.write_str("not allowed"),
            JukeboxError::NotListening => f.write_str("that member is not listening"),
            JukeboxError::Store(m) => write!(f, "store: {m}"),
        }
    }
}

impl std::error::Error for JukeboxError {}
