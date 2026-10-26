-- 1360: USE_EXPRESSIONS_EXTERNALLY (1 << 60, D-375, D-376) is default-granted, so it is part of
-- the default @everyone permissions of new servers. Existing servers gain it once here; the
-- @everyone role has id = guild id. Forward-only: never re-run.
UPDATE roles
SET permissions = permissions | (1::BIGINT << 60)
WHERE id = guild_id;
