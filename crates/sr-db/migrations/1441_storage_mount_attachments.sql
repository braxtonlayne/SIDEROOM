-- 1441 (BYOS-BRW-001, wave-27 follow-up): which servers have the platform bucket (mount 1) as a Guild Shelf.
--
-- An operator attaches the instance bucket to a server in Admin (`POST /admin/storage/mounts/{id}/attach`); the
-- members then reach it through the server's storage grants like any server shelf (`sr_storage::scopes`,
-- `ScopeFacts::attached_guilds`). Detaching removes the row. Only mounts owned by the platform are attached
-- this way: a member's or a server's own bucket belongs to its owner.
CREATE TABLE storage_mount_attachments (
    mount_id   BIGINT NOT NULL REFERENCES storage_mounts (id) ON DELETE CASCADE,
    guild_id   BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    attached_by BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (mount_id, guild_id)
);
CREATE INDEX storage_mount_attachments_guild_idx ON storage_mount_attachments (guild_id);
