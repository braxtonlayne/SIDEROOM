-- 0022: notification settings. docs/04 section 4 (guild_member_settings), LNG-NTF-001.
-- guild_id 0 is the DM / group DM scope (no NULLs in the key; same trick as feature_enablements).
CREATE TABLE notification_settings (
    user_id               BIGINT NOT NULL REFERENCES accounts (id),
    guild_id              BIGINT NOT NULL DEFAULT 0,
    -- 0 all, 1 only mentions, 2 nothing; NULL inherits guilds.default_message_notifications.
    level                 SMALLINT CHECK (level IN (0, 1, 2)),
    suppress_everyone     BOOLEAN NOT NULL DEFAULT FALSE,
    suppress_roles        BOOLEAN NOT NULL DEFAULT FALSE,
    suppress_highlights   BOOLEAN NOT NULL DEFAULT FALSE,
    mute_scheduled_events BOOLEAN NOT NULL DEFAULT FALSE,
    mobile_push           BOOLEAN NOT NULL DEFAULT TRUE,
    muted                 BOOLEAN NOT NULL DEFAULT FALSE,
    -- NULL while muted = until turned off.
    mute_end_at           TIMESTAMPTZ,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, guild_id),
    CHECK (muted OR mute_end_at IS NULL)
);

-- Channel and category overrides (a category is a channel row).
CREATE TABLE channel_notification_overrides (
    user_id      BIGINT NOT NULL REFERENCES accounts (id),
    channel_id   BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    level        SMALLINT CHECK (level IN (0, 1, 2)),
    muted        BOOLEAN NOT NULL DEFAULT FALSE,
    mute_end_at  TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, channel_id),
    CHECK (muted OR mute_end_at IS NULL)
);
CREATE INDEX channel_notification_overrides_channel_idx ON channel_notification_overrides (channel_id);
