-- 0505: interactions (DEV-BOT-004). Rows live for the 15 minutes of token validity plus a day
-- (swept by the interactions GC); they hold digests of the interaction token only.
CREATE TABLE interactions (
    id                   BIGINT PRIMARY KEY,
    application_id       BIGINT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    token_hash           BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    kind                 SMALLINT NOT NULL CHECK (kind BETWEEN 1 AND 5),
    user_id              BIGINT NOT NULL,
    guild_id             BIGINT,
    channel_id           BIGINT NOT NULL,
    command_id           BIGINT,
    source_message_id    BIGINT,
    nonce                TEXT,
    -- created -> responded | deferred (one transition, compare-and-set).
    state                TEXT NOT NULL DEFAULT 'created' CHECK (state IN ('created', 'responded', 'deferred')),
    response_type        SMALLINT,
    ephemeral            BOOLEAN NOT NULL DEFAULT FALSE,
    original_message_id  BIGINT,
    created_at           TIMESTAMPTZ NOT NULL,
    responded_at         TIMESTAMPTZ,
    expires_at           TIMESTAMPTZ NOT NULL
);
CREATE INDEX interactions_expiry_idx ON interactions (expires_at);
CREATE INDEX interactions_app_idx ON interactions (application_id, id DESC);

-- Messages that belong to an interaction: the only ones its token may edit or delete.
-- `payload` keeps the message object of ephemeral messages (never stored in `messages`).
CREATE TABLE interaction_messages (
    message_id      BIGINT PRIMARY KEY,
    interaction_id  BIGINT NOT NULL REFERENCES interactions (id) ON DELETE CASCADE,
    ephemeral       BOOLEAN NOT NULL DEFAULT FALSE,
    is_original     BOOLEAN NOT NULL DEFAULT FALSE,
    payload         JSONB,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX interaction_messages_interaction_idx ON interaction_messages (interaction_id);
