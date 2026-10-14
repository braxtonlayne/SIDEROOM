-- 0640: guild soundboard sounds (VOX-SB-001). Metadata only; the audio bytes are in object
-- storage under media_key. docs/specs/lounge-voice-video.md section 7.
CREATE TABLE soundboard_sounds (
    id           BIGINT PRIMARY KEY,
    guild_id     BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    name         TEXT NOT NULL CHECK (char_length(name) BETWEEN 2 AND 32),
    -- 0..1, the level the uploader wants the sound played at.
    volume       DOUBLE PRECISION NOT NULL DEFAULT 1 CHECK (volume >= 0 AND volume <= 1),
    -- A custom emoji of the guild, or a unicode emoji.
    emoji_id     BIGINT,
    emoji_name   TEXT CHECK (char_length(emoji_name) <= 32),
    available    BOOLEAN NOT NULL DEFAULT TRUE,
    creator_id   BIGINT NOT NULL REFERENCES accounts (id),
    media_key    TEXT NOT NULL,
    -- 1 mp3, 2 ogg, 3 wav.
    format_type  SMALLINT NOT NULL CHECK (format_type BETWEEN 1 AND 3),
    duration_ms  INTEGER NOT NULL CHECK (duration_ms BETWEEN 1 AND 5000),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX soundboard_sounds_guild_idx ON soundboard_sounds (guild_id, created_at);
