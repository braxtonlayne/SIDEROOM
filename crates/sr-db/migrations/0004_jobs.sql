-- 0004: durable job queue (docs/02 section 6). Claimed with FOR UPDATE SKIP LOCKED.
CREATE TYPE job_state AS ENUM ('queued', 'running', 'succeeded', 'dead');

CREATE TABLE jobs (
    id            BIGINT PRIMARY KEY,
    kind          TEXT NOT NULL,
    payload       JSONB NOT NULL DEFAULT '{}'::jsonb,
    state         job_state NOT NULL DEFAULT 'queued',
    priority      SMALLINT NOT NULL DEFAULT 0,
    attempts      INTEGER NOT NULL DEFAULT 0,
    max_attempts  INTEGER NOT NULL DEFAULT 5 CHECK (max_attempts >= 1),
    run_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at     TIMESTAMPTZ,
    locked_by     TEXT,
    last_error    TEXT,
    finished_at   TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX jobs_claim_idx ON jobs (kind, priority DESC, run_at) WHERE state = 'queued';
CREATE INDEX jobs_running_idx ON jobs (locked_at) WHERE state = 'running';
CREATE INDEX jobs_dead_idx ON jobs (finished_at) WHERE state = 'dead';
