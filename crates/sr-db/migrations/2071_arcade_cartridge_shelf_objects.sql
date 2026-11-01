-- 2071: ARC-ROOM-001 (wave 42 integration): a Guild Cartridge names an object of wave 41's shelf catalog.
--
-- `source_ref` was an opaque locator until the shelf API existed. A cartridge may now name one catalog object
-- (`shelf_id`, `object_hash` of `shelf_objects`, migration 2020); members fetch it through the per-object URL
-- route, which checks their grant and every delisting at the moment it mints. A delisted object drops out of the
-- cartridge listing (P28-LEG-1), and the cartridge goes with the object or the shelf.
ALTER TABLE arcade_guild_cartridges
    ADD COLUMN shelf_id    BIGINT,
    ADD COLUMN object_hash TEXT CHECK (object_hash ~ '^[0-9a-f]{64}$'),
    ADD CONSTRAINT arcade_guild_cartridges_object_pair CHECK ((shelf_id IS NULL) = (object_hash IS NULL)),
    ADD CONSTRAINT arcade_guild_cartridges_object_fk FOREIGN KEY (shelf_id, object_hash)
        REFERENCES shelf_objects (shelf_id, object_hash) ON DELETE CASCADE;
