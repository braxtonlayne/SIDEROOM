-- 0032: custom emoji and stickers (metadata; bytes are in object storage).
-- docs/04 section 4, LNG-SSET-021..024. Soundboard sounds come with the voice work.
CREATE TABLE emojis (
    id          BIGINT PRIMARY KEY,
    guild_id    BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    name        TEXT NOT NULL CHECK (name ~ '^[A-Za-z0-9_]{2,32}$'),
    animated    BOOLEAN NOT NULL DEFAULT FALSE,
    available   BOOLEAN NOT NULL DEFAULT TRUE,
    creator_id  BIGINT NOT NULL REFERENCES accounts (id),
    media_key   TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX emojis_guild_idx ON emojis (guild_id, created_at);

-- Role restriction: an emoji without rows here is usable by everyone.
CREATE TABLE emoji_roles (
    emoji_id  BIGINT NOT NULL REFERENCES emojis (id) ON DELETE CASCADE,
    role_id   BIGINT NOT NULL REFERENCES roles (id) ON DELETE CASCADE,
    PRIMARY KEY (emoji_id, role_id)
);
CREATE INDEX emoji_roles_role_idx ON emoji_roles (role_id);

CREATE TABLE stickers (
    id           BIGINT PRIMARY KEY,
    guild_id     BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    name         TEXT NOT NULL CHECK (char_length(name) BETWEEN 2 AND 30),
    description  TEXT CHECK (char_length(description) BETWEEN 2 AND 100),
    tags         TEXT NOT NULL CHECK (char_length(tags) BETWEEN 1 AND 200),
    -- 1 png, 2 apng, 3 lottie, 4 gif (Discord format_type).
    format_type  SMALLINT NOT NULL CHECK (format_type BETWEEN 1 AND 4),
    available    BOOLEAN NOT NULL DEFAULT TRUE,
    creator_id   BIGINT NOT NULL REFERENCES accounts (id),
    media_key    TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX stickers_guild_idx ON stickers (guild_id, created_at);
