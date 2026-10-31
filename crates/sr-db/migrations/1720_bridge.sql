-- 1720: BRG-COR-001 (wave 35), the Discord bridge: model, sealed bot token, outbound queue.
--
-- The bridge is OFF everywhere by default (module `bridge.enabled`, D-381): a fresh server has no row in any
-- of these tables. The server owner switches the module on, pastes the token of their own Discord bot (a bot
-- token, never a self-bot: the route checks `bot: true`), and maps channels. Switching off is a teardown that
-- deletes the Discord webhooks the bridge created and every row below for the server; the token is wiped.
--
-- * `bridges`: one row per server while a token is held. `bot_token_enc` is sealed with `server.secret_key`
--   (D-185 layout, the row's guild id as associated data) and is never returned by any route or logged.
--   `intents_granted` is what Discord said the bot may receive (read by the gateway half, BRG-GW-001).
-- * `bridge_channel_maps`: a SIDE ROOM channel and a Discord channel, the direction messages flow, and the
--   Discord webhook the bridge created in that channel for posting (`webhook_token_enc` sealed, the map id as
--   associated data). A `from_discord` map has no webhook.
-- * `bridge_message_map`: our message id and theirs, so edits, deletes and replies find each other, and the
--   loop guard can tell a mirrored message from a native one. Deleted with the map.
-- * `bridge_optouts`: a member's "Don't mirror my messages" per channel. Their messages stay here only.
-- * `bridge_outbox`: the outbound queue. A row is one webhook call (create, edit, delete) waiting for its
--   turn under Discord's rate limits; after `max attempts` it becomes `dead` (the dead-letter counter on
--   Admin > System counts these) and stays until the map goes.
-- * `bridge_teardown_residue`: the Discord webhooks a teardown could not delete after three tries. Only ids
--   (no token): the owner removes them by hand from Discord's Integrations page, and the dialog names them.
--   Cleared when the owner connects a bot again.
CREATE TABLE bridges (
    guild_id            BIGINT PRIMARY KEY REFERENCES guilds (id) ON DELETE CASCADE,
    discord_guild_id    BIGINT NOT NULL,
    discord_guild_name  TEXT NOT NULL DEFAULT '' CHECK (char_length(discord_guild_name) <= 200),
    bot_user_id         BIGINT NOT NULL,
    bot_name            TEXT NOT NULL DEFAULT '' CHECK (char_length(bot_name) <= 200),
    bot_token_enc       BYTEA NOT NULL,
    status              TEXT NOT NULL DEFAULT 'connecting'
                        CHECK (status IN ('connecting', 'connected', 'disconnected', 'token_invalid')),
    intents_granted     BIGINT NOT NULL DEFAULT 0 CHECK (intents_granted >= 0),
    created_by          BIGINT NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE bridge_channel_maps (
    id                    BIGINT PRIMARY KEY,
    guild_id              BIGINT NOT NULL REFERENCES bridges (guild_id) ON DELETE CASCADE,
    channel_id            BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    discord_channel_id    BIGINT NOT NULL,
    discord_channel_name  TEXT NOT NULL DEFAULT '' CHECK (char_length(discord_channel_name) <= 200),
    discord_webhook_id    BIGINT,
    webhook_token_enc     BYTEA,
    direction             TEXT NOT NULL CHECK (direction IN ('both', 'to_discord', 'from_discord')),
    created_by            BIGINT NOT NULL,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- A SIDE ROOM channel is mapped to one Discord channel, and a Discord channel to one of ours per bridge.
    UNIQUE (channel_id),
    UNIQUE (guild_id, discord_channel_id),
    CHECK ((discord_webhook_id IS NULL) = (webhook_token_enc IS NULL))
);
CREATE INDEX bridge_channel_maps_guild_idx ON bridge_channel_maps (guild_id);

CREATE TABLE bridge_message_map (
    sr_message_id       BIGINT PRIMARY KEY,
    discord_message_id  BIGINT NOT NULL,
    map_id              BIGINT NOT NULL REFERENCES bridge_channel_maps (id) ON DELETE CASCADE,
    webhook_id          BIGINT,
    -- The SIDE ROOM author of an `out` message (the data-request export lists a member's bridged messages by id;
    -- erasure deletes the Discord copies). Not a foreign key: erasure reads the rows first.
    author_id           BIGINT,
    direction           TEXT NOT NULL CHECK (direction IN ('out', 'in')),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (map_id, discord_message_id)
);
CREATE INDEX bridge_message_map_map_idx ON bridge_message_map (map_id);
CREATE INDEX bridge_message_map_author_idx ON bridge_message_map (author_id) WHERE author_id IS NOT NULL;

CREATE TABLE bridge_optouts (
    user_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    channel_id  BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    guild_id    BIGINT NOT NULL REFERENCES bridges (guild_id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, channel_id)
);
CREATE INDEX bridge_optouts_channel_idx ON bridge_optouts (channel_id);

CREATE TABLE bridge_outbox (
    id              BIGINT PRIMARY KEY,
    guild_id        BIGINT NOT NULL REFERENCES bridges (guild_id) ON DELETE CASCADE,
    map_id          BIGINT NOT NULL REFERENCES bridge_channel_maps (id) ON DELETE CASCADE,
    kind            TEXT NOT NULL CHECK (kind IN ('create', 'edit', 'delete', 'hello')),
    sr_message_id   BIGINT,
    payload         JSONB NOT NULL DEFAULT '{}'::jsonb,
    state           TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'dead')),
    attempts        INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    due_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_error      TEXT CHECK (char_length(last_error) <= 500),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX bridge_outbox_due_idx ON bridge_outbox (due_at) WHERE state = 'pending';
CREATE INDEX bridge_outbox_map_idx ON bridge_outbox (map_id, id);
CREATE INDEX bridge_outbox_dead_idx ON bridge_outbox (guild_id) WHERE state = 'dead';
-- One create per message, so a repeated event never posts twice.
CREATE UNIQUE INDEX bridge_outbox_create_once ON bridge_outbox (sr_message_id) WHERE kind = 'create';

CREATE TABLE bridge_teardown_residue (
    guild_id            BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    discord_webhook_id  BIGINT NOT NULL,
    discord_channel_id  BIGINT NOT NULL,
    reason              TEXT NOT NULL DEFAULT '' CHECK (char_length(reason) <= 200),
    failed_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, discord_webhook_id)
);
