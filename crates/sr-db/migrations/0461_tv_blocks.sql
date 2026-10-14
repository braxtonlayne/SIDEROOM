-- 0461: Live TV schedule blocks (TV-ENG-001). Blocks of one channel never overlap (the service keeps them
-- apart); times are Unix milliseconds. title_id, episode_id and asset_id are plain references to the Cinema
-- catalog (no foreign keys: the catalog lives in another crate); the display fields are a snapshot taken
-- when the block is made, so the guide needs no catalog query.

CREATE TABLE tv_blocks (
    id             BIGINT PRIMARY KEY,
    channel_id     BIGINT NOT NULL REFERENCES tv_channels (id) ON DELETE CASCADE,
    start_ms       BIGINT NOT NULL,
    end_ms         BIGINT NOT NULL,
    kind           TEXT NOT NULL CHECK (kind IN ('program', 'filler', 'slate', 'off_air')),
    title_id       BIGINT,
    episode_id     BIGINT,
    asset_id       BIGINT,
    title          TEXT NOT NULL,
    episode_title  TEXT,
    media_kind     TEXT CHECK (media_kind IS NULL OR media_kind IN ('movie', 'series', 'anime')),
    season_number  INTEGER,
    episode_number INTEGER,
    year           SMALLINT,
    age_restricted BOOLEAN NOT NULL DEFAULT FALSE,
    -- Position inside the asset at start_ms.
    offset_ms      BIGINT NOT NULL DEFAULT 0 CHECK (offset_ms >= 0),
    source         TEXT NOT NULL CHECK (source IN ('generated', 'manual', 'override')),
    -- Generator state after this block (generated blocks), so the schedule can continue from here.
    cursor_after   JSONB,
    -- HLS numbering of the block's first on-air segment and its discontinuity counter (live playlists).
    seq_base       BIGINT,
    disc_base      BIGINT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (end_ms > start_ms),
    UNIQUE (channel_id, start_ms)
);
CREATE INDEX tv_blocks_window_idx ON tv_blocks (channel_id, end_ms);
CREATE INDEX tv_blocks_cursor_idx ON tv_blocks (channel_id, end_ms) WHERE cursor_after IS NOT NULL;
