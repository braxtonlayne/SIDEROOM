-- 0201: per-account Library state. docs/04 section 11, LIB-DATA-001.
-- Owned by the account: removed with it (docs/09).

CREATE TABLE reading_progress (
    user_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    chapter_id  BIGINT NOT NULL REFERENCES reading_chapters (id) ON DELETE CASCADE,
    series_id   BIGINT NOT NULL REFERENCES reading_series (id) ON DELETE CASCADE,
    page_index  INTEGER NOT NULL DEFAULT 0 CHECK (page_index >= 0),
    percent     REAL NOT NULL DEFAULT 0 CHECK (percent >= 0 AND percent <= 100),
    -- EPUB position.
    cfi         TEXT,
    completed   BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, chapter_id)
);
CREATE INDEX reading_progress_recent_idx ON reading_progress (user_id, updated_at DESC);
CREATE INDEX reading_progress_series_idx ON reading_progress (series_id);

CREATE TABLE reading_bookmarks (
    id          BIGINT PRIMARY KEY,
    user_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    chapter_id  BIGINT NOT NULL REFERENCES reading_chapters (id) ON DELETE CASCADE,
    page_index  INTEGER NOT NULL DEFAULT 0 CHECK (page_index >= 0),
    note        TEXT NOT NULL DEFAULT '' CHECK (char_length(note) <= 1000),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, chapter_id, page_index)
);
CREATE INDEX reading_bookmarks_user_idx ON reading_bookmarks (user_id, created_at DESC);

CREATE TABLE reading_follows (
    user_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    series_id   BIGINT NOT NULL REFERENCES reading_series (id) ON DELETE CASCADE,
    -- The "My library" shelf.
    shelf       TEXT NOT NULL DEFAULT 'reading'
                CHECK (shelf IN ('reading', 'plan_to_read', 'completed', 'on_hold', 'dropped')),
    -- New-chapter notifications (the bell).
    notify      BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, series_id)
);
CREATE INDEX reading_follows_series_idx ON reading_follows (series_id);
