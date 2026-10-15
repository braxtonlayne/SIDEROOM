-- 0700: contribution credits (REV-CORE-006) and the daily upload quota (REV-CORE-008).

-- One credit per NEW published item (movie, episode, chapter), credited to the submitter
-- (D-074). Unique per (section, ref_id): a replacement (same item id) or an item that was
-- already credited inserts nothing. Unpublishing for cause sets `revoked_at`; the row stays,
-- so re-publishing the same item cannot earn a second credit.
CREATE TABLE contribution_credits (
    id            BIGINT PRIMARY KEY,
    section       TEXT NOT NULL,
    ref_id        BIGINT NOT NULL,
    user_id       BIGINT REFERENCES accounts (id) ON DELETE CASCADE,
    submission_id BIGINT REFERENCES submissions (id) ON DELETE SET NULL,
    label         TEXT NOT NULL DEFAULT '',
    granted_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at    TIMESTAMPTZ,
    revoke_reason TEXT,
    UNIQUE (section, ref_id)
);
CREATE INDEX contribution_credits_user_idx ON contribution_credits (user_id) WHERE revoked_at IS NULL;
CREATE INDEX contribution_credits_submission_idx ON contribution_credits (submission_id);

-- Bytes a member has reserved for upload per UTC day (50 GB/day default, staff exempt).
CREATE TABLE submission_upload_bytes (
    user_id BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    day     DATE NOT NULL,
    bytes   BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, day)
);
