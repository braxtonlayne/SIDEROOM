-- 1420 (BYOS-INS-001): which storage mount holds a media row's objects. Every existing row lives in the
-- instance bucket, which is mount 1 (seeded from `storage.*` by sr-server on first start), so the default
-- of 1 and the source 'instance' describe all current data without a rewrite. `source` says where the row's
-- object came from: 'instance' (uploaded to or imported into the instance bucket), later 'mount' (a
-- user or guild shelf, BYOS write side). Keys stay mount-relative and unchanged.
-- Not touched on purpose: attachments, emoji, avatars, stickers and profile media stay on the server's own path.
-- No foreign key: mount 1 is created by the application at start, after migrations ran, and soft-deleted
-- mounts must keep the rows that still name them.
ALTER TABLE video_assets   ADD COLUMN mount_id BIGINT NOT NULL DEFAULT 1, ADD COLUMN source TEXT NOT NULL DEFAULT 'instance';
ALTER TABLE video_titles   ADD COLUMN mount_id BIGINT NOT NULL DEFAULT 1, ADD COLUMN source TEXT NOT NULL DEFAULT 'instance';
ALTER TABLE video_episodes ADD COLUMN mount_id BIGINT NOT NULL DEFAULT 1, ADD COLUMN source TEXT NOT NULL DEFAULT 'instance';
ALTER TABLE reading_series   ADD COLUMN mount_id BIGINT NOT NULL DEFAULT 1, ADD COLUMN source TEXT NOT NULL DEFAULT 'instance';
ALTER TABLE reading_chapters ADD COLUMN mount_id BIGINT NOT NULL DEFAULT 1, ADD COLUMN source TEXT NOT NULL DEFAULT 'instance';
