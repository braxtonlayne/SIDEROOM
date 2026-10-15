-- 0771: seal the interactions signing seed (G-04, D-185) and track endpoint health (G-05,
-- D-186).
--
-- `interactions_signing_seed_sealed` holds the seed encrypted with `server.secret_key`
-- (AES-256-GCM, the helper TOTP secrets use). The plaintext column of 0710 stays until the
-- startup backfill `apps.seal_seeds` has re-encrypted every row once and set it to NULL; a
-- later migration drops it. No code reads the plaintext column after the backfill.
ALTER TABLE applications
    ADD COLUMN interactions_signing_seed_sealed BYTEA,
    ALTER COLUMN interactions_signing_seed DROP NOT NULL;

-- Endpoint health: consecutive failed deliveries (25 within 10 minutes disable the endpoint).
ALTER TABLE applications
    ADD COLUMN endpoint_state TEXT NOT NULL DEFAULT 'enabled'
        CHECK (endpoint_state IN ('enabled', 'disabled')),
    ADD COLUMN endpoint_failures INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN endpoint_failure_window_start TIMESTAMPTZ;
