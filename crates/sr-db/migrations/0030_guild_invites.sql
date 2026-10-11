-- 0030: server (guild) invites. docs/04 section 4, D-022, LNG-SSET-038.
-- Platform invites (account creation) are in 0002; these only join a guild.
CREATE TABLE guild_invites (
    code          TEXT PRIMARY KEY CHECK (code ~ '^[0-9A-Za-z-]{2,32}$'),
    guild_id      BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    channel_id    BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    inviter_id    BIGINT NOT NULL REFERENCES accounts (id),
    -- 0 = never expires / unlimited.
    max_age_secs  INTEGER NOT NULL DEFAULT 86400 CHECK (max_age_secs BETWEEN 0 AND 604800),
    max_uses      INTEGER NOT NULL DEFAULT 0 CHECK (max_uses BETWEEN 0 AND 100),
    uses          INTEGER NOT NULL DEFAULT 0 CHECK (uses >= 0),
    temporary     BOOLEAN NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at    TIMESTAMPTZ,
    CHECK (max_uses = 0 OR uses <= max_uses)
);
CREATE INDEX guild_invites_guild_idx ON guild_invites (guild_id, created_at DESC);
CREATE INDEX guild_invites_inviter_idx ON guild_invites (inviter_id);
