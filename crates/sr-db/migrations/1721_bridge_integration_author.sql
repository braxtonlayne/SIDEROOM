-- 1721: BRG-COR-001 / BRG-GW-001 (wave 35 integration), the bridge's own integration author.
--
-- Messages that come in from Discord are stored under an author of their own, never under the account of the
-- owner who connected the bot (commander default, wave 35): erasing the owner's data must not delete the
-- bridged messages, and nothing a Discord user writes is attributed to a person here. The author is a bot
-- account (`is_bot`, a password no one can match, no application, no token), made when the bot is first
-- connected and kept for the server across teardowns, so the messages already shown keep their author when the
-- bridge is switched off and on again. Display (the Discord name and avatar, the DISCORD tag) comes from
-- `message_bridge_authors`, as before.
CREATE TABLE bridge_integration_authors (
    guild_id    BIGINT PRIMARY KEY REFERENCES guilds (id) ON DELETE CASCADE,
    user_id     BIGINT NOT NULL UNIQUE REFERENCES accounts (id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
