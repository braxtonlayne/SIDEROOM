-- 0504: OAuth2 grants, authorization codes and tokens (DEV-APP-007). Digests only.
CREATE TABLE oauth2_grants (
    id              BIGINT PRIMARY KEY,
    application_id  BIGINT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    user_id         BIGINT NOT NULL REFERENCES accounts (id),
    scopes          TEXT[] NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at      TIMESTAMPTZ
);
CREATE UNIQUE INDEX oauth2_grants_live_uidx ON oauth2_grants (application_id, user_id) WHERE revoked_at IS NULL;
CREATE INDEX oauth2_grants_user_idx ON oauth2_grants (user_id) WHERE revoked_at IS NULL;

CREATE TABLE oauth2_codes (
    code_hash       BYTEA PRIMARY KEY CHECK (octet_length(code_hash) = 32),
    grant_id        BIGINT NOT NULL REFERENCES oauth2_grants (id) ON DELETE CASCADE,
    redirect_uri    TEXT NOT NULL,
    scopes          TEXT[] NOT NULL,
    code_challenge  TEXT,
    expires_at      TIMESTAMPTZ NOT NULL,
    used_at         TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX oauth2_codes_expiry_idx ON oauth2_codes (expires_at);

CREATE TABLE oauth2_tokens (
    id                  BIGINT PRIMARY KEY,
    grant_id            BIGINT NOT NULL REFERENCES oauth2_grants (id) ON DELETE CASCADE,
    -- Set when the pair came from a code; used to revoke on code replay.
    code_hash           BYTEA,
    access_hash         BYTEA NOT NULL UNIQUE CHECK (octet_length(access_hash) = 32),
    refresh_hash        BYTEA NOT NULL UNIQUE CHECK (octet_length(refresh_hash) = 32),
    scopes              TEXT[] NOT NULL,
    access_expires_at   TIMESTAMPTZ NOT NULL,
    refresh_expires_at  TIMESTAMPTZ NOT NULL,
    refresh_used_at     TIMESTAMPTZ,
    revoked_at          TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX oauth2_tokens_grant_idx ON oauth2_tokens (grant_id);
