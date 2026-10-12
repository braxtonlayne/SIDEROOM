-- 0034: webhooks. docs/04 section 4, LNG-SSET-044/045.
-- Only a hash of the execution token is stored.
CREATE TABLE webhooks (
    id                 BIGINT PRIMARY KEY,
    guild_id           BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    channel_id         BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    -- 1 incoming, 2 channel follower, 3 application (Discord webhook types).
    kind               SMALLINT NOT NULL CHECK (kind BETWEEN 1 AND 3),
    creator_id         BIGINT REFERENCES accounts (id),
    name               TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 80),
    avatar_key         TEXT,
    token_hash         TEXT,
    application_id     BIGINT,
    source_channel_id  BIGINT,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (kind <> 1 OR token_hash IS NOT NULL)
);
CREATE INDEX webhooks_guild_idx ON webhooks (guild_id);
CREATE INDEX webhooks_channel_idx ON webhooks (channel_id);
