-- 0210: storage garbage collection queue for the Library (LIB-ING-008 follow-up).
-- Rows name objects that were referenced by a page, cover or source file that has since been
-- replaced or deleted. They are queued by the repository in the same transaction as the change;
-- the `library.gc` job deletes the object once nothing references it any more.

CREATE TABLE reading_gc_keys (
    key         TEXT PRIMARY KEY,
    -- Chapter whose page list held the object (cheap re-check against a re-ingest that reused
    -- the key); NULL for covers and source files.
    chapter_id  BIGINT,
    queued_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX reading_gc_keys_queued_idx ON reading_gc_keys (queued_at);

-- The collector checks whether a queued object is still referenced by a cover or a source file.
CREATE INDEX reading_series_cover_idx ON reading_series (cover_key) WHERE cover_key IS NOT NULL;
CREATE INDEX reading_volumes_cover_idx ON reading_volumes (cover_key) WHERE cover_key IS NOT NULL;
CREATE INDEX reading_chapters_source_idx ON reading_chapters (source_key) WHERE source_key IS NOT NULL;
