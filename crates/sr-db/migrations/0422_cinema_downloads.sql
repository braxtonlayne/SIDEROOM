-- 0422: Patron downloads (CIN-DL-001, docs/specs/cinema-media.md section 5).

-- Members who may download, and how many a day. Written by the donations work (perks, doc 08) or
-- by staff; there is no other source of the perk yet. A NULL `expires_at` never expires.
CREATE TABLE cinema_download_grants (
    user_id    BIGINT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    per_day    INTEGER NOT NULL DEFAULT 20 CHECK (per_day BETWEEN 0 AND 1000),
    expires_at TIMESTAMPTZ,
    note       TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One prepared MP4 per asset and rendition, shared by every member who asks for it while it is
-- cached (7 days after it became ready).
CREATE TABLE cinema_download_cache (
    id            BIGINT PRIMARY KEY,
    asset_id      BIGINT NOT NULL REFERENCES video_assets (id) ON DELETE CASCADE,
    rendition     TEXT NOT NULL,
    state         TEXT NOT NULL DEFAULT 'queued'
                  CHECK (state IN ('queued', 'preparing', 'ready', 'failed', 'expired')),
    job_id        BIGINT REFERENCES transcode_jobs (id) ON DELETE SET NULL,
    object_key    TEXT,
    size_bytes    BIGINT,
    error         TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    ready_at      TIMESTAMPTZ,
    expires_at    TIMESTAMPTZ
);
-- At most one live (not failed, not expired) preparation per asset and rendition.
CREATE UNIQUE INDEX cinema_download_cache_live ON cinema_download_cache (asset_id, rendition)
    WHERE state IN ('queued', 'preparing', 'ready');
CREATE INDEX cinema_download_cache_expiry ON cinema_download_cache (expires_at) WHERE state = 'ready';
CREATE INDEX cinema_download_cache_job ON cinema_download_cache (job_id) WHERE job_id IS NOT NULL;

-- A member's request. The daily limit counts these rows; the link (`/cinema/downloads/{id}/file`)
-- only works for `user_id` and redirects to a short-lived signed URL.
CREATE TABLE cinema_downloads (
    id         BIGINT PRIMARY KEY,
    user_id    BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    title_id   BIGINT NOT NULL,
    episode_id BIGINT,
    asset_id   BIGINT NOT NULL REFERENCES video_assets (id) ON DELETE CASCADE,
    -- A rendition name (`1080p`) or `original`.
    rendition  TEXT NOT NULL,
    cache_id   BIGINT REFERENCES cinema_download_cache (id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX cinema_downloads_user_idx ON cinema_downloads (user_id, created_at DESC);
