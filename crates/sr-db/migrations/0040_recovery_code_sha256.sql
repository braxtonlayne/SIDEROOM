-- 0040: recovery codes are SHA-256 digests (open questions 1 and 39).
-- 0005 made `code_phc` argon2 strings, which cannot be looked up by value and
-- cost ten argon2 checks per attempt. A code now carries 80 random bits, so a
-- 32-byte digest is enough, and UNIQUE (account_id, code_hash) lets the
-- database enforce one row per code. No code has ever been issued under the
-- PHC scheme, so existing rows (if any) are dropped rather than converted.
DELETE FROM recovery_codes;

ALTER TABLE recovery_codes DROP COLUMN code_phc;
ALTER TABLE recovery_codes
    ADD COLUMN code_hash BYTEA NOT NULL CHECK (octet_length(code_hash) = 32);
ALTER TABLE recovery_codes
    ADD CONSTRAINT recovery_codes_account_code_hash_key UNIQUE (account_id, code_hash);
