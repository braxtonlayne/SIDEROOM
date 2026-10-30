-- MUS-JB-003 (wave 31): 24/7 Listening Room channels.
--
-- A voice channel is a listening room exactly when it has a row here (the flag IS the row: a second boolean on
-- `channels` could drift from it). The room is a CURSOR over a server playlist (`music_playlists.owner_guild_id`):
-- the server never streams audio (D-201), it only says "item N, this many milliseconds in, as of this moment",
-- and every listener plays the file locally from the server shelf, synced to that cursor.
--
-- * `item_id` is the playlist item the cursor is on (NULL until the first tick, or when nothing in the
--   playlist can be scheduled); `position_ms` is how far into that item the cursor was at `cursor_at`.
--   Items whose track has no known duration are skipped by the job (a cursor cannot know when they end).
-- * `seq` counts track changes (and loops), so a client can tell "same track, later position" from "next track".
-- * `next_due_at` is when the job must look at the room again: the earlier of the next track boundary and the
--   next 15 second publication. The job picks only rooms that are due.
-- * `mode = 'stream'` is the DJ's stay (the DJ sends the music track through the voice channel, MUS-JB-001);
--   `stream_dj_id` is then the DJ and the cursor is frozen. It returns to `cursor` when the DJ leaves.
CREATE TABLE listening_rooms (
    channel_id        BIGINT PRIMARY KEY REFERENCES channels (id) ON DELETE CASCADE,
    guild_id          BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    playlist_id       BIGINT NOT NULL REFERENCES music_playlists (id) ON DELETE CASCADE,
    mode              TEXT NOT NULL DEFAULT 'cursor' CHECK (mode IN ('cursor', 'stream')),
    stream_dj_id      BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    item_id           BIGINT REFERENCES music_playlist_items (id) ON DELETE SET NULL,
    position_ms       BIGINT NOT NULL DEFAULT 0 CHECK (position_ms >= 0),
    cursor_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    seq               BIGINT NOT NULL DEFAULT 0,
    next_due_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_published_at TIMESTAMPTZ,
    created_by        BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((mode = 'stream') = (stream_dj_id IS NOT NULL))
);
CREATE INDEX listening_rooms_due_idx ON listening_rooms (next_due_at) WHERE mode = 'cursor';
CREATE INDEX listening_rooms_guild_idx ON listening_rooms (guild_id);
CREATE INDEX listening_rooms_playlist_idx ON listening_rooms (playlist_id);
