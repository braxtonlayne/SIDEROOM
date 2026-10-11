-- 0035: server templates. docs/04 section 4, LNG-SSET-070.
-- One template per guild; `snapshot` is the serde form of
-- sr_chat::settings::templates::TemplateSnapshot.
CREATE TABLE guild_templates (
    code         TEXT PRIMARY KEY CHECK (code ~ '^[0-9A-Za-z]{8,16}$'),
    guild_id     BIGINT NOT NULL UNIQUE REFERENCES guilds (id) ON DELETE CASCADE,
    creator_id   BIGINT NOT NULL REFERENCES accounts (id),
    name         TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    description  TEXT CHECK (char_length(description) <= 120),
    usage_count  INTEGER NOT NULL DEFAULT 0 CHECK (usage_count >= 0),
    snapshot     JSONB NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
