-- 0760: user-facing feature modules (docs/inventory/09-feature-modules.md): scheduled messages (FTR-SCH-001),
-- reminders (FTR-REM-001), bookmarks (FTR-BKM-001), birthdays (FTR-BDAY-001), leveling (FTR-LVL-001),
-- RSS/Atom poster (FTR-RSS-001), poll templates (FTR-POLLT-001), suggestions (FTR-SUGG-001), tickets
-- (FTR-TICK-001) and the cursors of the library feed and watch party announcer (FTR-LIBF-001, FTR-WPA-001).
-- Per-guild settings of every module (channels, roles, templates) live in `feature_enablements.config`; only
-- data that belongs to a person, a feed or a post gets a table.

-- A message composed now and sent later, as its author. `status` moves pending -> sent | failed | cancelled.
CREATE TABLE scheduled_messages (
    id          BIGINT PRIMARY KEY,
    author_id   BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    channel_id  BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    guild_id    BIGINT REFERENCES guilds (id) ON DELETE CASCADE,
    content     TEXT NOT NULL CHECK (char_length(content) BETWEEN 1 AND 4000),
    send_at     TIMESTAMPTZ NOT NULL,
    status      TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'sent', 'failed', 'cancelled')),
    failure     TEXT,
    message_id  BIGINT,
    sent_at     TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX scheduled_messages_due_idx ON scheduled_messages (send_at) WHERE status = 'pending';
CREATE INDEX scheduled_messages_author_idx ON scheduled_messages (author_id, send_at DESC);

-- "Remind me": optionally about a message. Delivered as a DM from the system user.
CREATE TABLE reminders (
    id           BIGINT PRIMARY KEY,
    user_id      BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    channel_id   BIGINT REFERENCES channels (id) ON DELETE SET NULL,
    guild_id     BIGINT REFERENCES guilds (id) ON DELETE SET NULL,
    message_id   BIGINT,
    note         TEXT NOT NULL DEFAULT '' CHECK (char_length(note) <= 1000),
    remind_at    TIMESTAMPTZ NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'sent', 'failed', 'cancelled')),
    delivered_at TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX reminders_due_idx ON reminders (remind_at) WHERE status = 'pending';
CREATE INDEX reminders_user_idx ON reminders (user_id, remind_at DESC);

-- Private saved messages with a note. The message may later be deleted; the row then shows as "unavailable".
CREATE TABLE message_bookmarks (
    user_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    message_id  BIGINT NOT NULL,
    channel_id  BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    guild_id    BIGINT REFERENCES guilds (id) ON DELETE CASCADE,
    note        TEXT NOT NULL DEFAULT '' CHECK (char_length(note) <= 500),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, message_id)
);
CREATE INDEX message_bookmarks_user_idx ON message_bookmarks (user_id, created_at DESC);

-- A person's birthday: month and day, the year only when they choose to share it. `visibility` 'servers' shows the
-- day to servers that run the module; 'hidden' keeps it private (nothing is announced).
CREATE TABLE user_birthdays (
    user_id     BIGINT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    month       SMALLINT NOT NULL CHECK (month BETWEEN 1 AND 12),
    day         SMALLINT NOT NULL CHECK (day BETWEEN 1 AND 31),
    year        SMALLINT CHECK (year BETWEEN 1900 AND 2100),
    visibility  TEXT NOT NULL DEFAULT 'servers' CHECK (visibility IN ('servers', 'hidden')),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX user_birthdays_day_idx ON user_birthdays (month, day) WHERE visibility = 'servers';

-- One row per (server, person, UTC date) announced; `role_until` marks the birthday role to remove again.
CREATE TABLE birthday_announcements (
    guild_id    BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    user_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    on_date     DATE NOT NULL,
    role_id     BIGINT,
    role_until  TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, user_id, on_date)
);
CREATE INDEX birthday_announcements_role_idx ON birthday_announcements (role_until) WHERE role_id IS NOT NULL;

