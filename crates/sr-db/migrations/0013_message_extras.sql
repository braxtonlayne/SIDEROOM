-- 0013: attachments, reactions, pins, poll votes, read states. docs/04 section 3, LNG-MSG-001.
-- References to `messages(id)` target the partitioned table's primary key.
CREATE TYPE attachment_state AS ENUM ('pending', 'ready', 'expired', 'purged');

CREATE TABLE attachments (
    id             BIGINT PRIMARY KEY,
    -- NULL while an upload is pending and not yet attached to a message.
    message_id     BIGINT REFERENCES messages (id) ON DELETE CASCADE,
    uploader_id    BIGINT NOT NULL REFERENCES accounts (id),
    filename       TEXT NOT NULL CHECK (char_length(filename) BETWEEN 1 AND 255),
    content_type   TEXT,
    size_bytes     BIGINT NOT NULL CHECK (size_bytes >= 0),
    width          INTEGER,
    height         INTEGER,
    duration_secs  REAL,
    waveform       TEXT,
    description    TEXT,
    storage_key    TEXT NOT NULL,
    is_large       BOOLEAN NOT NULL DEFAULT FALSE,
    -- Large attachments only: uploaded_at + 7 days (D-043).
    expires_at     TIMESTAMPTZ,
    state          attachment_state NOT NULL DEFAULT 'pending',
    flags          INTEGER NOT NULL DEFAULT 0,
    blurhash       TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (is_large OR expires_at IS NULL)
);
CREATE INDEX attachments_message_idx ON attachments (message_id);
CREATE INDEX attachments_expiry_idx ON attachments (expires_at) WHERE is_large AND state = 'ready';

CREATE TABLE reactions (
    message_id  BIGINT NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    emoji_key   TEXT NOT NULL,
    user_id     BIGINT NOT NULL REFERENCES accounts (id),
    burst       BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (message_id, emoji_key, user_id)
);
CREATE INDEX reactions_user_idx ON reactions (user_id);

-- Pins as their own table (ordered list, 250 cap enforced by the service).
CREATE TABLE pins (
    channel_id  BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    message_id  BIGINT NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    pinned_by   BIGINT REFERENCES accounts (id),
    pinned_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (channel_id, message_id)
);
CREATE INDEX pins_channel_time_idx ON pins (channel_id, pinned_at DESC);

CREATE TABLE poll_votes (
    message_id  BIGINT NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    answer_id   INTEGER NOT NULL CHECK (answer_id > 0),
    user_id     BIGINT NOT NULL REFERENCES accounts (id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (message_id, answer_id, user_id)
);
CREATE INDEX poll_votes_user_idx ON poll_votes (user_id);

CREATE TABLE read_states (
    user_id               BIGINT NOT NULL REFERENCES accounts (id),
    channel_id            BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    last_read_message_id  BIGINT,
    mention_count         INTEGER NOT NULL DEFAULT 0 CHECK (mention_count >= 0),
    last_pin_timestamp    TIMESTAMPTZ,
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, channel_id)
);
CREATE INDEX read_states_channel_idx ON read_states (channel_id);
