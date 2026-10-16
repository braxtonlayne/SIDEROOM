-- 0879: the export cache index (H-18). Chapter exports are built into `library/exports/{chapter}/{fingerprint}.cbz`
-- in the object store; this table records when each one was last served so `library.gc` can delete the ones
-- nobody has downloaded for 7 days. No foreign key: a deleted chapter's exports simply age out.
CREATE TABLE library_export_cache (
    export_key   TEXT PRIMARY KEY,
    chapter_id   BIGINT NOT NULL,
    last_read_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX library_export_cache_idle_idx ON library_export_cache (last_read_at);

-- H-17 (D-229): reading presence has the Cinema's three audiences. `audience` is 1 when the reader shares
-- with everyone (friends and people who share a server) and 0 for friends only, set by the heartbeat from
-- `library.share_reading` and never 1 for an age-restricted series.
ALTER TABLE reading_presence ADD COLUMN audience SMALLINT NOT NULL DEFAULT 0 CHECK (audience IN (0, 1));
