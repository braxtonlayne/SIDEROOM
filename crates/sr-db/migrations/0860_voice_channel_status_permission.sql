-- 0860: SET_VOICE_CHANNEL_STATUS (1 << 48), Discord's "Set Voice Channel Status", is part of the
-- default @everyone permissions of new servers (like Use Soundboard). Existing servers gain it once
-- here; the @everyone role has id = guild id. A server that does not want it clears the bit on
-- @everyone afterwards; this never re-runs. Forward-only.
UPDATE roles
SET permissions = permissions | (1::BIGINT << 48)
WHERE id = guild_id;
