-- 0011: channels, categories, permission overwrites, DMs. docs/04 section 2, LNG-CHN-001.
-- Categories are channels of type 4. DMs and group DMs have guild_id NULL.
CREATE TABLE moderation_holds (
    id          BIGINT PRIMARY KEY,
    reason      TEXT NOT NULL DEFAULT '',
    created_by  BIGINT REFERENCES accounts (id),
    case_ref    TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE channels (
    id                            BIGINT PRIMARY KEY,
    guild_id                      BIGINT REFERENCES guilds (id) ON DELETE CASCADE,
    -- Discord enum: 0 text, 1 DM, 2 voice, 3 group DM, 4 category, 5 announcement,
    -- 10/11/12 threads, 13 stage, 15 forum, 16 media.
    type                          SMALLINT NOT NULL
                                  CHECK (type IN (0, 1, 2, 3, 4, 5, 10, 11, 12, 13, 15, 16)),
    name                          TEXT CHECK (char_length(name) BETWEEN 1 AND 100),
    topic                         TEXT CHECK (char_length(topic) <= 4096),
    position                      INTEGER NOT NULL DEFAULT 0,
    parent_id                     BIGINT REFERENCES channels (id) ON DELETE SET NULL,
    nsfw                          BOOLEAN NOT NULL DEFAULT FALSE,
    rate_limit_per_user           INTEGER NOT NULL DEFAULT 0 CHECK (rate_limit_per_user BETWEEN 0 AND 21600),
    bitrate                       INTEGER,
    user_limit                    INTEGER CHECK (user_limit BETWEEN 0 AND 99),
    rtc_region                    TEXT,
    video_quality_mode            SMALLINT,
    default_auto_archive_duration INTEGER,
    default_thread_rate_limit     INTEGER,
    available_tags                JSONB,
    default_reaction_emoji        JSONB,
    default_sort_order            SMALLINT,
    default_forum_layout          SMALLINT,
    last_message_id               BIGINT,
    owner_id                      BIGINT REFERENCES accounts (id),
    icon_key                      TEXT,
    flags                         INTEGER NOT NULL DEFAULT 0,
    created_at                    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                    TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Guild channels have a guild; DMs/group DMs must not.
    CHECK ((type IN (1, 3)) = (guild_id IS NULL))
);
CREATE INDEX channels_guild_idx ON channels (guild_id, position) WHERE guild_id IS NOT NULL;
CREATE INDEX channels_parent_idx ON channels (parent_id) WHERE parent_id IS NOT NULL;

-- Close the circular references from 0010.
ALTER TABLE guilds
    ADD CONSTRAINT guilds_system_channel_fk FOREIGN KEY (system_channel_id) REFERENCES channels (id) ON DELETE SET NULL,
    ADD CONSTRAINT guilds_rules_channel_fk FOREIGN KEY (rules_channel_id) REFERENCES channels (id) ON DELETE SET NULL,
    ADD CONSTRAINT guilds_public_updates_channel_fk FOREIGN KEY (public_updates_channel_id) REFERENCES channels (id) ON DELETE SET NULL,
    ADD CONSTRAINT guilds_afk_channel_fk FOREIGN KEY (afk_channel_id) REFERENCES channels (id) ON DELETE SET NULL;

CREATE TYPE overwrite_target AS ENUM ('role', 'member');

-- target_id is a role id or a user id depending on target_type, so it has no FK.
CREATE TABLE permission_overwrites (
    channel_id   BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    target_id    BIGINT NOT NULL,
    target_type  overwrite_target NOT NULL,
    allow        BIGINT NOT NULL DEFAULT 0,
    deny         BIGINT NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (channel_id, target_id),
    CHECK (allow & deny = 0)
);

-- DMs and group DMs.
CREATE TABLE channel_recipients (
    channel_id  BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    user_id     BIGINT NOT NULL REFERENCES accounts (id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (channel_id, user_id)
);
CREATE INDEX channel_recipients_user_idx ON channel_recipients (user_id);

-- One 1:1 DM per user pair (user_a < user_b), for POST /users/@me/channels reuse.
CREATE TABLE dm_channels (
    channel_id  BIGINT PRIMARY KEY REFERENCES channels (id) ON DELETE CASCADE,
    user_a      BIGINT NOT NULL REFERENCES accounts (id),
    user_b      BIGINT NOT NULL REFERENCES accounts (id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (user_a < user_b),
    UNIQUE (user_a, user_b)
);
CREATE INDEX dm_channels_user_b_idx ON dm_channels (user_b);
