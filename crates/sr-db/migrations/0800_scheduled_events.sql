-- 0800: guild scheduled events with Discord parity (P12 lounge events).
-- A voice or stage event points at a channel of the server; an external one carries a
-- location and an end time. `status` follows Discord (1 scheduled, 2 active, 3 completed,
-- 4 canceled). Members mark themselves interested (`GUILD_SCHEDULED_EVENT_USER_ADD/REMOVE`).
-- When the channel of an open voice or stage event is deleted the column becomes NULL and the
-- scheduler cancels (or completes) the event on its next run.
CREATE TABLE guild_scheduled_events (
    id               BIGINT PRIMARY KEY,
    guild_id         BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    channel_id       BIGINT REFERENCES channels (id) ON DELETE SET NULL,
    creator_id       BIGINT NOT NULL REFERENCES accounts (id),
    name             TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    description      TEXT CHECK (char_length(description) <= 1000),
    scheduled_start  TIMESTAMPTZ NOT NULL,
    scheduled_end    TIMESTAMPTZ,
    status           SMALLINT NOT NULL DEFAULT 1 CHECK (status BETWEEN 1 AND 4),
    -- 1 stage instance, 2 voice, 3 external.
    entity_type      SMALLINT NOT NULL CHECK (entity_type BETWEEN 1 AND 3),
    location         TEXT CHECK (char_length(location) <= 100),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (scheduled_end IS NULL OR scheduled_end > scheduled_start)
);
CREATE INDEX guild_scheduled_events_guild_idx
    ON guild_scheduled_events (guild_id, scheduled_start) WHERE status IN (1, 2);
-- The scheduler looks for open events across all servers.
CREATE INDEX guild_scheduled_events_open_idx
    ON guild_scheduled_events (scheduled_start) WHERE status IN (1, 2);

CREATE TABLE guild_scheduled_event_users (
    event_id    BIGINT NOT NULL REFERENCES guild_scheduled_events (id) ON DELETE CASCADE,
    user_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (event_id, user_id)
);
CREATE INDEX guild_scheduled_event_users_user_idx ON guild_scheduled_event_users (user_id);
