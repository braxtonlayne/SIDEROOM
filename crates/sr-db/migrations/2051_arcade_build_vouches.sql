-- 2051: ARC-SBX-001 (wave 42), hash-pinned community vouches.
--
-- A vouch is "I played this exact build and it is fine". It is pinned to `code_sha256`: when the developer ships
-- different code the hash differs, the vouches for the old hash stay where they were (shown as "previous build")
-- and the new hash starts at zero. One vouch per member per build. The read side counts only members in good
-- standing at the time of the read (an active, non-bot account at least three days old), so a deleted or banned
-- account's vouch stops counting without a cleanup job.
CREATE TABLE arcade_build_vouches (
    application_id BIGINT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    code_sha256    TEXT NOT NULL CHECK (code_sha256 ~ '^[0-9a-f]{64}$'),
    account_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (application_id, code_sha256, account_id)
);
CREATE INDEX arcade_build_vouches_account_idx ON arcade_build_vouches (account_id, created_at DESC);
