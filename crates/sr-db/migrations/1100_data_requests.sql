-- 1100: the data-subject request queue (M-25 step c, MOD-DSR; D-295, D-315, docs/specs/moderation-compliance.md
-- section 6). One row per request for an export or an erasure. `due_at` is computed from
-- `compliance.data_request_deadline_days` when the row is created and never recomputed; one documented
-- extension may be added (`extended_until`, GDPR article 12(3)). Rows are kept forever (D-124, D-212): the
-- table has no delete path and no cascade, and it references the account (never removed) and the Person.
CREATE TABLE data_requests (
    id                BIGINT PRIMARY KEY,
    kind              TEXT NOT NULL CHECK (kind IN ('export', 'erasure')),
    person_id         BIGINT NOT NULL REFERENCES persons (id),
    account_id        BIGINT NOT NULL REFERENCES accounts (id),
    state             TEXT NOT NULL DEFAULT 'open'
                      CHECK (state IN ('open', 'verified', 'running', 'completed', 'declined')),
    note              TEXT NOT NULL DEFAULT '',
    received_at       TIMESTAMPTZ NOT NULL,
    due_at            TIMESTAMPTZ NOT NULL,
    extended_until    TIMESTAMPTZ,
    extension_reason  TEXT,
    extended_by       BIGINT REFERENCES accounts (id),
    extended_at       TIMESTAMPTZ,
    verified_by       BIGINT REFERENCES accounts (id),
    verified_at       TIMESTAMPTZ,
    export_id         BIGINT REFERENCES data_exports (id),
    outcome           TEXT,
    retained          JSONB NOT NULL DEFAULT '[]'::jsonb,
    completed_at      TIMESTAMPTZ,
    created_by        BIGINT NOT NULL REFERENCES accounts (id),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (due_at >= received_at),
    -- The extension is all-or-nothing and moves the date forward; it can be recorded once.
    CHECK ((extended_until IS NULL) = (extension_reason IS NULL)
       AND (extended_until IS NULL) = (extended_by IS NULL)
       AND (extended_until IS NULL) = (extended_at IS NULL)),
    CHECK (extended_until IS NULL OR extended_until > due_at),
    CHECK (state NOT IN ('completed', 'declined') OR (completed_at IS NOT NULL AND outcome IS NOT NULL))
);
CREATE INDEX data_requests_queue_idx ON data_requests (due_at) WHERE state IN ('open', 'verified', 'running');
CREATE INDEX data_requests_person_idx ON data_requests (person_id, id DESC);
CREATE UNIQUE INDEX data_requests_export_uidx ON data_requests (export_id) WHERE export_id IS NOT NULL;
-- One open request of a kind per account, so a repeat click never queues the same work twice.
CREATE UNIQUE INDEX data_requests_open_uidx ON data_requests (account_id, kind)
    WHERE state IN ('open', 'verified', 'running');
