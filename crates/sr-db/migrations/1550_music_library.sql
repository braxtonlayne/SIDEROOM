-- MUS-LIB-001 (wave 30): the member's music library on top of `music_tracks` (migration 1450).
--
-- * `music_playlists`: owned by a member or by a server (exactly one of the two owner columns). A server's
--   playlist is readable by its members and editable with the storage `upload` (or `manage`) grant.
-- * `music_playlist_items`: the same track may appear twice; `position` is a dense 0-based order the API keeps.
-- * `music_favorites`: one row per member and track.
-- * `music_recent`: one row per member and track (replaying moves the row to the top); the API prunes to 200.
--
-- Rows follow the track: when the index deletes a track the items, favorites and history rows go with it.
-- Whether a viewer may still PLAY a listed track is decided per request by the storage scopes, never here.

CREATE TABLE music_playlists (
    id             BIGINT PRIMARY KEY,
    owner_user_id  BIGINT REFERENCES accounts (id) ON DELETE CASCADE,
    owner_guild_id BIGINT REFERENCES guilds (id) ON DELETE CASCADE,
    name           TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    description    TEXT NOT NULL DEFAULT '' CHECK (char_length(description) <= 500),
    created_by     BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((owner_user_id IS NOT NULL) <> (owner_guild_id IS NOT NULL))
);
CREATE INDEX music_playlists_user_idx ON music_playlists (owner_user_id, id) WHERE owner_user_id IS NOT NULL;
CREATE INDEX music_playlists_guild_idx ON music_playlists (owner_guild_id, id) WHERE owner_guild_id IS NOT NULL;

CREATE TABLE music_playlist_items (
    id          BIGINT PRIMARY KEY,
    playlist_id BIGINT NOT NULL REFERENCES music_playlists (id) ON DELETE CASCADE,
    track_id    BIGINT NOT NULL REFERENCES music_tracks (id) ON DELETE CASCADE,
    position    INTEGER NOT NULL CHECK (position >= 0),
    added_by    BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    added_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX music_playlist_items_order_idx ON music_playlist_items (playlist_id, position, id);
CREATE INDEX music_playlist_items_track_idx ON music_playlist_items (track_id);

CREATE TABLE music_favorites (
    user_id    BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    track_id   BIGINT NOT NULL REFERENCES music_tracks (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, track_id)
);
CREATE INDEX music_favorites_user_idx ON music_favorites (user_id, created_at DESC);

CREATE TABLE music_recent (
    user_id   BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    track_id  BIGINT NOT NULL REFERENCES music_tracks (id) ON DELETE CASCADE,
    played_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, track_id)
);
CREATE INDEX music_recent_user_idx ON music_recent (user_id, played_at DESC);
