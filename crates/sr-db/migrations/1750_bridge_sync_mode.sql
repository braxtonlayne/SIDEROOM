-- 1750: BRG-SYN-001 (wave 35b), the whole-guild mirror: mode state on the bridge and the sync columns of a map.
--
-- `bridges.mode` already exists (1720): 'selective' (the default; wave 35 plus category and thread maps) or
-- 'whole_guild' (one server on both platforms). There is no "off" value: off is the absence of a `bridges` row.
-- This migration adds what the whole-guild mode needs around it:
--
-- * `sync_state`: where the structure/permission sync is. 'off' (selective: no role or permission sync),
--   'seeding' (the seed job is running), 'live', 'paused' (a manager paused it), 'paused_discord_access' (the bot
--   lost ADMINISTRATOR or its top position), 'paused_invalid_requests' (the invalid-request breaker tripped).
-- * the mode-change audit fields (who, when, which plan) and `last_reconcile_at`.
-- * a partial unique index: one whole_guild bridge per Discord server on this instance (two SIDE ROOM servers
--   must not both own the permissions of one Discord server).
--
-- `bridge_channel_maps` grows the fields of the reconciler (design section 2.1):
-- * `kind` ('channel', 'category', 'thread') exists since 1720; categories, threads, voice and stage maps carry no
--   webhook (the existing check already allows both webhook columns to be NULL).
-- * `selected`: true for a map the owner picked (selective top-level), false for one inherited (children of a
--   mapped category, threads of a mapped channel, every whole_guild map).
-- * `origin`: who made the pair. 'matched' (both sides existed), 'sr' (the bridge created the Discord object),
--   'discord' (the bridge created the SIDE ROOM object). Teardown "also remove what the bridge created" deletes
--   only `origin = 'sr'` objects on Discord.
-- * `parent_map_id`: the map of the parent category or channel (threads, children of categories).
-- * `synced_state`: the last state both sides agreed on, field by field, each side as that side expresses it.
-- * `sync_status`: 'mapped', 'degraded' (Discord refused the type; `degraded_reason`), 'frozen' (ping-pong
--   freeze), 'pending_delete' (a deletion made on Discord is held, 1753).
ALTER TABLE bridges
    ADD COLUMN sync_state          TEXT NOT NULL DEFAULT 'off'
        CHECK (sync_state IN ('off', 'seeding', 'live', 'paused', 'paused_discord_access', 'paused_invalid_requests')),
    ADD COLUMN mode_changed_by     BIGINT,
    ADD COLUMN mode_changed_at     TIMESTAMPTZ,
    ADD COLUMN mode_plan_id        BIGINT,
    ADD COLUMN mode_plan_hash      TEXT CHECK (char_length(mode_plan_hash) <= 128),
    ADD COLUMN last_reconcile_at   TIMESTAMPTZ;

CREATE UNIQUE INDEX bridges_whole_guild_discord_uidx
    ON bridges (discord_guild_id) WHERE mode = 'whole_guild';

ALTER TABLE bridge_channel_maps
    ADD COLUMN selected        BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN origin          TEXT NOT NULL DEFAULT 'matched'
        CHECK (origin IN ('matched', 'sr', 'discord')),
    ADD COLUMN parent_map_id   BIGINT REFERENCES bridge_channel_maps (id) ON DELETE SET NULL,
    ADD COLUMN synced_state    JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN sync_status     TEXT NOT NULL DEFAULT 'mapped'
        CHECK (sync_status IN ('mapped', 'degraded', 'frozen', 'pending_delete')),
    ADD COLUMN degraded_reason TEXT CHECK (char_length(degraded_reason) <= 200);

CREATE INDEX bridge_channel_maps_parent_idx ON bridge_channel_maps (parent_map_id) WHERE parent_map_id IS NOT NULL;
