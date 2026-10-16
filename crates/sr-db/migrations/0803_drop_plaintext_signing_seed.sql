-- G-85 (P12-DPB-5): drop the plaintext interactions signing seed (migration 0710). Since 0771 the
-- seed lives encrypted in `interactions_signing_seed_sealed`, and the previous release's startup job
-- `apps.seal_seeds` (D-185) moved every old row over and cleared this column.
--
-- Upgrade order: a deployment that still has plaintext seeds must first run the previous release
-- once so that job runs. This migration refuses to continue while any row still has one, rather than
-- destroy a key that cannot be recovered.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM applications WHERE interactions_signing_seed IS NOT NULL) THEN
        RAISE EXCEPTION 'run the previous release once to seal seeds (applications.interactions_signing_seed still holds plaintext)';
    END IF;
END
$$;

ALTER TABLE applications DROP COLUMN interactions_signing_seed;
