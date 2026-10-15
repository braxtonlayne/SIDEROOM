-- 0710: HTTP interactions endpoint and per-application Ed25519 signing key (DEV-BOT-005).
-- `interactions_signing_seed` is the private seed; the public key (`verify_key`) is derived
-- from it on read. It is created on demand and never returned by any API.
ALTER TABLE applications
    ADD COLUMN interactions_endpoint_url  TEXT CHECK (char_length(interactions_endpoint_url) <= 512),
    ADD COLUMN interactions_signing_seed  BYTEA CHECK (octet_length(interactions_signing_seed) = 32);
