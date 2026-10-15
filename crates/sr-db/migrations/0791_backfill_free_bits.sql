-- 0791: USE_EXTERNAL_SOUNDS (1 << 45) and START_WATCH_PARTY (1 << 56) are free for
-- everybody (D-202), so they are part of the default @everyone permissions of new servers.
-- Existing servers gain them once here; the @everyone role has id = guild id.
-- Forward-only: never re-run.
UPDATE roles
SET permissions = permissions | (1::BIGINT << 45) | (1::BIGINT << 56)
WHERE id = guild_id;
