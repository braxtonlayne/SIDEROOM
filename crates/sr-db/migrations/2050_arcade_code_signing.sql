-- 2050: ARC-SBX-001 (wave 42), swarm-safety code signing.
--
-- * `code_sha256` on a bundle: SHA-256 (hex) over the executable code of the bundle only (`.js`, `.mjs`,
--   `.wasm`), in sorted path order with the framing documented in `sr_activities::sandbox::code_sha256`. It is
--   what the operator signs and what a member's vouch is pinned to. A bundle uploaded before this migration has
--   NULL until `sr-cli arcade backfill-hashes` (or the first trust read) computes it from the stored files.
-- * `connect_hosts`: the extra origins the reviewed `sideroom.json` lets the sandbox's CSP `connect-src` name
--   (https origins, or the word "self"); empty means `connect-src 'none'`. CSP is per host, never per URL.
-- * `arcade_signing_keys`: the operator's Ed25519 signing keys. The seed is sealed with `server.secret_key`
--   (`sr-cli arcade sign-code` makes the first key on first use); only the verify key is ever read by the
--   server. A retired key's signatures stop counting.
-- * `arcade_code_signatures`: one signature per (application, code hash): the Ed25519 signature over the
--   domain-separated hash, the key that made it and the static audit report that preceded it. Revoking a row
--   removes the badge at once without touching the bundle.
ALTER TABLE application_activity_bundles
    ADD COLUMN code_sha256   TEXT CHECK (code_sha256 ~ '^[0-9a-f]{64}$'),
    ADD COLUMN connect_hosts TEXT[] NOT NULL DEFAULT '{}';
CREATE INDEX application_activity_bundles_code_idx
    ON application_activity_bundles (code_sha256) WHERE code_sha256 IS NOT NULL;

CREATE TABLE arcade_signing_keys (
    id          BIGINT PRIMARY KEY,
    verify_key  TEXT NOT NULL UNIQUE CHECK (verify_key ~ '^[0-9a-f]{64}$'),
    seed_sealed BYTEA NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    retired_at  TIMESTAMPTZ
);
-- At most one key signs at a time.
CREATE UNIQUE INDEX arcade_signing_keys_active_idx ON arcade_signing_keys ((true)) WHERE retired_at IS NULL;

CREATE TABLE arcade_code_signatures (
    id             BIGINT PRIMARY KEY,
    application_id BIGINT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    code_sha256    TEXT NOT NULL CHECK (code_sha256 ~ '^[0-9a-f]{64}$'),
    key_id         BIGINT NOT NULL REFERENCES arcade_signing_keys (id),
    signature      TEXT NOT NULL CHECK (signature ~ '^[0-9a-f]{128}$'),
    audit          JSONB NOT NULL,
    signed_by      TEXT NOT NULL CHECK (char_length(signed_by) BETWEEN 1 AND 80),
    signed_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at     TIMESTAMPTZ,
    revoked_reason TEXT CHECK (char_length(revoked_reason) <= 300),
    UNIQUE (application_id, code_sha256)
);
CREATE INDEX arcade_code_signatures_hash_idx ON arcade_code_signatures (code_sha256);
