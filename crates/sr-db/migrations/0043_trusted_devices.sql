-- 0043: "Trust this device for 30 days" (open question 144). The cookie holds
-- a random 256-bit token; only its SHA-256 digest is stored, and a row only
-- counts while it is unexpired and unrevoked.
CREATE TABLE trusted_devices (
    id           BIGINT PRIMARY KEY,
    account_id   BIGINT NOT NULL REFERENCES accounts (id),
    token_hash   BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    device_name  TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL,
    revoked_at   TIMESTAMPTZ,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX trusted_devices_account_idx ON trusted_devices (account_id) WHERE revoked_at IS NULL;
