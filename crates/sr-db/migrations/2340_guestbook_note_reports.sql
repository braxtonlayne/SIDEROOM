-- PRF-PRO-001 (D-506, D-507): a guestbook note can be reported (`target_kind = 'guestbook_note'`). The report is
-- routed to platform staff with a text snapshot as evidence; there is no hold, so `moderation_holds` is unchanged.
-- Each CHECK below is rebuilt from its own current definition with `guestbook_note` appended (as 1710 did for
-- `status_ring`), so this migration adds its value whatever other waves added to the same lists before it.
DO $$
DECLARE
    c RECORD;
    t TEXT;
BEGIN
    FOREACH t IN ARRAY ARRAY['moderation_cases', 'moderation_reports'] LOOP
        FOR c IN
            SELECT conname, pg_get_constraintdef(oid) AS def FROM pg_constraint
            WHERE conrelid = t::regclass AND contype = 'c'
              AND pg_get_constraintdef(oid) LIKE '%target_kind%ARRAY%'
        LOOP
            IF c.def NOT LIKE '%''guestbook_note''%' THEN
                EXECUTE format('ALTER TABLE %I DROP CONSTRAINT %I', t, c.conname);
                EXECUTE format('ALTER TABLE %I ADD CONSTRAINT %I %s', t, c.conname,
                               regexp_replace(c.def, '::text\]', '::text, ''guestbook_note''::text]'));
            END IF;
        END LOOP;
    END LOOP;
END $$;
