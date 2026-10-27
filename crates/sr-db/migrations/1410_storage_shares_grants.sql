-- 1410 (BYOS-SCP-001): who besides the owner may read a storage mount.
--
-- `storage_mount_shares` is the Friend Shelf: the owner of a user mount shares it with one friend, per kind
-- (cinema, library, music). Mutual friends only: the route checks it on write, `sr_storage::scopes::resolve`
-- checks it on every read, and the friend-removal hook deletes the rows when the friendship ends.
--
-- `guild_storage_grants` holds the five storage grants a role carries (O-1 default: a table, not permission
-- bits; the permission word has no free bits to spare). A guild shelf belongs to no channel, so there are no
-- overwrites. Owner and ADMINISTRATOR bypass, MANAGE_GUILD implies `manage` (evaluated in code, not stored).
CREATE TABLE storage_mount_shares (
    mount_id   BIGINT NOT NULL REFERENCES storage_mounts (id) ON DELETE CASCADE,
    friend_id  BIGINT NOT NULL REFERENCES accounts (id),
    cinema     BOOLEAN NOT NULL DEFAULT false,
    library    BOOLEAN NOT NULL DEFAULT false,
    music      BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (mount_id, friend_id)
);
-- "What friends share with me" and the unfriend hook read by friend.
CREATE INDEX storage_mount_shares_friend_idx ON storage_mount_shares (friend_id);

CREATE TABLE guild_storage_grants (
    guild_id   BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    role_id    BIGINT NOT NULL REFERENCES roles (id) ON DELETE CASCADE,
    grant_name TEXT NOT NULL CHECK (grant_name IN ('view_cinema', 'view_library', 'view_music', 'upload', 'manage')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, role_id, grant_name)
);
CREATE INDEX guild_storage_grants_role_idx ON guild_storage_grants (role_id);
