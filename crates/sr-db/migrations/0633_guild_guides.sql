-- 0633: Server Guide content (LNG-SSET-066): welcome message, new-member to-dos and
-- resource pages. Whether the guide is shown is the community setting `guide_enabled`.
CREATE TABLE guild_guides (
    guild_id         BIGINT PRIMARY KEY REFERENCES guilds (id) ON DELETE CASCADE,
    welcome_message  TEXT,
    todos            JSONB NOT NULL DEFAULT '[]'::jsonb,
    resources        JSONB NOT NULL DEFAULT '[]'::jsonb,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
