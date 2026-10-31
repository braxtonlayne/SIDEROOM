-- 1800 (ACT-EMU-001, wave 37): the sixth storage grant `view_games` and the catalog kind `games` for ROMs.
--
-- `view_games` joins the five grants of 1410 (roadmap-p27 section 3, same table): a guild role that holds it may list
-- and read the files under `emu/roms/` of a guild shelf. The owner of a mount reads their own ROMs without a grant;
-- a Friend Shelf has no games column (friends never reach `emu/`), so `storage_mount_shares` is unchanged.
--
-- The indexer files each ROM under `emu/roms/` as `storage_objects` with `kind = 'games'`, `class = 'rom'` and
-- `parsed = {title, platform, core}`. No other table is needed: the emulator lists ROMs from this catalog or from the
-- bucket browser, and saves and BIOS files under `emu/saves/` and `emu/bios/` are never cataloged.
ALTER TABLE guild_storage_grants DROP CONSTRAINT guild_storage_grants_grant_name_check;
ALTER TABLE guild_storage_grants ADD CONSTRAINT guild_storage_grants_grant_name_check
    CHECK (grant_name IN ('view_cinema', 'view_library', 'view_music', 'upload', 'manage', 'view_games'));

ALTER TABLE storage_objects DROP CONSTRAINT storage_objects_kind_check;
ALTER TABLE storage_objects ADD CONSTRAINT storage_objects_kind_check
    CHECK (kind IN ('cinema', 'library', 'music', 'games'));
