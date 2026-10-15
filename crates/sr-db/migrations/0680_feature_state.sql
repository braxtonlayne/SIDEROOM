-- 0680: runtime state of built-in feature modules (FTR-*; docs/inventory/09-feature-modules.md).
-- Settings an admin chose live in feature_enablements; this table holds what a module
-- learned while running: the live sticky post of a channel, the starboard copy of a
-- message, a temporary role's expiry. One keyed JSON row per (feature, guild, key).
-- `due_at` is when the module wants to look at the row again; the module tick reads
-- the rows that are due. Rows go with the guild.
CREATE TABLE feature_state (
    feature    TEXT NOT NULL CHECK (char_length(feature) BETWEEN 1 AND 100),
    guild_id   BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    key        TEXT NOT NULL CHECK (char_length(key) BETWEEN 1 AND 200),
    value      JSONB NOT NULL DEFAULT '{}'::jsonb,
    due_at     TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (feature, guild_id, key)
);
CREATE INDEX feature_state_due_idx ON feature_state (feature, due_at) WHERE due_at IS NOT NULL;
