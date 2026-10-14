-- 0541: entitlement grants, server levels and donor preferences (DON-PAY-001, DON-ENT, DON-LB).
--
-- Grants are the source of truth. Patron validity and a server's pool are derived from the live
-- (not revoked) grants by sr_payments::entitlements, so a reorg or a staff revocation is just
-- "revoke the grant and recompute".

CREATE TABLE entitlement_grants (
    id         BIGINT PRIMARY KEY,
    kind       TEXT NOT NULL CHECK (kind IN ('patron', 'server_pool')),
    user_id    BIGINT REFERENCES accounts (id),
    guild_id   BIGINT REFERENCES guilds (id) ON DELETE CASCADE,
    source     TEXT NOT NULL CHECK (source IN ('invoice', 'admin', 'migrated')),
    -- The invoice (source = invoice) or the staff account (source = admin).
    source_id  BIGINT,
    -- Patron: days added after the current end; or a fixed end; or forever (staff only).
    days       BIGINT CHECK (days >= 0),
    ends_ms    BIGINT,
    permanent  BOOLEAN NOT NULL DEFAULT FALSE,
    -- Server pool: USD cents contributed.
    usd_cents  BIGINT CHECK (usd_cents > 0),
    starts_ms  BIGINT NOT NULL,
    revoked_ms BIGINT,
    revoked_by BIGINT REFERENCES accounts (id),
    reason     TEXT,
    CHECK (kind <> 'patron' OR user_id IS NOT NULL),
    CHECK (kind <> 'server_pool' OR guild_id IS NOT NULL),
    CHECK (kind <> 'patron' OR permanent OR days IS NOT NULL OR ends_ms IS NOT NULL),
    CHECK (kind <> 'server_pool' OR usd_cents IS NOT NULL),
    CHECK (NOT permanent OR source <> 'invoice')
);
CREATE INDEX entitlement_grants_user_idx ON entitlement_grants (user_id) WHERE kind = 'patron';
CREATE INDEX entitlement_grants_guild_idx ON entitlement_grants (guild_id, starts_ms) WHERE kind = 'server_pool';
CREATE INDEX entitlement_grants_source_idx ON entitlement_grants (source, source_id);

-- A server's current level (the stored level lags the pool by the grace period).
CREATE TABLE guild_support_levels (
    guild_id                BIGINT PRIMARY KEY REFERENCES guilds (id) ON DELETE CASCADE,
    level                   SMALLINT NOT NULL DEFAULT 0 CHECK (level BETWEEN 0 AND 3),
    -- When the pool first fell below the level; the downgrade happens after the grace period.
    below_since_ms          BIGINT,
    -- The owner hides the server from the public board; staff can hide it too.
    hidden_from_leaderboard BOOLEAN NOT NULL DEFAULT FALSE,
    staff_hidden            BOOLEAN NOT NULL DEFAULT FALSE,
    updated_ms              BIGINT NOT NULL DEFAULT 0
);

-- How far the "your perks end" notices got for a Patron (0 none, 1 expiring, 2 grace, 3 lapsed).
CREATE TABLE user_patron_notices (
    user_id BIGINT PRIMARY KEY REFERENCES accounts (id),
    level   SMALLINT NOT NULL DEFAULT 0 CHECK (level BETWEEN 0 AND 3)
);

-- Leaderboard privacy (D-074): the donor can hide the name and/or the amount, and the badge.
CREATE TABLE donor_preferences (
    user_id      BIGINT PRIMARY KEY REFERENCES accounts (id),
    hide_name    BOOLEAN NOT NULL DEFAULT FALSE,
    hide_amount  BOOLEAN NOT NULL DEFAULT FALSE,
    hide_badge   BOOLEAN NOT NULL DEFAULT FALSE,
    staff_hidden BOOLEAN NOT NULL DEFAULT FALSE
);
