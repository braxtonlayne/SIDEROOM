-- 0631: server tags and their adoption (LNG-SSET-011, LNG-SSET-012).
-- A guild defines one short tag with a badge and colour; members choose at most one
-- guild whose tag they show next to their name everywhere (Discord's "primary guild").
-- `members_can_display` is the "Members can choose to display" toggle.
CREATE TABLE guild_tags (
    guild_id             BIGINT PRIMARY KEY REFERENCES guilds (id) ON DELETE CASCADE,
    tag                  TEXT NOT NULL CHECK (char_length(tag) BETWEEN 2 AND 4),
    badge                SMALLINT NOT NULL DEFAULT 0 CHECK (badge BETWEEN 0 AND 15),
    color                INTEGER NOT NULL DEFAULT 0 CHECK (color BETWEEN 0 AND 16777215),
    members_can_display  BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE user_primary_guilds (
    user_id   BIGINT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    guild_id  BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    set_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX user_primary_guilds_guild_idx ON user_primary_guilds (guild_id);
