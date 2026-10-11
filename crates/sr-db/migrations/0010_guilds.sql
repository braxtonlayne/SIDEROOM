-- 0010: guilds, roles, members, bans. docs/04 section 2, docs/07, LNG-GLD-001.
-- Numbered from 0010 to leave 0005-0009 free for parallel work.
-- Ids are application-generated Snowflakes. Channel FKs from guilds are added in 0011.
CREATE TABLE guilds (
    id                          BIGINT PRIMARY KEY,
    name                        TEXT NOT NULL CHECK (char_length(name) BETWEEN 2 AND 100),
    icon_key                    TEXT,
    banner_key                  TEXT,
    splash_key                  TEXT,
    owner_id                    BIGINT NOT NULL REFERENCES accounts (id),
    description                 TEXT,
    features                    TEXT[] NOT NULL DEFAULT '{}',
    verification_level          SMALLINT NOT NULL DEFAULT 0 CHECK (verification_level BETWEEN 0 AND 4),
    default_message_notifications SMALLINT NOT NULL DEFAULT 0 CHECK (default_message_notifications BETWEEN 0 AND 1),
    explicit_content_filter     SMALLINT NOT NULL DEFAULT 0 CHECK (explicit_content_filter BETWEEN 0 AND 2),
    mfa_level                   SMALLINT NOT NULL DEFAULT 0 CHECK (mfa_level BETWEEN 0 AND 1),
    system_channel_id           BIGINT,
    rules_channel_id            BIGINT,
    public_updates_channel_id   BIGINT,
    afk_channel_id              BIGINT,
    afk_timeout                 INTEGER NOT NULL DEFAULT 300 CHECK (afk_timeout >= 0),
    preferred_locale            TEXT NOT NULL DEFAULT 'en-US',
    vanity_code                 CITEXT UNIQUE,
    nsfw_level                  SMALLINT NOT NULL DEFAULT 0 CHECK (nsfw_level BETWEEN 0 AND 3),
    is_staff_guild              BOOLEAN NOT NULL DEFAULT FALSE,
    boost_level                 SMALLINT NOT NULL DEFAULT 0,
    traits                      JSONB NOT NULL DEFAULT '[]'::jsonb,
    settings                    JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- Exactly one staff guild (doc 04 section 2).
CREATE UNIQUE INDEX guilds_staff_guild_uidx ON guilds (is_staff_guild) WHERE is_staff_guild;
CREATE INDEX guilds_owner_idx ON guilds (owner_id);

-- The @everyone role has id = guild id, position 0.
CREATE TABLE roles (
    id                    BIGINT PRIMARY KEY,
    guild_id              BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    name                  TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    color                 INTEGER NOT NULL DEFAULT 0 CHECK (color BETWEEN 0 AND 16777215),
    secondary_color       INTEGER CHECK (secondary_color BETWEEN 0 AND 16777215),
    icon_key              TEXT,
    unicode_emoji         TEXT,
    hoist                 BOOLEAN NOT NULL DEFAULT FALSE,
    position              INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
    permissions           BIGINT NOT NULL DEFAULT 0,
    -- Only meaningful on Staff Server roles (doc 07 section 3).
    platform_permissions  BIGINT NOT NULL DEFAULT 0,
    mentionable           BOOLEAN NOT NULL DEFAULT FALSE,
    managed               BOOLEAN NOT NULL DEFAULT FALSE,
    tags                  JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX roles_guild_idx ON roles (guild_id, position);

CREATE TABLE guild_members (
    guild_id                    BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    user_id                     BIGINT NOT NULL REFERENCES accounts (id),
    nick                        TEXT CHECK (char_length(nick) BETWEEN 1 AND 32),
    avatar_key                  TEXT,
    banner_key                  TEXT,
    bio                         TEXT,
    joined_at                   TIMESTAMPTZ NOT NULL DEFAULT now(),
    premium_since               TIMESTAMPTZ,
    communication_disabled_until TIMESTAMPTZ,
    flags                       INTEGER NOT NULL DEFAULT 0,
    pending                     BOOLEAN NOT NULL DEFAULT FALSE,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, user_id)
);
CREATE INDEX guild_members_user_idx ON guild_members (user_id);

-- Role assignments. Normalised instead of doc 04's `roles bigint[]` so role
-- deletion cascades (see docs/open-questions.md). The implicit @everyone role is not stored.
CREATE TABLE member_roles (
    guild_id   BIGINT NOT NULL,
    user_id    BIGINT NOT NULL,
    role_id    BIGINT NOT NULL REFERENCES roles (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, user_id, role_id),
    FOREIGN KEY (guild_id, user_id) REFERENCES guild_members (guild_id, user_id) ON DELETE CASCADE
);
CREATE INDEX member_roles_role_idx ON member_roles (role_id);

CREATE TABLE guild_bans (
    guild_id   BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    user_id    BIGINT NOT NULL REFERENCES accounts (id),
    reason     TEXT,
    banned_by  BIGINT REFERENCES accounts (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, user_id)
);
