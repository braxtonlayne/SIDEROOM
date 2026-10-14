-- 0501: applications, bot tokens, application audit trail (DEV-APP-001/003).
CREATE TYPE app_release_state AS ENUM ('development', 'in_review', 'released', 'suspended');

CREATE TABLE applications (
    id                      BIGINT PRIMARY KEY,
    owner_id                BIGINT NOT NULL REFERENCES accounts (id),
    bot_user_id             BIGINT NOT NULL UNIQUE REFERENCES accounts (id),
    name                    TEXT NOT NULL CHECK (char_length(name) BETWEEN 2 AND 32),
    description             TEXT NOT NULL DEFAULT '' CHECK (char_length(description) <= 400),
    icon_key                TEXT,
    tags                    TEXT[] NOT NULL DEFAULT '{}',
    bot_public              BOOLEAN NOT NULL DEFAULT FALSE,
    bot_require_code_grant  BOOLEAN NOT NULL DEFAULT FALSE,
    redirect_uris           TEXT[] NOT NULL DEFAULT '{}',
    install_params          JSONB NOT NULL DEFAULT '{"scopes":["bot","applications.commands"],"permissions":"0"}'::jsonb,
    terms_url               TEXT CHECK (char_length(terms_url) <= 512),
    privacy_url             TEXT CHECK (char_length(privacy_url) <= 512),
    release_state           app_release_state NOT NULL DEFAULT 'development',
    -- State before `suspended`, restored by unsuspend.
    state_before_suspend    app_release_state,
    suspended_reason        TEXT,
    -- Privileged intent bits (GUILD_MEMBERS 1<<1, GUILD_PRESENCES 1<<8, MESSAGE_CONTENT 1<<15).
    intents_requested       BIGINT NOT NULL DEFAULT 0,
    intents_approved        BIGINT NOT NULL DEFAULT 0,
    client_secret_hash      BYTEA NOT NULL CHECK (octet_length(client_secret_hash) = 32),
    client_secret_hint      TEXT NOT NULL DEFAULT '',
    secret_rotated_at       TIMESTAMPTZ,
    flags                   BIGINT NOT NULL DEFAULT 0,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX applications_owner_idx ON applications (owner_id, id);
CREATE INDEX applications_state_idx ON applications (release_state);

-- One live token per application; only the SHA-256 digest is stored.
CREATE TABLE bot_tokens (
    id              BIGINT PRIMARY KEY,
    application_id  BIGINT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    user_id         BIGINT NOT NULL REFERENCES accounts (id),
    token_hash      BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at      TIMESTAMPTZ,
    last_used_at    TIMESTAMPTZ
);
CREATE INDEX bot_tokens_app_idx ON bot_tokens (application_id) WHERE revoked_at IS NULL;

-- Application audit trail. No FK so a deletion record outlives the application; rows
-- never hold secrets. Append-only.
CREATE TABLE application_audit (
    id              BIGINT PRIMARY KEY,
    application_id  BIGINT NOT NULL,
    actor_id        BIGINT NOT NULL,
    action          TEXT NOT NULL CHECK (char_length(action) <= 64),
    details         JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX application_audit_app_idx ON application_audit (application_id, id DESC);

CREATE FUNCTION sr_application_audit_immutable() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'application_audit is append-only';
END;
$$;
CREATE TRIGGER application_audit_no_update
    BEFORE UPDATE ON application_audit
    FOR EACH ROW EXECUTE FUNCTION sr_application_audit_immutable();
