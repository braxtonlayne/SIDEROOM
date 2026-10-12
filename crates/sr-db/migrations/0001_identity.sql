-- 0001: identity. See docs/04-data-model.md section 1 and docs/06.
-- Ids are application-generated Snowflakes (BIGINT), never DB-generated.
CREATE EXTENSION IF NOT EXISTS citext;

CREATE TYPE account_state AS ENUM ('active', 'deactivated', 'deleted', 'platform_banned');

CREATE TABLE persons (
    id               BIGINT PRIMARY KEY,
    notes            TEXT NOT NULL DEFAULT '',
    suspicion_level  SMALLINT NOT NULL DEFAULT 0 CHECK (suspicion_level >= 0),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE accounts (
    id                    BIGINT PRIMARY KEY,
    person_id             BIGINT NOT NULL REFERENCES persons (id),
    username              CITEXT NOT NULL UNIQUE
                          CHECK (username ~ '^[a-z0-9_.]{2,32}$'),
    display_name          TEXT,
    email                 CITEXT NOT NULL UNIQUE,
    email_verified_at     TIMESTAMPTZ,
    password_hash         TEXT NOT NULL,
    state                 account_state NOT NULL DEFAULT 'active',
    state_changed_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    age_confirmed_at      TIMESTAMPTZ,
    age_confirmed         BOOLEAN,
    locale                TEXT NOT NULL DEFAULT 'en-US',
    timezone              TEXT NOT NULL DEFAULT 'UTC',
    flags                 BIGINT NOT NULL DEFAULT 0,
    invited_by_invite_id  BIGINT, -- FK added in 0002 (circular with platform_invites)
    avatar_key            TEXT,
    banner_key            TEXT,
    accent_color          INTEGER,
    bio                   TEXT,
    pronouns              TEXT,
    profile_doc           JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX accounts_person_idx ON accounts (person_id);
-- Cap check (D-013): only active accounts count.
CREATE INDEX accounts_active_idx ON accounts (id) WHERE state = 'active';

CREATE TYPE external_provider AS ENUM ('discord');

-- Unique across ALL accounts including deleted ones (D-030).
CREATE TABLE external_identities (
    id                BIGINT PRIMARY KEY,
    account_id        BIGINT NOT NULL REFERENCES accounts (id),
    provider          external_provider NOT NULL,
    provider_user_id  TEXT NOT NULL,
    linked_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    raw_profile       JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, provider_user_id)
);
CREATE INDEX external_identities_account_idx ON external_identities (account_id);

-- Tokens are stored as SHA-256 hashes (32 bytes).
CREATE TABLE sessions (
    id             BIGINT PRIMARY KEY,
    account_id     BIGINT NOT NULL REFERENCES accounts (id),
    token_hash     BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    ip             INET,
    user_agent     TEXT,
    device_label   TEXT,
    last_seen_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at     TIMESTAMPTZ NOT NULL,
    revoked_at     TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX sessions_account_idx ON sessions (account_id) WHERE revoked_at IS NULL;
CREATE INDEX sessions_expires_idx ON sessions (expires_at);

-- Login IP history, kept 180 days (doc 09).
CREATE TABLE login_events (
    id          BIGINT PRIMARY KEY,
    account_id  BIGINT NOT NULL REFERENCES accounts (id),
    ip          INET,
    success     BOOLEAN NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX login_events_account_idx ON login_events (account_id, created_at DESC);
CREATE INDEX login_events_created_idx ON login_events (created_at);

CREATE TABLE mfa_totp (
    id               BIGINT PRIMARY KEY,
    account_id       BIGINT NOT NULL UNIQUE REFERENCES accounts (id),
    secret_enc       BYTEA NOT NULL, -- encrypted at rest by the app
    confirmed_at     TIMESTAMPTZ,
    last_used_step   BIGINT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE webauthn_credentials (
    id             BIGINT PRIMARY KEY,
    account_id     BIGINT NOT NULL REFERENCES accounts (id),
    credential_id  BYTEA NOT NULL UNIQUE,
    public_key     BYTEA NOT NULL,
    sign_count     BIGINT NOT NULL DEFAULT 0,
    transports     TEXT[] NOT NULL DEFAULT '{}',
    name           TEXT NOT NULL DEFAULT '',
    discoverable   BOOLEAN NOT NULL DEFAULT false,
    last_used_at   TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX webauthn_account_idx ON webauthn_credentials (account_id);

CREATE TABLE recovery_codes (
    id          BIGINT PRIMARY KEY,
    account_id  BIGINT NOT NULL REFERENCES accounts (id),
    code_hash   BYTEA NOT NULL CHECK (octet_length(code_hash) = 32),
    used_at     TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (account_id, code_hash)
);

CREATE TYPE email_token_purpose AS ENUM ('verify_email', 'reset_password', 'change_email');

CREATE TABLE email_tokens (
    id          BIGINT PRIMARY KEY,
    account_id  BIGINT NOT NULL REFERENCES accounts (id),
    purpose     email_token_purpose NOT NULL,
    email       CITEXT NOT NULL,
    token_hash  BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    expires_at  TIMESTAMPTZ NOT NULL,
    used_at     TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX email_tokens_account_idx ON email_tokens (account_id, purpose);
