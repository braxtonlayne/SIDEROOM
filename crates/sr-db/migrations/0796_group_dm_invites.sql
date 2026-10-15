-- 0796: invite links to group DMs (G-47, P11-SHL-2). 24-hour expiry, unlimited uses,
-- revocable by the group owner; they are always in-platform (D-022 does not apply).
-- Group DM channels (type 3) live in `channels`; the invite dies with the channel.
CREATE TABLE group_dm_invites (
    code        TEXT PRIMARY KEY CHECK (code ~ '^[0-9A-Za-z-]{2,32}$'),
    channel_id  BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    inviter_id  BIGINT NOT NULL REFERENCES accounts (id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL,
    revoked_at  TIMESTAMPTZ
);
CREATE INDEX group_dm_invites_channel_idx ON group_dm_invites (channel_id, created_at DESC);
