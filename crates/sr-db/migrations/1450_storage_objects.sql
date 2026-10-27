-- 1450 (BYOS-CAT-001): the catalog of what the indexer found in a storage mount.
--
-- `storage_objects` is one row per object (mount, key) the `storage.index` job classified: which section it
-- belongs to, its container, and the catalog row built from it. A row is never deleted when the object
-- disappears from the bucket; it becomes `vanished` (and its catalog rows are hidden), so an edit the member made
-- (an override) survives an object that comes back. Keys are mount-relative, as everywhere (`safe_key`).
--
-- Catalog rows built from a mount live in the existing tables (`video_titles`, `video_episodes`, `reading_series`,
-- `reading_chapters`; `source = 'mount'` and `mount_id` come from migration 1420) plus the new `music_tracks`.
-- They are written with `published = FALSE`, so every existing member query (published and not merged) skips them
-- and nothing leaks by omission; the shelf queries read them by (`source = 'mount'`, `mount_id` the viewer may
-- reach, `shelf_state = 'listed'`). `overridden` names the fields a member edited: re-index never rewrites those.
CREATE TABLE storage_objects (
    id           BIGINT PRIMARY KEY,
    mount_id     BIGINT NOT NULL,
    key          TEXT NOT NULL CHECK (char_length(key) BETWEEN 1 AND 1024),
    size_bytes   BIGINT NOT NULL DEFAULT 0 CHECK (size_bytes >= 0),
    -- 'cinema', 'library', 'music', or NULL for a companion file (cover, subtitle, sidecar).
    kind         TEXT CHECK (kind IN ('cinema', 'library', 'music')),
    -- movie, episode, book, comic, manga, track, artwork, subtitle, sidecar, other.
    class        TEXT NOT NULL,
    container    TEXT NOT NULL DEFAULT '',
    -- Plays in a browser without conversion (MP4, WebM, MP3, FLAC, OGG).
    web_native   BOOLEAN NOT NULL DEFAULT FALSE,
    -- What the classifier read from the path (title, year, season, episode, artist, album ...).
    parsed       JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(parsed) = 'object'),
    state        TEXT NOT NULL DEFAULT 'indexed' CHECK (state IN ('indexed', 'vanished', 'ignored')),
    -- The run that last saw the object; a finished run hides what it did not see.
    seen_run     BIGINT NOT NULL DEFAULT 0,
    indexed_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    vanished_at  TIMESTAMPTZ,
    UNIQUE (mount_id, key)
);
CREATE INDEX storage_objects_mount_state_idx ON storage_objects (mount_id, state);

-- One row per mount: the last index run (the Re-index button shows it).
CREATE TABLE storage_index_runs (
    mount_id     BIGINT PRIMARY KEY,
    run          BIGINT NOT NULL DEFAULT 0,
    state        TEXT NOT NULL DEFAULT 'idle' CHECK (state IN ('idle', 'running', 'done', 'failed')),
    started_at   TIMESTAMPTZ,
    finished_at  TIMESTAMPTZ,
    objects      INTEGER NOT NULL DEFAULT 0,
    added        INTEGER NOT NULL DEFAULT 0,
    vanished     INTEGER NOT NULL DEFAULT 0,
    truncated    BOOLEAN NOT NULL DEFAULT FALSE,
    error        TEXT
);

-- Catalog rows from a mount: the object they were built from, the folder that groups a series (so a re-index
-- finds the same row even after the member renamed it), the fields the member edited, and the provider's
-- attribution when the details were fetched from one.
ALTER TABLE video_titles ADD COLUMN mount_object_id BIGINT, ADD COLUMN shelf_key TEXT,
    ADD COLUMN overridden TEXT[] NOT NULL DEFAULT '{}', ADD COLUMN attribution TEXT,
    ADD COLUMN shelf_state TEXT NOT NULL DEFAULT 'listed' CHECK (shelf_state IN ('listed', 'vanished'));
ALTER TABLE video_episodes ADD COLUMN mount_object_id BIGINT,
    ADD COLUMN overridden TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN shelf_state TEXT NOT NULL DEFAULT 'listed' CHECK (shelf_state IN ('listed', 'vanished'));
ALTER TABLE reading_series ADD COLUMN mount_object_id BIGINT, ADD COLUMN shelf_key TEXT,
    ADD COLUMN external_ids JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN overridden TEXT[] NOT NULL DEFAULT '{}', ADD COLUMN attribution TEXT,
    ADD COLUMN shelf_state TEXT NOT NULL DEFAULT 'listed' CHECK (shelf_state IN ('listed', 'vanished'));
ALTER TABLE reading_chapters ADD COLUMN mount_object_id BIGINT,
    ADD COLUMN overridden TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN shelf_state TEXT NOT NULL DEFAULT 'listed' CHECK (shelf_state IN ('listed', 'vanished'));
-- A CBZ on a mount is a chapter of format 'images' with no stored pages: `source_key` names the archive, which the
-- browser reads by range requests. (Unpublished, so no instance reader ever asks for its pages.)

CREATE UNIQUE INDEX video_titles_shelf_uq ON video_titles (mount_id, shelf_key)
    WHERE source = 'mount' AND shelf_key IS NOT NULL;
CREATE UNIQUE INDEX video_episodes_object_uq ON video_episodes (mount_object_id)
    WHERE mount_object_id IS NOT NULL;
CREATE UNIQUE INDEX reading_series_shelf_uq ON reading_series (mount_id, shelf_key)
    WHERE source = 'mount' AND shelf_key IS NOT NULL;
CREATE UNIQUE INDEX reading_chapters_object_uq ON reading_chapters (mount_object_id)
    WHERE mount_object_id IS NOT NULL;
CREATE INDEX video_titles_mount_idx ON video_titles (mount_id, id) WHERE source = 'mount';
CREATE INDEX reading_series_mount_idx ON reading_series (mount_id, id) WHERE source = 'mount';

-- Audio found in a mount's Music folder. The player and the music library (wave 30) read the same table.
CREATE TABLE music_tracks (
    id               BIGINT PRIMARY KEY,
    mount_id         BIGINT NOT NULL,
    mount_object_id  BIGINT NOT NULL UNIQUE,
    source           TEXT NOT NULL DEFAULT 'mount',
    title            TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 300),
    artist           TEXT NOT NULL DEFAULT '',
    album            TEXT NOT NULL DEFAULT '',
    album_artist     TEXT NOT NULL DEFAULT '',
    track_no         INTEGER CHECK (track_no BETWEEN 0 AND 9999),
    disc_no          INTEGER CHECK (disc_no BETWEEN 0 AND 999),
    year             SMALLINT CHECK (year BETWEEN 1000 AND 2200),
    duration_ms      INTEGER CHECK (duration_ms >= 0),
    genres           TEXT[] NOT NULL DEFAULT '{}',
    tags             TEXT[] NOT NULL DEFAULT '{}',
    cover_key        TEXT,
    external_ids     JSONB NOT NULL DEFAULT '{}'::jsonb,
    attribution      TEXT,
    synopsis         TEXT NOT NULL DEFAULT '',
    overridden       TEXT[] NOT NULL DEFAULT '{}',
    shelf_state      TEXT NOT NULL DEFAULT 'listed' CHECK (shelf_state IN ('listed', 'vanished')),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX music_tracks_mount_idx ON music_tracks (mount_id, album, track_no, id);
