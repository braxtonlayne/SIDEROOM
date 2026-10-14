-- 0500: developer platform foundations. docs/specs/developer-platform.md, doc 11 section 2.
-- Bot accounts are ordinary rows in `accounts` marked `is_bot`; they never log in
-- (the password hash is a non-PHC marker) and never count toward the account cap.
ALTER TABLE accounts ADD COLUMN is_bot BOOLEAN NOT NULL DEFAULT FALSE;
CREATE INDEX accounts_bot_idx ON accounts (id) WHERE is_bot;

-- "Apply to build apps" (DEV-APP-002). Approval sets accounts.flags bit 1 (APP_DEVELOPER).
CREATE TYPE developer_request_state AS ENUM ('pending', 'approved', 'rejected', 'suspended');

CREATE TABLE developer_requests (
    id                 BIGINT PRIMARY KEY,
    account_id         BIGINT NOT NULL REFERENCES accounts (id),
    pitch              TEXT NOT NULL CHECK (char_length(pitch) BETWEEN 1 AND 2000),
    prior_work         TEXT NOT NULL DEFAULT '' CHECK (char_length(prior_work) <= 2000),
    terms_accepted_at  TIMESTAMPTZ NOT NULL,
    state              developer_request_state NOT NULL DEFAULT 'pending',
    reviewer_id        BIGINT REFERENCES accounts (id),
    reviewer_note      TEXT CHECK (char_length(reviewer_note) <= 2000),
    reviewed_at        TIMESTAMPTZ,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX developer_requests_open_uidx ON developer_requests (account_id) WHERE state = 'pending';
CREATE INDEX developer_requests_state_idx ON developer_requests (state, id DESC);
CREATE INDEX developer_requests_account_idx ON developer_requests (account_id, id DESC);
