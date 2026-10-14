-- 0632: Server Insights (LNG-SSET-068): per-guild daily counters computed by a daily job.
-- One row per guild and UTC day; the job fills yesterday (and any of the last days that
-- are still missing) and keeps 90 days. `members` is the member count at the end of the
-- day (members who joined by then and are still members when the job runs).
-- `voice_minutes` stays 0 until voice sessions are persisted.
CREATE TABLE guild_insights_daily (
    guild_id       BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    day            DATE NOT NULL,
    members        INTEGER NOT NULL DEFAULT 0,
    joins          INTEGER NOT NULL DEFAULT 0,
    messages       INTEGER NOT NULL DEFAULT 0,
    voice_minutes  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (guild_id, day)
);

CREATE TABLE guild_insights_channel_daily (
    guild_id    BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    day         DATE NOT NULL,
    channel_id  BIGINT NOT NULL,
    messages    INTEGER NOT NULL,
    PRIMARY KEY (guild_id, day, channel_id)
);
