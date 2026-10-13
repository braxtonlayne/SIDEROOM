-- 0160: last-seen tracking for member prune (D-166, F-02).
-- Kept by the gateway (IDENTIFY/RESUME, then at most every 10 minutes per session)
-- and by login. NULL = never seen. Never serialised in user objects.
ALTER TABLE accounts ADD COLUMN last_seen_at TIMESTAMPTZ;
