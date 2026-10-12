-- 0033: AutoMod rules. docs/04 section 4, LNG-SSET-059.
-- `trigger` and `actions` hold the serde form of sr_chat::settings::automod types;
-- `trigger_kind` is denormalised for the per-kind quota check.
CREATE TABLE automod_rules (
    id                BIGINT PRIMARY KEY,
    guild_id          BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    name              TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    creator_id        BIGINT NOT NULL REFERENCES accounts (id),
    event_type        TEXT NOT NULL CHECK (event_type IN ('message_send', 'member_update')),
    trigger_kind      TEXT NOT NULL,
    trigger           JSONB NOT NULL,
    actions           JSONB NOT NULL,
    enabled           BOOLEAN NOT NULL DEFAULT TRUE,
    exempt_role_ids   BIGINT[] NOT NULL DEFAULT '{}',
    exempt_channel_ids BIGINT[] NOT NULL DEFAULT '{}',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX automod_rules_guild_idx ON automod_rules (guild_id, enabled);
