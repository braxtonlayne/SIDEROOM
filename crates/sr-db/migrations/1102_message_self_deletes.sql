-- 1102: N-05 (D-324): a member's bulk self-delete of their own messages, run as a job. One row per run:
-- one open run per account (unique index), and the member route allows one run per 24 hours.
CREATE TABLE message_self_deletes (
    id            BIGINT PRIMARY KEY,
    account_id    BIGINT NOT NULL REFERENCES accounts (id),
    state         TEXT NOT NULL DEFAULT 'queued' CHECK (state IN ('queued', 'running', 'done', 'failed')),
    servers       BOOLEAN NOT NULL,
    dms           BOOLEAN NOT NULL,
    before_at     TIMESTAMPTZ,
    via           TEXT NOT NULL DEFAULT 'member' CHECK (via IN ('member', 'staff')),
    requested_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at   TIMESTAMPTZ,
    hidden_count  BIGINT,
    error         TEXT,
    CHECK (servers OR dms)
);
CREATE UNIQUE INDEX message_self_deletes_open_uidx ON message_self_deletes (account_id)
    WHERE state IN ('queued', 'running');
CREATE INDEX message_self_deletes_account_idx ON message_self_deletes (account_id, requested_at DESC);
