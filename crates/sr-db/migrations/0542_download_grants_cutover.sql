-- 0542: cutover of the Cinema download perk (CIN-DL-001 stand-in -> DON-ENT-006).
--
-- `cinema_download_grants` (0422) was the placeholder source of the Patron download perk. The
-- perk now comes from `entitlement_grants`. Unexpired legacy rows are copied once as Patron
-- grants with the same end (NULL expiry = permanent). The old table is NOT dropped: the server
-- keeps reading it as a fallback until `SR_DOWNLOADS_LEGACY_FALLBACK=0`, and a later migration
-- removes it (docs/dev/handoff/feat-p8-donations.md, "Cutover").
--
-- `per_day` of a legacy row is not carried over: the allowance is now the Patron perk
-- `downloads_per_day` (Admin > Donations).

INSERT INTO entitlement_grants
    (id, kind, user_id, source, source_id, days, ends_ms, permanent, starts_ms, reason)
SELECT -g.user_id,  -- negative ids can never clash with snowflakes
       'patron',
       g.user_id,
       'migrated',
       NULL,
       NULL,
       CASE WHEN g.expires_at IS NULL THEN NULL
            ELSE (extract(epoch FROM g.expires_at) * 1000)::bigint END,
       g.expires_at IS NULL,
       (extract(epoch FROM g.created_at) * 1000)::bigint,
       COALESCE(g.note, 'migrated from cinema_download_grants')
FROM cinema_download_grants g
WHERE g.expires_at IS NULL OR g.expires_at > now();
