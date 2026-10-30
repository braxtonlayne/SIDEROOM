-- UTL-COL-001: tool rooms. A short-lived shared document (a tier list or a poll builder) bound to one
-- channel or DM. `state` is a flat JSON object of keys; an edit is a patch against `version`.
-- A room lives 7 days from creation (`expires_at`); the sweep job and every read drop expired rows.
CREATE TABLE tool_rooms (
    id         BIGINT PRIMARY KEY,
    channel_id BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    guild_id   BIGINT,
    kind       TEXT NOT NULL CHECK (kind IN ('tierlist', 'poll')),
    title      TEXT NOT NULL DEFAULT '' CHECK (char_length(title) <= 100),
    state      JSONB NOT NULL DEFAULT '{}'::jsonb,
    version    BIGINT NOT NULL DEFAULT 1 CHECK (version >= 1),
    created_by BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX tool_rooms_channel_idx ON tool_rooms (channel_id, id DESC);
CREATE INDEX tool_rooms_expiry_idx ON tool_rooms (expires_at);
