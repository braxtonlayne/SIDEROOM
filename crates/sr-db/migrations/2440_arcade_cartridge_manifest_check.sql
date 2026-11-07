-- 2440: cartridges listed before D-568 are brought under it (wave 55, SEC-AUD-045; D-579).
--
-- Since wave 54 a guild cartridge's tier and capabilities are read from its file's `sideroom.json` when it is
-- listed. Rows listed before that kept the tier their lister typed. A one-off boot job (`arcade.manifest_recheck`)
-- reads the manifest of every row without `manifest_checked_at` and rewrites tier and capabilities from it; a row
-- whose file cannot be read, has no manifest (or no tier) or is not a zip is delisted (`delisted_at`, with the
-- reason) and staff are alerted with its name, so the lister can list it again. A delisted row is left out of the
-- listing and of the per-server cap; listing the same file again replaces it.
ALTER TABLE arcade_guild_cartridges
    ADD COLUMN manifest_checked_at TIMESTAMPTZ,
    ADD COLUMN delisted_at         TIMESTAMPTZ,
    ADD COLUMN delist_reason       TEXT CHECK (char_length(delist_reason) <= 400);

CREATE INDEX arcade_guild_cartridges_unchecked_idx ON arcade_guild_cartridges (id)
    WHERE manifest_checked_at IS NULL;
