-- 0966: AutoMod alert messages with moderation buttons (P14-LNG-3, D-244).
-- One row per alert message the system user posted into a rule's alert channel. The id is the
-- id of the alert message; the buttons on it carry only that id, and who was flagged and which
-- message is read here.
CREATE TABLE automod_alerts (
    id                  BIGINT PRIMARY KEY,
    guild_id            BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    rule_id             BIGINT NOT NULL,
    rule_name           TEXT NOT NULL,
    channel_id          BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    target_user_id      BIGINT NOT NULL,
    flagged_channel_id  BIGINT NOT NULL,
    flagged_message_id  BIGINT,
    resolved_by         BIGINT,
    resolved_at         TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX automod_alerts_guild_idx ON automod_alerts (guild_id);
