-- 0400: Cinema watch state per account (docs/specs/cinema-media.md section 8, CIN-PLY-007/009).
-- Owned by the account: removed with it (docs/09). `title_id` / `episode_id` name catalog rows but carry
-- no foreign keys (the catalog is a separate crate); `episode_id = 0` is a movie (the title itself).

CREATE TABLE cinema_progress (
    user_id        BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    title_id       BIGINT NOT NULL CHECK (title_id > 0),
    episode_id     BIGINT NOT NULL DEFAULT 0 CHECK (episode_id >= 0),
    position_secs  INTEGER NOT NULL DEFAULT 0 CHECK (position_secs >= 0),
    duration_secs  INTEGER NOT NULL CHECK (duration_secs > 0),
    percent        REAL NOT NULL DEFAULT 0 CHECK (percent >= 0 AND percent <= 100),
    completed      BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, title_id, episode_id)
);
CREATE INDEX cinema_progress_recent_idx ON cinema_progress (user_id, updated_at DESC);
CREATE INDEX cinema_progress_title_idx ON cinema_progress (title_id, updated_at DESC);

-- Per-account switches: history pause (nothing recorded, no presence).
CREATE TABLE cinema_user_state (
    user_id         BIGINT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    history_paused  BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- "Watching" presence: one short-lived row per account, refreshed by the player heartbeat. Rows past
-- `expires_at` are ignored by every read. `audience` is the account's `cinema.share_watching` at the
-- time of the last heartbeat (0 friends, 1 everyone), so readers need not re-read the settings.
CREATE TABLE cinema_watching (
    user_id     BIGINT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    title_id    BIGINT NOT NULL CHECK (title_id > 0),
    episode_id  BIGINT NOT NULL DEFAULT 0 CHECK (episode_id >= 0),
    audience    SMALLINT NOT NULL DEFAULT 0 CHECK (audience IN (0, 1)),
    -- Age-restricted titles are announced to friends only and masked (section 8.4).
    restricted  BOOLEAN NOT NULL DEFAULT FALSE,
    started_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL
);
CREATE INDEX cinema_watching_expiry_idx ON cinema_watching (expires_at);

-- The account setting `cinema.share_watching` (user settings blob): 0 off, 1 friends (default, also for
-- absent or unknown values), 2 everyone (friends and people who share a server, D-120).
CREATE FUNCTION cinema_share_level(uid BIGINT) RETURNS SMALLINT
LANGUAGE sql STABLE AS $$
    SELECT CASE (SELECT settings #>> '{cinema,share_watching}' FROM user_settings WHERE user_id = uid)
        WHEN 'off' THEN 0
        WHEN 'everyone' THEN 2
        ELSE 1
    END::smallint
$$;