-- Leveling (opt-in per server, and per member). Rows exist only for members who earned XP.
CREATE TABLE member_xp (
    guild_id      BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    user_id       BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    xp            BIGINT NOT NULL DEFAULT 0 CHECK (xp >= 0),
    level         INTEGER NOT NULL DEFAULT 0 CHECK (level >= 0),
    messages      BIGINT NOT NULL DEFAULT 0,
    last_award_at TIMESTAMPTZ,
    PRIMARY KEY (guild_id, user_id)
);
CREATE INDEX member_xp_board_idx ON member_xp (guild_id, xp DESC, user_id);

-- A member who opted out of XP in a server (their row is deleted, nothing more is earned).
CREATE TABLE member_xp_optout (
    guild_id    BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    user_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, user_id)
);

-- RSS / Atom feeds a server follows into a channel.
CREATE TABLE rss_feeds (
    id                 BIGINT PRIMARY KEY,
    guild_id           BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    channel_id         BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    url                TEXT NOT NULL CHECK (char_length(url) BETWEEN 8 AND 2048),
    title              TEXT NOT NULL DEFAULT '',
    include_keywords   TEXT[] NOT NULL DEFAULT '{}',
    exclude_keywords   TEXT[] NOT NULL DEFAULT '{}',
    poll_interval_secs INTEGER NOT NULL DEFAULT 1800 CHECK (poll_interval_secs BETWEEN 300 AND 86400),
    enabled            BOOLEAN NOT NULL DEFAULT TRUE,
    next_check_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_checked_at    TIMESTAMPTZ,
    last_status        TEXT NOT NULL DEFAULT 'pending',
    failures           INTEGER NOT NULL DEFAULT 0,
    seeded             BOOLEAN NOT NULL DEFAULT FALSE,
    created_by         BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (guild_id, channel_id, url)
);
CREATE INDEX rss_feeds_due_idx ON rss_feeds (next_check_at) WHERE enabled;

-- Entries already posted for a feed (hash of the entry id or link), so each shows up once.
CREATE TABLE rss_seen (
    feed_id    BIGINT NOT NULL REFERENCES rss_feeds (id) ON DELETE CASCADE,
    entry_hash TEXT NOT NULL,
    seen_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (feed_id, entry_hash)
);

-- Saved poll setups of a server.
CREATE TABLE poll_templates (
    id                BIGINT PRIMARY KEY,
    guild_id          BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    name              TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 60),
    question          TEXT NOT NULL CHECK (char_length(question) BETWEEN 1 AND 300),
    answers           JSONB NOT NULL,
    allow_multiselect BOOLEAN NOT NULL DEFAULT FALSE,
    duration_hours    INTEGER NOT NULL DEFAULT 24 CHECK (duration_hours BETWEEN 1 AND 768),
    created_by        BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (guild_id, name)
);

-- Suggestions: each is a message in the server's suggestions channel (up/down votes are reactions on it).
CREATE TABLE suggestions (
    id            BIGINT PRIMARY KEY,
    guild_id      BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    channel_id    BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    message_id    BIGINT NOT NULL,
    author_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    content       TEXT NOT NULL CHECK (char_length(content) BETWEEN 1 AND 2000),
    status        TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'considering', 'approved', 'denied', 'implemented')),
    response      TEXT NOT NULL DEFAULT '' CHECK (char_length(response) <= 1000),
    responder_id  BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX suggestions_guild_idx ON suggestions (guild_id, id DESC);

-- Tickets: a private thread per request.
CREATE TABLE tickets (
    id          BIGINT PRIMARY KEY,
    guild_id    BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    opener_id   BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    thread_id   BIGINT NOT NULL,
    subject     TEXT NOT NULL CHECK (char_length(subject) BETWEEN 1 AND 100),
    status      TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed')),
    closed_by   BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    closed_at   TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX tickets_guild_idx ON tickets (guild_id, id DESC);
CREATE INDEX tickets_opener_idx ON tickets (guild_id, opener_id) WHERE status = 'open';

-- Where a polled announcer (library feed, watch party announcer) stopped reading, per server.
CREATE TABLE feature_cursors (
    feature     TEXT NOT NULL,
    guild_id    BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    cursor_ts   TIMESTAMPTZ NOT NULL,
    cursor_id   BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (feature, guild_id)
);
