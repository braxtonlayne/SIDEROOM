-- 0831: Patron chapter downloads (LIB-RDR-016, D-075): one row per file handed out, for the
-- daily limit (`downloads_per_day` of the Patron perk) and the member's recent downloads.

CREATE TABLE reading_downloads (
    id         BIGINT PRIMARY KEY,
    user_id    BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    chapter_id BIGINT NOT NULL,
    format     TEXT NOT NULL CHECK (format IN ('cbz', 'pdf', 'epub')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX reading_downloads_user_idx ON reading_downloads (user_id, created_at DESC);
