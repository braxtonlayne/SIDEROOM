-- 0502: installs and release versions (DEV-APP-004/005/007).
CREATE TABLE application_installs (
    application_id  BIGINT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    guild_id        BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    installed_by    BIGINT NOT NULL REFERENCES accounts (id),
    scopes          TEXT[] NOT NULL,
    permissions     BIGINT NOT NULL DEFAULT 0,
    role_id         BIGINT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (application_id, guild_id)
);
CREATE INDEX application_installs_guild_idx ON application_installs (guild_id);

CREATE TYPE app_version_state AS ENUM ('pending', 'approved', 'rejected', 'changes_requested', 'withdrawn');

CREATE TABLE application_versions (
    id                    BIGINT PRIMARY KEY,
    application_id        BIGINT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    version               INTEGER NOT NULL CHECK (version >= 1),
    manifest              JSONB NOT NULL,
    needs_review_reasons  TEXT[] NOT NULL DEFAULT '{}',
    note                  TEXT CHECK (char_length(note) <= 2000),
    state                 app_version_state NOT NULL DEFAULT 'pending',
    submitted_by          BIGINT NOT NULL REFERENCES accounts (id),
    reviewer_id           BIGINT REFERENCES accounts (id),
    reviewer_note         TEXT CHECK (char_length(reviewer_note) <= 2000),
    decided_at            TIMESTAMPTZ,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (application_id, version)
);
CREATE UNIQUE INDEX application_versions_pending_uidx ON application_versions (application_id) WHERE state = 'pending';
CREATE INDEX application_versions_state_idx ON application_versions (state, id DESC);
