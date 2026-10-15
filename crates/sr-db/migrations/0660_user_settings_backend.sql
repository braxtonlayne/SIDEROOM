-- 0660: user-settings backend (phase 11).
-- LNG-PRF-001 profile fields, LNG-USET-007 account deletion, LNG-USET-020 data export,
-- LNG-USET-024 connections.

-- ---- LNG-PRF-001: decoration, effect, two-colour profile theme -------------------------
-- Slugs from the approved packs (free, D-053); the pack catalogue is a client concern, the
-- server only checks the slug shape. Theme colours are 24-bit RGB like `accent_color`.
ALTER TABLE accounts
    ADD COLUMN avatar_decoration  TEXT CHECK (avatar_decoration ~ '^[a-z0-9-]{1,48}$'),
    ADD COLUMN profile_effect     TEXT CHECK (profile_effect ~ '^[a-z0-9-]{1,48}$'),
    ADD COLUMN theme_primary      INTEGER CHECK (theme_primary BETWEEN 0 AND 16777215),
    ADD COLUMN theme_accent       INTEGER CHECK (theme_accent BETWEEN 0 AND 16777215);

-- Per-server overrides (nick, avatar_key, banner_key, bio) already live on guild_members.

-- ---- LNG-USET-007: account deletion (D-029, doc 09) -------------------------------------
-- The account row stays (D-124); the visible fields are scrubbed to "Deleted User". The
-- original identifiers are archived here for staff (they see the full record and the Person
-- link) and the username stays reserved until `username_release_at` (30 days).
CREATE TABLE deleted_accounts (
    account_id           BIGINT PRIMARY KEY REFERENCES accounts (id),
    username             CITEXT NOT NULL,
    email                CITEXT NOT NULL,
    display_name         TEXT,
    deleted_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    username_release_at  TIMESTAMPTZ NOT NULL,
    credits_forfeited    INTEGER NOT NULL DEFAULT 0,
    invites_revoked      INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX deleted_accounts_username_idx ON deleted_accounts (username, username_release_at);

-- The forfeit shows in the append-only ledger.
ALTER TYPE invite_credit_reason ADD VALUE IF NOT EXISTS 'account_deleted';

-- ---- LNG-USET-020: data export ----------------------------------------------------------
CREATE TABLE data_exports (
    id            BIGINT PRIMARY KEY,
    user_id       BIGINT NOT NULL REFERENCES accounts (id),
    state         TEXT NOT NULL DEFAULT 'pending'
                  CHECK (state IN ('pending', 'building', 'ready', 'failed', 'expired')),
    requested_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    ready_at      TIMESTAMPTZ,
    expires_at    TIMESTAMPTZ,
    storage_key   TEXT,
    size_bytes    BIGINT,
    error         TEXT,
    CHECK (state <> 'ready' OR (storage_key IS NOT NULL AND expires_at IS NOT NULL))
);
CREATE INDEX data_exports_user_idx ON data_exports (user_id, requested_at DESC);
CREATE UNIQUE INDEX data_exports_open_uidx
    ON data_exports (user_id) WHERE state IN ('pending', 'building');

-- ---- LNG-USET-024: connections ----------------------------------------------------------
-- Tokens are never stored: the OAuth exchange proves the account once and the public stats
-- are fetched from public endpoints.
ALTER TABLE user_connections
    ADD COLUMN show_activity     BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN url               TEXT CHECK (char_length(url) <= 300),
    ADD COLUMN stats             JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN stats_refreshed_at TIMESTAMPTZ;
-- One account per provider identity platform-wide (a verified link cannot be claimed twice).
CREATE UNIQUE INDEX user_connections_verified_uidx
    ON user_connections (provider, external_id) WHERE verified;
