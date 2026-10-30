-- MUS-JB-002 (wave 31): the voice jukebox. One `jukebox_sessions` row per voice channel that ever used it,
-- a queue, the skip votes and the listeners the vote ratio is counted against.
--
-- * `mode`: `stream` (one DJ sends the audio into the call, MUS-JB-001) or `synced` (each listener plays the
--   track from a mount they may read, aligned to the server clock).
-- * The playing position is derived, never stored per tick: `position_ms` at `started_at` while not paused,
--   `position_ms` alone while paused. Everyone derives the same position from the server clock.
-- * `jukebox_queue` keeps the track's mount id and key (a snapshot, so the row outlives the index). It does
--   NOT grant anything: the presign is issued per listener at play time, only for a mount that listener can
--   read (routes::voice::jukebox). Title and artist are a snapshot shown only to listeners who can read the
--   mount; others see "not shared with you".
-- * `jukebox_listeners` are the members who said they are listening (a heartbeat); a vote counts while its
--   voter is a live listener.

CREATE TABLE jukebox_sessions (
    channel_id      BIGINT PRIMARY KEY REFERENCES channels (id) ON DELETE CASCADE,
    guild_id        BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    mode            TEXT NOT NULL DEFAULT 'synced' CHECK (mode IN ('stream', 'synced')),
    dj_user_id      BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    current_item_id BIGINT,
    started_at      TIMESTAMPTZ,
    position_ms     INTEGER NOT NULL DEFAULT 0 CHECK (position_ms >= 0),
    paused          BOOLEAN NOT NULL DEFAULT FALSE,
    version         BIGINT NOT NULL DEFAULT 1,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE jukebox_queue (
    id          BIGINT PRIMARY KEY,
    channel_id  BIGINT NOT NULL REFERENCES jukebox_sessions (channel_id) ON DELETE CASCADE,
    seq         BIGINT NOT NULL,
    track_id    BIGINT,
    mount_id    BIGINT NOT NULL,
    key         TEXT NOT NULL CHECK (char_length(key) BETWEEN 1 AND 1024),
    title       TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 300),
    artist      TEXT NOT NULL DEFAULT '',
    duration_ms INTEGER CHECK (duration_ms >= 0),
    added_by    BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    state       TEXT NOT NULL DEFAULT 'queued' CHECK (state IN ('queued', 'playing', 'played', 'skipped')),
    added_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX jukebox_queue_order_idx ON jukebox_queue (channel_id, state, seq);

CREATE TABLE jukebox_votes (
    item_id    BIGINT NOT NULL REFERENCES jukebox_queue (id) ON DELETE CASCADE,
    user_id    BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (item_id, user_id)
);

CREATE TABLE jukebox_listeners (
    channel_id BIGINT NOT NULL REFERENCES jukebox_sessions (channel_id) ON DELETE CASCADE,
    user_id    BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    seen_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (channel_id, user_id)
);
