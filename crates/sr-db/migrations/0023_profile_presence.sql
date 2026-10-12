-- 0023: profile connections placeholder and persisted presence preferences.
-- Profile basics (bio, pronouns, banner_key, accent_color) already live on accounts (0001).
-- LNG-PRF-001/013.
CREATE TABLE user_connections (
    user_id      BIGINT NOT NULL REFERENCES accounts (id),
    provider     TEXT NOT NULL CHECK (provider ~ '^[a-z0-9-]{1,32}$'),
    external_id  TEXT NOT NULL CHECK (char_length(external_id) BETWEEN 1 AND 128),
    display_name TEXT NOT NULL CHECK (char_length(display_name) BETWEEN 1 AND 100),
    -- True only for OAuth-proven links; others are plain "link" entries.
    verified     BOOLEAN NOT NULL DEFAULT FALSE,
    visible      BOOLEAN NOT NULL DEFAULT TRUE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, provider, external_id)
);

-- What the user picked. Live presence (sessions, activities) is memory only (docs/04 section 4).
CREATE TABLE user_presence_prefs (
    user_id            BIGINT PRIMARY KEY REFERENCES accounts (id),
    -- 0 online, 1 idle, 2 dnd, 3 invisible.
    status             SMALLINT NOT NULL DEFAULT 0 CHECK (status IN (0, 1, 2, 3)),
    status_expires_at  TIMESTAMPTZ,
    custom_text        TEXT CHECK (char_length(custom_text) <= 128),
    custom_emoji_name  TEXT,
    custom_emoji_id    BIGINT,
    custom_expires_at  TIMESTAMPTZ,
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
