-- 1510 (BYOS-TV-001): where a Live TV channel draws its pools from.
--
-- 'instance' is every channel that exists today (the instance catalog, no owner). 'guild' is a server's channel:
-- its pools are that server's shelves only (the server's own mounts and the instance bucket when it is attached),
-- so every viewer who may watch the server's Cinema sees the same guide. 'personal' is one member's channel from
-- their own shelf, for that member alone. A friend's shelf is never a source.
--
-- Channel numbers were unique across the instance; they are now unique within a source (an instance number, a
-- server's numbers, a member's numbers), so two servers can both have a channel 1. The old constraint's default
-- name is dropped if present; the partial indexes keep every number inside its source unique either way.
ALTER TABLE tv_channels
    ADD COLUMN pool_source   TEXT NOT NULL DEFAULT 'instance' CHECK (pool_source IN ('instance', 'guild', 'personal')),
    ADD COLUMN guild_id      BIGINT,
    ADD COLUMN owner_user_id BIGINT,
    ADD CONSTRAINT tv_channels_pool_scope CHECK (
        (pool_source = 'instance' AND guild_id IS NULL AND owner_user_id IS NULL)
        OR (pool_source = 'guild' AND guild_id IS NOT NULL AND owner_user_id IS NULL)
        OR (pool_source = 'personal' AND owner_user_id IS NOT NULL AND guild_id IS NULL)
    );

ALTER TABLE tv_channels DROP CONSTRAINT IF EXISTS tv_channels_number_key;

CREATE UNIQUE INDEX tv_channels_number_instance ON tv_channels (number) WHERE pool_source = 'instance';
CREATE UNIQUE INDEX tv_channels_number_guild ON tv_channels (guild_id, number) WHERE pool_source = 'guild';
CREATE UNIQUE INDEX tv_channels_number_personal ON tv_channels (owner_user_id, number) WHERE pool_source = 'personal';
CREATE INDEX tv_channels_guild_idx ON tv_channels (guild_id) WHERE guild_id IS NOT NULL;
