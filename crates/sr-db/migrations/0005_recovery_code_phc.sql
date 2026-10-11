-- 0005: recovery codes are argon2id PHC strings (sr-auth::recovery), not
-- 32-byte digests, so 0001's `code_hash BYTEA CHECK (octet_length = 32)` cannot
-- hold them. No implementation has written to this table yet, so it is
-- recreated. Salted hashes cannot be looked up by value; the application
-- verifies a presented code against `list_unused` rows.
DROP TABLE recovery_codes;

CREATE TABLE recovery_codes (
    id          BIGINT PRIMARY KEY,
    account_id  BIGINT NOT NULL REFERENCES accounts (id),
    code_phc    TEXT NOT NULL,
    used_at     TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX recovery_codes_account_idx ON recovery_codes (account_id) WHERE used_at IS NULL;
