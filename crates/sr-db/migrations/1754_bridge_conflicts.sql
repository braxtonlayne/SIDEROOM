-- 1754: BRG-SYN-002 (wave 35b), the whole-guild Discord bridge: the conflict log.
--
-- SIDE ROOM is the master copy of permissions. When Discord's state differs from what the bridge pushed in a
-- SIDE ROOM-authoritative field (a role's shared permission bits, an overwrite, a role's position), the bridge puts
-- SIDE ROOM's value back and writes one row here. The row is also the proposal: "Apply on SIDE ROOM" opens the
-- editor prefilled with what Discord attempted, and an ordinary SIDE ROOM edit by the person who clicks is the only
-- way it ever becomes a SIDE ROOM permission. Nothing in this table is read by any authorization code.
--
-- One row per (kind, object, field) while open; a repeat inside ten minutes bumps `count` and `last_at` instead of
-- adding a row (coalescing happens before the write, in `sr_bridge::sync::conflicts`). The row carries the Discord
-- actor's id and name when the audit log named them (personal data of a Discord user, kept only as long as the
-- bridge: the rows go with the `bridges` row on teardown, and closed rows are pruned after 90 days).
--
-- Kinds: permission_reverted (role bits, @everyone bits, overwrite or role position put back),
-- overwrite_trimmed (allow bits stripped from a Discord member or managed-role overwrite), structure_conflict
-- (both sides edited a two-way field; SIDE ROOM won), role_deletion_refused (a Discord role deletion that would have
-- removed a deny), deletion_held (a channel, category or thread deleted on Discord, hidden here for the hold),
-- frozen (ping-pong freeze), dead_op (an op that failed 8 times), cap (Discord's 250 role or 500 channel limit).
CREATE TABLE bridge_conflicts (
    id                  BIGINT PRIMARY KEY,
    guild_id            BIGINT NOT NULL REFERENCES bridges (guild_id) ON DELETE CASCADE,
    kind                TEXT NOT NULL CHECK (kind IN (
                            'permission_reverted', 'overwrite_trimmed', 'structure_conflict',
                            'role_deletion_refused', 'deletion_held', 'frozen', 'dead_op', 'cap')),
    object_kind         TEXT NOT NULL CHECK (object_kind IN
                            ('role', 'channel', 'category', 'thread', 'overwrite', 'op', 'guild')),
    -- The SIDE ROOM object (role, channel, ...) and its Discord counterpart; either may be unknown.
    sr_object_id        BIGINT,
    discord_object_id   BIGINT,
    -- An overwrite's target (the SIDE ROOM role id, or the Discord member / role id that was trimmed).
    target_id           BIGINT,
    -- The field in question: 'permissions', 'position', 'allow', 'deny', 'name', 'topic', ...
    field               TEXT NOT NULL DEFAULT '' CHECK (char_length(field) <= 64),
    -- What Discord has (or attempted) and what SIDE ROOM has, as JSON the log shows (permission words are
    -- decimal strings, shared bits only: SIDE ROOM-only bits never appear in either).
    discord_value       JSONB,
    sr_value            JSONB,
    -- The Discord user who did it, when the Discord audit log said so (matched by target id within 10 s).
    discord_actor_id    BIGINT,
    discord_actor_name  TEXT CHECK (char_length(discord_actor_name) <= 100),
    op_id               BIGINT,
    count               INTEGER NOT NULL DEFAULT 1 CHECK (count >= 1),
    status              TEXT NOT NULL DEFAULT 'open'
                        CHECK (status IN ('open', 'resolved', 'dismissed', 'superseded')),
    resolution          TEXT CHECK (resolution IN
                            ('applied_on_sideroom', 'took_discord', 'restored', 'deleted_now',
                             'unfrozen', 'retried', 'dismissed', 'superseded')),
    resolved_by         BIGINT,
    resolved_at         TIMESTAMPTZ,
    first_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((status = 'open') = (resolution IS NULL)),
    CHECK (status = 'open' OR resolved_at IS NOT NULL)
);
CREATE INDEX bridge_conflicts_guild_idx ON bridge_conflicts (guild_id, id DESC);
CREATE INDEX bridge_conflicts_open_idx ON bridge_conflicts (guild_id, kind, id DESC) WHERE status = 'open';
-- At most one open row per (kind, object, field): the coalescing key.
CREATE UNIQUE INDEX bridge_conflicts_open_key ON bridge_conflicts (
    guild_id, kind, object_kind, COALESCE(sr_object_id, 0), COALESCE(discord_object_id, 0),
    COALESCE(target_id, 0), field) WHERE status = 'open';
