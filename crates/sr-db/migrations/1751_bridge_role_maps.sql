-- 1751: BRG-SYN-001, the role map of the whole-guild mirror.
--
-- A SIDE ROOM role and the Discord role it mirrors. Only whole_guild bridges have rows here (selective mode
-- syncs no roles, and the switch back to selective deletes them). `@everyone` is mapped by identity
-- (`is_everyone`: role id = guild id here, the Discord guild id there). Managed roles (bots, boosters,
-- integrations) are never mapped.
--
-- `discord_only_bits` holds the permission bits Discord sent that SIDE ROOM does not define (47, 53 and up): kept
-- raw so a push preserves them; they never enter a SIDE ROOM permission word. SIDE ROOM-only bits (56 to 62) are
-- never stored here and never sent.
-- `synced_state` is the last agreed state (name, colour, hoist, mentionable, emoji, icon, the shared permission
-- bits as last pushed, the position as last pushed).
CREATE TABLE bridge_role_maps (
    id                  BIGINT PRIMARY KEY,
    guild_id            BIGINT NOT NULL REFERENCES bridges (guild_id) ON DELETE CASCADE,
    role_id             BIGINT NOT NULL REFERENCES roles (id) ON DELETE CASCADE,
    discord_role_id     BIGINT NOT NULL,
    is_everyone         BOOLEAN NOT NULL DEFAULT FALSE,
    origin              TEXT NOT NULL CHECK (origin IN ('matched', 'sr', 'discord')),
    discord_only_bits   BIGINT NOT NULL DEFAULT 0 CHECK (discord_only_bits >= 0),
    synced_state        JSONB NOT NULL DEFAULT '{}'::jsonb,
    sync_status         TEXT NOT NULL DEFAULT 'mapped' CHECK (sync_status IN ('mapped', 'frozen')),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (role_id),
    UNIQUE (guild_id, discord_role_id)
);
CREATE INDEX bridge_role_maps_guild_idx ON bridge_role_maps (guild_id);
