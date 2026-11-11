-- 2500: a shelf that cannot be asked defers a cartridge's manifest re-check instead of delisting it (wave 56, D-582).
--
-- The one-off re-check of cartridges listed before D-568 (2440) used to fold every read failure into "delisted".
-- It now tells a definite answer (no mount, path not on the shelf, object not found, not a zip, no manifest, no
-- tier) from a transient one (a backend error, a client that could not be built, a failed endpoint check): the
-- row stays unchecked, remembers since when and why it is deferred, and the job runs again hourly while any row is
-- deferred. After 24 hours staff get one alert per shelf (`manifest_defer_alerted_at` marks the rows told about).
ALTER TABLE arcade_guild_cartridges
    ADD COLUMN manifest_defer_shelf      BIGINT,
    ADD COLUMN manifest_deferred_since   TIMESTAMPTZ,
    ADD COLUMN manifest_defer_run        BIGINT,
    ADD COLUMN manifest_defer_reason     TEXT CHECK (char_length(manifest_defer_reason) <= 400),
    ADD COLUMN manifest_defer_alerted_at TIMESTAMPTZ;

CREATE INDEX arcade_guild_cartridges_deferred_idx ON arcade_guild_cartridges (manifest_defer_shelf)
    WHERE manifest_checked_at IS NULL AND manifest_deferred_since IS NOT NULL;
