-- 0421: Transcode workers, tokens and the job queue (docs/specs/cinema-media.md 4.4 and 4.5).

-- Worker tokens are created in Admin / Transcoding; only the SHA-256 of the secret is stored.
CREATE TABLE worker_tokens (
    id            BIGINT PRIMARY KEY,
    name          TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 64),
    token_sha256  TEXT NOT NULL UNIQUE CHECK (token_sha256 ~ '^[0-9a-f]{64}$'),
    created_by    BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at  TIMESTAMPTZ,
    revoked_at    TIMESTAMPTZ
);

CREATE TABLE transcode_workers (
    id            TEXT PRIMARY KEY,
    token_id      BIGINT NOT NULL REFERENCES worker_tokens (id) ON DELETE CASCADE,
    name          TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 64),
    version       TEXT NOT NULL,
    capabilities  JSONB NOT NULL,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at  TIMESTAMPTZ NOT NULL,
    drain         BOOLEAN NOT NULL DEFAULT FALSE,
    UNIQUE (token_id, name)
);

-- One row per job. `kind` is proxy, ladder or remux (the Patron download).
CREATE TABLE transcode_jobs (
    id               BIGINT PRIMARY KEY,
    asset_id         BIGINT NOT NULL REFERENCES video_assets (id) ON DELETE CASCADE,
    kind             TEXT NOT NULL CHECK (kind IN ('proxy', 'ladder', 'remux')),
    -- sr_media::worker_api::JobSpec (the plan travels with the job).
    spec             JSONB NOT NULL,
    source_key       TEXT NOT NULL,
    source_size      BIGINT NOT NULL CHECK (source_size >= 0),
    source_ext       TEXT NOT NULL,
    state            TEXT NOT NULL DEFAULT 'queued'
                     CHECK (state IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
    priority         SMALLINT NOT NULL DEFAULT 5,
    attempt          INTEGER NOT NULL DEFAULT 0,
    worker_id        TEXT REFERENCES transcode_workers (id) ON DELETE SET NULL,
    lease_expires_at TIMESTAMPTZ,
    cancel_requested BOOLEAN NOT NULL DEFAULT FALSE,
    progress         JSONB,
    error            TEXT,
    log_tail         TEXT,
    encoder          TEXT,
    outputs          JSONB NOT NULL DEFAULT '[]'::jsonb,
    segment_index    JSONB NOT NULL DEFAULT '[]'::jsonb,
    wall_secs        DOUBLE PRECISION,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at       TIMESTAMPTZ,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at      TIMESTAMPTZ
);
CREATE INDEX transcode_jobs_claim ON transcode_jobs (priority DESC, id) WHERE state = 'queued';
CREATE INDEX transcode_jobs_lease ON transcode_jobs (lease_expires_at) WHERE state = 'running';
CREATE INDEX transcode_jobs_asset ON transcode_jobs (asset_id, id DESC);
CREATE INDEX transcode_jobs_state ON transcode_jobs (state, id DESC);

-- Outputs the worker started to upload (presigned multipart), one row per output path.
CREATE TABLE transcode_uploads (
    job_id    BIGINT NOT NULL REFERENCES transcode_jobs (id) ON DELETE CASCADE,
    upload_id TEXT NOT NULL,
    path      TEXT NOT NULL,
    key       TEXT NOT NULL,
    size      BIGINT NOT NULL,
    finished  BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (job_id, path)
);
CREATE INDEX transcode_uploads_upload_idx ON transcode_uploads (job_id, upload_id);
