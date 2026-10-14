-- 0420: Cinema media assets (CIN-ING, docs/specs/cinema-media.md sections 4.4 and 4.5).
-- An asset is one playable video: the uploaded original, its probe result and, once the ladder
-- job finished, the renditions, audio tracks and subtitle tracks. Ids are application-generated
-- Snowflakes. `video_titles.asset_id` / `video_episodes.asset_id` stay plain references (no
-- foreign key, see 0370); `title_id` / `episode_id` here are the same kind of plain reference
-- and say which catalog item the asset was uploaded for.

CREATE TABLE video_assets (
    id                 BIGINT PRIMARY KEY,
    title_id           BIGINT,
    episode_id         BIGINT,
    uploaded_by        BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    -- awaiting_upload: the slot exists and the presigned PUT was handed out.
    -- uploaded: the object is in storage; probing: the probe job runs; proxy_ready: a review proxy
    -- exists and the ladder waits for approval; in_review / approved: review pipeline;
    -- transcoding: jobs are queued or running; published: the ladder is ready to play;
    -- rejected / removed: retention deletes the original later; failed: see `error`; expired: the
    -- slot was never completed.
    state              TEXT NOT NULL DEFAULT 'awaiting_upload'
                       CHECK (state IN ('awaiting_upload', 'uploaded', 'probing', 'proxy_ready',
                                        'in_review', 'approved', 'transcoding', 'published',
                                        'rejected', 'removed', 'failed', 'expired')),
    error              TEXT,
    filename           TEXT NOT NULL CHECK (char_length(filename) BETWEEN 1 AND 255),
    -- Object key of the original, `originals/<asset id>/source.<ext>`; served as /cdn/originals/...
    original_key       TEXT NOT NULL,
    original_ext       TEXT NOT NULL CHECK (original_ext ~ '^[a-z0-9]{1,8}$'),
    -- Declared at reservation, replaced by the stored size at completion.
    original_size      BIGINT NOT NULL CHECK (original_size > 0),
    -- hot: in the primary bucket; cold: moved to the cold bucket / Storage Box by retention.
    original_tier      TEXT NOT NULL DEFAULT 'hot' CHECK (original_tier IN ('hot', 'cold')),
    -- Staff allow members with the download perk to fetch the original (CIN-DL-001).
    original_downloads BOOLEAN NOT NULL DEFAULT FALSE,
    -- The ladder is queued right after the probe (staff uploads) or waits for approval.
    auto_ladder        BOOLEAN NOT NULL DEFAULT TRUE,
    probe              JSONB,
    duration_ms        BIGINT CHECK (duration_ms IS NULL OR duration_ms >= 0),
    profile_version    INTEGER,
    -- The storage prefix of the published outputs, `transcode/<job id>/<attempt>` (no trailing
    -- slash); served as /cdn/hls/<job id>/<attempt>/... Replaced by a re-transcode; the previous
    -- prefix is collected after the new one is live.
    hls_prefix         TEXT,
    sprite_vtt_path    TEXT,
    poster_paths       TEXT[] NOT NULL DEFAULT '{}',
    -- The review proxy: a single MP4 (storage key), deleted after publishing.
    proxy_key          TEXT,
    published_at       TIMESTAMPTZ,
    ended_at           TIMESTAMPTZ,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX video_assets_title_idx ON video_assets (title_id) WHERE title_id IS NOT NULL;
CREATE INDEX video_assets_episode_idx ON video_assets (episode_id) WHERE episode_id IS NOT NULL;
CREATE INDEX video_assets_state_idx ON video_assets (state, updated_at);
CREATE INDEX video_assets_uploader_idx ON video_assets (uploaded_by, created_at DESC);

-- Video ladder rungs of an asset (one per entry of the plan: 1080p, 720p, 480p).
CREATE TABLE video_renditions (
    id              BIGINT PRIMARY KEY,
    asset_id        BIGINT NOT NULL REFERENCES video_assets (id) ON DELETE CASCADE,
    job_id          BIGINT NOT NULL,
    name            TEXT NOT NULL,
    width           INTEGER NOT NULL,
    height          INTEGER NOT NULL,
    bandwidth_bps   BIGINT NOT NULL,
    frame_rate      DOUBLE PRECISION NOT NULL,
    codecs          TEXT NOT NULL,
    -- Paths relative to the asset prefix (`v/1080p/index.m3u8`, `v/1080p/init.mp4`).
    playlist_path   TEXT NOT NULL,
    init_path       TEXT NOT NULL,
    -- sr_media::worker_api::SegmentIndex: segment names, durations and sizes.
    segment_index   JSONB NOT NULL,
    size_bytes      BIGINT NOT NULL DEFAULT 0,
    profile_version INTEGER NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (asset_id, name)
);

CREATE TABLE video_audio_tracks (
    id              BIGINT PRIMARY KEY,
    asset_id        BIGINT NOT NULL REFERENCES video_assets (id) ON DELETE CASCADE,
    job_id          BIGINT NOT NULL,
    ordinal         INTEGER NOT NULL,
    name            TEXT NOT NULL,
    language        TEXT,
    language_tag    TEXT,
    channels        INTEGER NOT NULL,
    is_default      BOOLEAN NOT NULL DEFAULT FALSE,
    playlist_path   TEXT NOT NULL,
    init_path       TEXT NOT NULL,
    segment_index   JSONB NOT NULL,
    profile_version INTEGER NOT NULL,
    UNIQUE (asset_id, ordinal)
);

CREATE TABLE video_subtitle_tracks (
    id               BIGINT PRIMARY KEY,
    asset_id         BIGINT NOT NULL REFERENCES video_assets (id) ON DELETE CASCADE,
    job_id           BIGINT NOT NULL,
    ordinal          INTEGER NOT NULL,
    name             TEXT NOT NULL,
    language         TEXT,
    language_tag     TEXT,
    forced           BOOLEAN NOT NULL DEFAULT FALSE,
    hearing_impaired BOOLEAN NOT NULL DEFAULT FALSE,
    is_default       BOOLEAN NOT NULL DEFAULT FALSE,
    vtt_path         TEXT NOT NULL,
    -- Kept when the source was ASS/SSA, for styled rendering in the player.
    ass_path         TEXT,
    UNIQUE (asset_id, ordinal)
);
