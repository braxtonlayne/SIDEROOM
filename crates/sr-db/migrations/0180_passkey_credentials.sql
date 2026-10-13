-- 0180: passkeys (ENT-AUTH-010, F-14, D-161). The unused `webauthn_credentials` table from
-- 0001 becomes `passkey_credentials`, shaped for the pure-Rust verifier in sr-auth.
ALTER TABLE webauthn_credentials RENAME TO passkey_credentials;
ALTER INDEX webauthn_account_idx RENAME TO passkey_credentials_account_idx;
ALTER TABLE passkey_credentials RENAME COLUMN public_key TO cose_key;
-- COSE algorithm id: -7 ES256, -8 EdDSA, -257 RS256.
ALTER TABLE passkey_credentials ADD COLUMN alg SMALLINT NOT NULL DEFAULT -7;
ALTER TABLE passkey_credentials ALTER COLUMN alg DROP DEFAULT;
ALTER TABLE passkey_credentials ADD COLUMN backup_eligible BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE passkey_credentials ADD COLUMN backup_state BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE passkey_credentials ADD COLUMN aaguid BYTEA;
ALTER TABLE passkey_credentials DROP COLUMN transports;
ALTER TABLE passkey_credentials DROP COLUMN discoverable;
ALTER TABLE passkey_credentials
    ADD CONSTRAINT passkey_credentials_name_len CHECK (char_length(name) BETWEEN 1 AND 64);
ALTER TABLE passkey_credentials ALTER COLUMN name DROP DEFAULT;

-- WebAuthn `user.id`: 32 random bytes, never the account id. Created on first registration.
ALTER TABLE accounts ADD COLUMN user_handle BYTEA CHECK (octet_length(user_handle) = 32);
CREATE UNIQUE INDEX accounts_user_handle_idx ON accounts (user_handle) WHERE user_handle IS NOT NULL;
