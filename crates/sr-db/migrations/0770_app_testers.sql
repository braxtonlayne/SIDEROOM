-- 0770: application testers (G-01, D-204). The owner invites up to 25 accounts; an accepted
-- tester may install a `development` application in a guild of at most 25 members where they
-- hold MANAGE_GUILD. The cap is enforced by the repository under a row lock on the
-- application. Declining an invitation deletes the row.
CREATE TABLE app_testers (
    id              BIGINT PRIMARY KEY,
    application_id  BIGINT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    account_id      BIGINT NOT NULL REFERENCES accounts (id),
    state           TEXT NOT NULL DEFAULT 'invited' CHECK (state IN ('invited', 'accepted')),
    invited_by      BIGINT NOT NULL REFERENCES accounts (id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (application_id, account_id)
);
CREATE INDEX app_testers_account_idx ON app_testers (account_id);
