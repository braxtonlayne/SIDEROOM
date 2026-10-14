-- 0630: guild deletion keeps the guild row for the 30-day message window (LNG-SSET-073;
-- docs/09 section 2: "Deleting a server marks all its channels deleted. Server metadata
-- is kept for staff"). Deleting a guild now hides its messages as `user_deleted`
-- (D-121) with the normal `purge_after`, removes members, invites, webhooks and app
-- installs, and stamps `deleted_at`. The purge job hard-deletes the guild row (and with
-- it channels, roles, audit entries) once no message of the guild is left, so a message
-- under a moderation hold keeps the guild alive (D-122).
ALTER TABLE guilds ADD COLUMN deleted_at TIMESTAMPTZ;
CREATE INDEX guilds_deleted_idx ON guilds (deleted_at) WHERE deleted_at IS NOT NULL;
