-- 0503: application commands and permissions v2 (DEV-BOT-003).
CREATE TABLE application_commands (
    id                          BIGINT PRIMARY KEY,
    application_id              BIGINT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    -- NULL = global.
    guild_id                    BIGINT REFERENCES guilds (id) ON DELETE CASCADE,
    kind                        SMALLINT NOT NULL DEFAULT 1 CHECK (kind BETWEEN 1 AND 3),
    name                        TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 32),
    description                 TEXT NOT NULL DEFAULT '' CHECK (char_length(description) <= 100),
    options                     JSONB NOT NULL DEFAULT '[]'::jsonb,
    name_localizations          JSONB,
    description_localizations   JSONB,
    default_member_permissions  BIGINT,
    dm_permission               BOOLEAN NOT NULL DEFAULT TRUE,
    nsfw                        BOOLEAN NOT NULL DEFAULT FALSE,
    contexts                    JSONB,
    integration_types          JSONB,
    version                     BIGINT NOT NULL,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX application_commands_name_uidx
    ON application_commands (application_id, COALESCE(guild_id, 0), kind, name);
CREATE INDEX application_commands_guild_idx ON application_commands (guild_id) WHERE guild_id IS NOT NULL;

-- `command_id` is a command id, or the application id for the application-wide default.
CREATE TABLE application_command_permissions (
    application_id  BIGINT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    guild_id        BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    command_id      BIGINT NOT NULL,
    target_id       BIGINT NOT NULL,
    -- 1 role, 2 user, 3 channel.
    target_type     SMALLINT NOT NULL CHECK (target_type BETWEEN 1 AND 3),
    allowed         BOOLEAN NOT NULL,
    PRIMARY KEY (application_id, guild_id, command_id, target_type, target_id)
);
