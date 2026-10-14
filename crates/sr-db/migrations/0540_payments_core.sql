-- 0540: self-custodial, watch-only Bitcoin payments (DON-PAY-001, ADR-0006, docs/specs/donations.md).
--
-- The database never holds a spending key or seed. `payment_wallets.descriptor` is an
-- operator-supplied WATCH-ONLY output descriptor (`wpkh(xpub.../0/*)`); the CHECK below refuses
-- anything that looks like private key material as a second line of defence behind the parser.
-- Times in the payment tables are Unix milliseconds (the watcher and the state machine work in them).

CREATE TABLE payment_wallets (
    id          BIGINT PRIMARY KEY,
    network     TEXT NOT NULL CHECK (network IN ('mainnet', 'testnet', 'signet', 'regtest')),
    descriptor  TEXT NOT NULL CHECK (
                    descriptor !~* '(xprv|tprv|zprv|vprv|yprv|uprv)'
                    AND char_length(descriptor) BETWEEN 10 AND 600),
    -- First 8 hex characters of SHA-256 of the descriptor: shown in Admin, identifies the wallet.
    fingerprint TEXT NOT NULL CHECK (char_length(fingerprint) = 8),
    -- Next unused derivation index; only ever increases (addresses are never reused).
    next_index  INTEGER NOT NULL DEFAULT 0 CHECK (next_index >= 0),
    gap_limit   INTEGER NOT NULL DEFAULT 20 CHECK (gap_limit BETWEEN 1 AND 1000),
    active      BOOLEAN NOT NULL DEFAULT FALSE,
    created_by  BIGINT REFERENCES accounts (id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (network, fingerprint)
);
CREATE UNIQUE INDEX payment_wallets_one_active ON payment_wallets (active) WHERE active;

-- One JSON document of operator settings (prices, thresholds, perks, depths); see
-- sr_payments::DonationConfig. Absent row = defaults.
CREATE TABLE payment_config (
    id         SMALLINT PRIMARY KEY CHECK (id = 1),
    data       JSONB NOT NULL,
    updated_by BIGINT REFERENCES accounts (id),
    updated_ms BIGINT NOT NULL
);

CREATE TABLE payment_invoices (
    id                     BIGINT PRIMARY KEY,
    wallet_id              BIGINT NOT NULL REFERENCES payment_wallets (id),
    -- Accounts are never hard-deleted while their invoices exist (accounting is kept forever, doc 09).
    user_id                BIGINT NOT NULL REFERENCES accounts (id),
    purpose                TEXT NOT NULL CHECK (purpose IN ('donate', 'patron', 'server_level', 'invite_credit')),
    guild_id               BIGINT REFERENCES guilds (id) ON DELETE SET NULL,
    months                 INTEGER CHECK (months BETWEEN 1 AND 36),
    credits                INTEGER CHECK (credits BETWEEN 1 AND 100),
    -- The price snapshot: USD cents, satoshis, and the rate they were computed with.
    usd_cents              BIGINT NOT NULL CHECK (usd_cents > 0),
    sats                   BIGINT NOT NULL CHECK (sats > 0),
    rate_cents_per_btc     BIGINT NOT NULL CHECK (rate_cents_per_btc > 0),
    rate_source            TEXT NOT NULL,
    rate_stale             BOOLEAN NOT NULL DEFAULT FALSE,
    address                TEXT NOT NULL UNIQUE,
    derivation_index       INTEGER NOT NULL CHECK (derivation_index >= 0),
    network                TEXT NOT NULL,
    state                  TEXT NOT NULL DEFAULT 'open' CHECK (state IN
                           ('open', 'seen', 'confirming', 'confirmed', 'overpaid', 'late_paid',
                            'underpaid', 'expired', 'cancelled', 'resolved')),
    confirmations_required INTEGER NOT NULL CHECK (confirmations_required >= 0),
    confirmations          INTEGER,
    received_sats          BIGINT NOT NULL DEFAULT 0,
    confirmed_sats         BIGINT NOT NULL DEFAULT 0,
    shortfall_sats         BIGINT NOT NULL DEFAULT 0,
    excess_sats            BIGINT NOT NULL DEFAULT 0,
    late                   BOOLEAN NOT NULL DEFAULT FALSE,
    -- The payer's choice for server contributions: show the name or contribute anonymously.
    anonymous              BOOLEAN NOT NULL DEFAULT FALSE,
    created_ms             BIGINT NOT NULL,
    expires_ms             BIGINT NOT NULL,
    quote_expires_ms       BIGINT NOT NULL,
    credited_ms            BIGINT,
    next_check_ms          BIGINT NOT NULL,
    resolution_kind        TEXT CHECK (resolution_kind IN ('mark_paid', 'refund_noted', 'dismiss')),
    resolution_note        TEXT,
    resolved_by            BIGINT REFERENCES accounts (id),
    resolved_ms            BIGINT,
    UNIQUE (wallet_id, derivation_index),
    CHECK ((purpose = 'patron') = (months IS NOT NULL)),
    CHECK ((purpose = 'invite_credit') = (credits IS NOT NULL))
);
CREATE INDEX payment_invoices_user_idx ON payment_invoices (user_id, id DESC);
CREATE INDEX payment_invoices_state_idx ON payment_invoices (state, id DESC);
CREATE INDEX payment_invoices_due_idx ON payment_invoices (next_check_ms)
    WHERE state NOT IN ('cancelled', 'resolved');
CREATE INDEX payment_invoices_guild_idx ON payment_invoices (guild_id) WHERE guild_id IS NOT NULL;

-- Every transaction ever seen paying an invoice address. (invoice_id, txid) is the idempotent
-- match key: polling again updates the row, it never adds a second one.
CREATE TABLE payment_txs (
    invoice_id    BIGINT NOT NULL REFERENCES payment_invoices (id) ON DELETE CASCADE,
    txid          TEXT NOT NULL CHECK (char_length(txid) BETWEEN 8 AND 64),
    amount_sats   BIGINT NOT NULL CHECK (amount_sats > 0),
    block_height  INTEGER,
    confirmations INTEGER NOT NULL DEFAULT 0,
    first_seen_ms BIGINT NOT NULL,
    last_seen_ms  BIGINT NOT NULL,
    -- Set when the chain source stopped showing it (replaced or reorged out); cleared if it returns.
    dropped_ms    BIGINT,
    PRIMARY KEY (invoice_id, txid)
);
CREATE INDEX payment_txs_txid_idx ON payment_txs (txid);

-- Exactly one credit per invoice: the idempotency key of crediting. A reorg sets `active` to
-- false (and the grants are revoked); confirming again re-activates the same row.
CREATE TABLE payment_credits (
    invoice_id     BIGINT PRIMARY KEY REFERENCES payment_invoices (id) ON DELETE CASCADE,
    active         BOOLEAN NOT NULL,
    credited_ms    BIGINT NOT NULL,
    reverted_ms    BIGINT,
    -- What was added to the platform invite ledger, so a revert can subtract the same amount.
    invite_credits INTEGER NOT NULL DEFAULT 0,
    invite_user    BIGINT REFERENCES accounts (id)
);

-- Append-only trail of everything that happened to an invoice.
CREATE TABLE payment_audit (
    id         BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    invoice_id BIGINT NOT NULL REFERENCES payment_invoices (id) ON DELETE CASCADE,
    at_ms      BIGINT NOT NULL,
    kind       TEXT NOT NULL,
    detail     JSONB NOT NULL DEFAULT '{}'::jsonb
);
CREATE INDEX payment_audit_invoice_idx ON payment_audit (invoice_id, id);
CREATE TRIGGER payment_audit_append_only
    BEFORE UPDATE ON payment_audit
    FOR EACH ROW EXECUTE FUNCTION sr_forbid_mutation();

-- Health of the watcher job, for Admin.
CREATE TABLE payment_watcher (
    id          SMALLINT PRIMARY KEY CHECK (id = 1),
    last_run_ms BIGINT,
    last_ok_ms  BIGINT,
    last_error  TEXT,
    tip_height  INTEGER,
    polled      INTEGER NOT NULL DEFAULT 0
);
