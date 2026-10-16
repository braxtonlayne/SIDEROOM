-- 0876: USE_LIBRARY_EMBEDS (1 << 58) is enforced at send (a message carrying a <library:id> or
-- <cinema:id> card needs it, D-241) and is part of the default @everyone permissions of new
-- servers. Existing servers gain it once here so cards keep working; the @everyone role has
-- id = guild id. A server that does not want cards clears the bit on @everyone afterwards; this
-- never re-runs. Forward-only.
UPDATE roles
SET permissions = permissions | (1::BIGINT << 58)
WHERE id = guild_id;
