-- 0990: unlinking a Discord identity from User Settings (ENT-AUTH-011).
-- The row stays so the identity is never freed for another account (D-030) and registration still
-- finds the Person; sign-in and the link status ignore a row with `unlinked_at` set.
ALTER TABLE external_identities ADD COLUMN unlinked_at TIMESTAMPTZ;
