-- 0891: Perceptual hashes of series covers (REV-ED-006). A 64-bit difference hash per series,
-- computed lazily by the duplicate check, so a new submission's cover can be compared with every
-- published series without reading their images. `cover_key` records which cover the hash is of;
-- a replaced cover makes the row stale and it is hashed again.

CREATE TABLE library_cover_hashes (
    series_id  BIGINT PRIMARY KEY REFERENCES reading_series (id) ON DELETE CASCADE,
    cover_key  TEXT NOT NULL,
    phash      BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
