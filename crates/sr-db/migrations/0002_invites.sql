-- 0002: platform invites, credit ledger, invite graph. docs/04 section 1, docs/06 section 5.
CREATE TYPE invite_state AS ENUM ('open', 'revoked', 'redeemed');

CREATE TABLE platform_invites (
    id                      BIGINT PRIMARY KEY,
    code                    CITEXT NOT NULL UNIQUE CHECK (code ~ '^[0-9A-Za-z]{10}$'),
    issuer_account_id       BIGINT NOT NULL REFERENCES accounts (id),
    state                   invite_state NOT NULL DEFAULT 'open',
    redeemed_by_account_id  BIGINT REFERENCES accounts (id),
    redeemed_at             TIMESTAMPTZ,
    revoked_at              TIMESTAMPTZ,
    note                    TEXT NOT NULL DEFAULT '',
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((state = 'redeemed') = (redeemed_by_account_id IS NOT NULL)),
    CHECK ((state = 'revoked') = (revoked_at IS NOT NULL))
);
CREATE INDEX platform_invites_issuer_idx ON platform_invites (issuer_account_id, state);
CREATE UNIQUE INDEX platform_invites_redeemer_uidx
    ON platform_invites (redeemed_by_account_id) WHERE redeemed_by_account_id IS NOT NULL;

-- Closes the circular reference from 0001.
ALTER TABLE accounts
    ADD CONSTRAINT accounts_invited_by_invite_fk
    FOREIGN KEY (invited_by_invite_id) REFERENCES platform_invites (id);

CREATE TYPE invite_credit_reason AS ENUM
    ('initial_grant', 'redemption', 'purchase', 'staff_grant', 'staff_revoke');

-- Append-only. Balance = SUM(delta).
CREATE TABLE invite_credits_ledger (
    id          BIGINT PRIMARY KEY,
    account_id  BIGINT NOT NULL REFERENCES accounts (id),
    delta       INTEGER NOT NULL CHECK (delta <> 0),
    reason      invite_credit_reason NOT NULL,
    ref_id      BIGINT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX invite_credits_ledger_account_idx ON invite_credits_ledger (account_id);

CREATE FUNCTION sr_forbid_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION '% is append-only', TG_TABLE_NAME;
END $$;

CREATE TRIGGER invite_credits_ledger_append_only
    BEFORE UPDATE OR DELETE ON invite_credits_ledger
    FOR EACH ROW EXECUTE FUNCTION sr_forbid_mutation();

CREATE VIEW invite_credit_balances AS
    SELECT account_id, SUM(delta)::BIGINT AS balance
    FROM invite_credits_ledger GROUP BY account_id;

-- Edges issuer -> redeemer. The graph is permanent, even across deletion.
CREATE VIEW invite_graph AS
    SELECT pi.id AS invite_id,
           pi.issuer_account_id AS issuer_id,
           pi.redeemed_by_account_id AS redeemer_id,
           pi.redeemed_at,
           r.state AS redeemer_state,
           r.person_id AS redeemer_person_id
    FROM platform_invites pi
    JOIN accounts r ON r.id = pi.redeemed_by_account_id
    WHERE pi.state = 'redeemed';
