-- 0423: Retention bookkeeping for cinema media (CIN-ING-009). Storage prefixes that are no
-- longer referenced (a replaced ladder, an expired download, a deleted original) are queued here
-- by the sweep and removed after a grace period, so a request that still holds a signed URL to
-- the old files has time to finish.
CREATE TABLE cinema_media_gc (
    prefix    TEXT PRIMARY KEY,
    queued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    asset_id  BIGINT
);
