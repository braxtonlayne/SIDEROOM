-- 0892: Ingest sources (REV-UP-005, REV-UP-006, ADM-CNT-005). A watched folder is polled for new
-- Library files; a storage import is run on demand after a dry run. Both create draft submissions
-- owned by a staff account. The ledger remembers every file seen, so a file is imported once and
-- is only picked up after it has stopped changing.

CREATE TABLE ingest_sources (
    id             BIGINT PRIMARY KEY,
    label          TEXT NOT NULL CHECK (char_length(label) BETWEEN 1 AND 100),
    kind           TEXT NOT NULL CHECK (kind IN ('watched_folder', 'storage_import')),
    backend        TEXT NOT NULL CHECK (backend IN ('local', 'storage')),
    -- A directory under an operator-approved root, or a key prefix below `imports/`.
    location       TEXT NOT NULL CHECK (char_length(location) BETWEEN 1 AND 500),
    owner_id       BIGINT NOT NULL REFERENCES accounts (id) ON DELETE RESTRICT,
    enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    poll_secs      INTEGER NOT NULL DEFAULT 300 CHECK (poll_secs BETWEEN 30 AND 86400),
    stable_secs    INTEGER NOT NULL DEFAULT 60 CHECK (stable_secs BETWEEN 5 AND 3600),
    last_scan_at   TIMESTAMPTZ,
    last_scan      JSONB,
    last_error     TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ingest_sources_due_idx ON ingest_sources (last_scan_at NULLS FIRST)
    WHERE enabled AND kind = 'watched_folder';

CREATE TABLE ingest_files (
    source_id       BIGINT NOT NULL REFERENCES ingest_sources (id) ON DELETE CASCADE,
    rel_path        TEXT NOT NULL CHECK (char_length(rel_path) BETWEEN 1 AND 1000),
    size_bytes      BIGINT NOT NULL,
    mtime_ms        BIGINT NOT NULL,
    etag            TEXT,
    -- Since when size, modification time and etag have not changed.
    stable_since_ms BIGINT NOT NULL,
    state           TEXT NOT NULL CHECK (state IN ('seen', 'imported', 'skipped', 'failed')),
    submission_id   BIGINT,
    error           TEXT,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (source_id, rel_path)
);
