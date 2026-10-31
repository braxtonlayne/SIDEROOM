-- 1753: BRG-SYN-001, the deletion hold and the Discord snapshot.
--
-- `bridge_pending_deletes`: a channel, category or thread deleted on Discord is HELD here, not deleted (deleting
-- destroys SIDE ROOM history and `channels` has no soft delete). The row hides the object at once (channel
-- lists, gateway and reads treat it as gone: a hold only shrinks access). After `bridge.delete_hold_hours` the
-- hold job deletes it through the normal channel delete path; until then "Restore" recreates it on Discord from
-- SIDE ROOM's state and "Delete now" deletes it. The row keeps no content, only ids and times. Deleting the
-- channel removes the row with it.
--
-- `bridge_discord_snapshot`: what Discord's roles and channel overwrites looked like when whole_guild was switched
-- on (taken by the seed job before any write). Only the teardown option "restore Discord's permissions to before
-- the bridge" reads it. `data` holds ids, names and permission words, never message content or tokens. Deleted
-- with the bridge.
CREATE TABLE bridge_pending_deletes (
    channel_id          BIGINT PRIMARY KEY REFERENCES channels (id) ON DELETE CASCADE,
    guild_id            BIGINT NOT NULL REFERENCES bridges (guild_id) ON DELETE CASCADE,
    discord_channel_id  BIGINT NOT NULL,
    map_id              BIGINT,
    -- The Discord user who deleted it, when the audit log said so.
    discord_actor_id    BIGINT,
    held_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    delete_after        TIMESTAMPTZ NOT NULL
);
CREATE INDEX bridge_pending_deletes_due_idx ON bridge_pending_deletes (delete_after);
CREATE INDEX bridge_pending_deletes_guild_idx ON bridge_pending_deletes (guild_id);

CREATE TABLE bridge_discord_snapshot (
    guild_id    BIGINT NOT NULL REFERENCES bridges (guild_id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('role', 'channel')),
    discord_id  BIGINT NOT NULL,
    data        JSONB NOT NULL,
    taken_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, kind, discord_id)
);
