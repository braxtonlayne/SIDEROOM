-- 0871: remove the legacy download fallback (H-03). `cinema_download_grants` (0422) was the placeholder
-- source of the Patron download perk; migration 0542 copied unexpired rows into `entitlement_grants` and
-- the server has read only entitlements since Phase 12 (`SR_DOWNLOADS_LEGACY_FALLBACK` is gone).
--
-- Guard: refuse to drop the table when a grant row is newer than the newest entitlement row, which means
-- the entitlements were never backfilled from it (the upgrade skipped a Phase 12 release).
DO $$
DECLARE
    newest_grant BIGINT;
    newest_entitlement BIGINT;
BEGIN
    IF to_regclass('cinema_download_grants') IS NULL THEN
        RETURN;
    END IF;
    SELECT (extract(epoch FROM max(created_at)) * 1000)::bigint INTO newest_grant
      FROM cinema_download_grants;
    SELECT max(starts_ms) INTO newest_entitlement FROM entitlement_grants;
    IF newest_grant IS NOT NULL AND (newest_entitlement IS NULL OR newest_grant > newest_entitlement) THEN
        RAISE EXCEPTION 'run a Phase 12 release first so entitlements are backfilled';
    END IF;
END
$$;

DROP TABLE IF EXISTS cinema_download_grants;
