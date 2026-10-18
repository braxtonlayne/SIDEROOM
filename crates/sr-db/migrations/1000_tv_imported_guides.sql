-- 1000: Live TV imported guides, weekly slots and favorites (TV-ENG-001, TV-ENG-003, TV-ADM-003,
-- TV-ADM-004, docs/specs/cinema-live-tv.md section 4 and 6).
--
-- * An `imported_guide` channel airs a weekly grid: one row per slot in tv_schedule_slots (weekday and start
--   time in the channel time zone, 0 = Monday). A slot keeps the show text it was imported with and the
--   catalog title it matched (null while it matches nothing: such slots are skipped when the schedule is
--   resolved and picked up again when the library gains the title).
-- * tv_guide_imports holds an uploaded CSV or JSON guide between the upload and the staff confirming the
--   mapping report. Rows and report are plain JSON.
-- * tv_favorites is a member's starred channels.

ALTER TABLE tv_channels DROP CONSTRAINT tv_channels_kind_check;
ALTER TABLE tv_channels ADD CONSTRAINT tv_channels_kind_check
    CHECK (kind IN ('rule_based', 'manual', 'imported_guide'));

CREATE TABLE tv_schedule_slots (
    id             BIGINT PRIMARY KEY,
    channel_id     BIGINT NOT NULL REFERENCES tv_channels (id) ON DELETE CASCADE,
    weekday        SMALLINT NOT NULL CHECK (weekday BETWEEN 0 AND 6),
    start_minute   SMALLINT NOT NULL CHECK (start_minute BETWEEN 0 AND 1439),
    duration_min   SMALLINT NOT NULL CHECK (duration_min BETWEEN 1 AND 1440),
    show_name      TEXT NOT NULL CHECK (char_length(show_name) BETWEEN 1 AND 200),
    title_id       BIGINT,
    episode_id     BIGINT,
    season_number  INTEGER,
    episode_number INTEGER,
    note           TEXT CHECK (note IS NULL OR char_length(note) <= 200),
    import_id      BIGINT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (channel_id, weekday, start_minute)
);
CREATE INDEX tv_schedule_slots_title_idx ON tv_schedule_slots (title_id) WHERE title_id IS NOT NULL;

CREATE TABLE tv_guide_imports (
    id           BIGINT PRIMARY KEY,
    channel_id   BIGINT NOT NULL REFERENCES tv_channels (id) ON DELETE CASCADE,
    created_by   BIGINT,
    format       TEXT NOT NULL CHECK (format IN ('csv', 'json')),
    state        TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'confirmed')),
    rows         JSONB NOT NULL,
    report       JSONB NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    confirmed_at TIMESTAMPTZ
);
CREATE INDEX tv_guide_imports_channel_idx ON tv_guide_imports (channel_id, created_at DESC);

CREATE TABLE tv_favorites (
    account_id BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    channel_id BIGINT NOT NULL REFERENCES tv_channels (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (account_id, channel_id)
);
