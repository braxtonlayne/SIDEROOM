-- 1400 (BYOS-MNT-001): storage mounts. A mount is one S3-compatible bucket (plus key prefix) that a user, a guild
-- or the platform owns. Mount 1 is the instance bucket (owner_kind = 'platform'), seeded from `storage.*` by
-- sr-server on first start (BYOS-INS-001); every other id is an application-generated Snowflake.
--
-- `secret_key_enc` is the secret access key sealed with `server.secret_key` (sr_db::sealed, layout
-- version || nonce || AES-256-GCM); the associated data is `mount:<id>` so a blob cannot be moved to another
-- row. `key_hint` is the last four characters of the access key id, safe to show. Nothing in an API body ever
-- carries the secret. `cors_ok_at` is set when the CORS check last passed (BYOS-MNT-003) and gates presigning.
-- Deleting a mount is soft: `deleted_at` is set and the row stays for audit and for objects that still name it.
CREATE TABLE storage_mounts (
    id              BIGINT PRIMARY KEY,
    owner_kind      TEXT NOT NULL CHECK (owner_kind IN ('user', 'guild', 'platform')),
    owner_id        BIGINT NOT NULL DEFAULT 0,
    label           TEXT NOT NULL CHECK (char_length(label) BETWEEN 1 AND 64),
    provider        TEXT NOT NULL CHECK (provider IN ('s3', 'r2', 'b2', 'minio', 'wasabi', 'garage')),
    endpoint        TEXT NOT NULL CHECK (char_length(endpoint) BETWEEN 1 AND 512),
    region          TEXT NOT NULL DEFAULT 'auto' CHECK (char_length(region) BETWEEN 1 AND 64),
    bucket          TEXT NOT NULL CHECK (char_length(bucket) BETWEEN 1 AND 255),
    prefix          TEXT NOT NULL DEFAULT '' CHECK (char_length(prefix) <= 512),
    path_style      BOOLEAN NOT NULL DEFAULT false,
    access_key_id   TEXT NOT NULL CHECK (char_length(access_key_id) BETWEEN 1 AND 256),
    secret_key_enc  BYTEA NOT NULL,
    key_hint        TEXT NOT NULL DEFAULT '' CHECK (char_length(key_hint) <= 8),
    status          TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'failing', 'disabled')),
    cors_ok_at      TIMESTAMPTZ,
    last_test_at    TIMESTAMPTZ,
    last_test       JSONB,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at      TIMESTAMPTZ,
    CHECK ((owner_kind = 'platform') = (owner_id = 0))
);

-- The list and the cap count read one owner's live mounts.
CREATE INDEX storage_mounts_owner_idx ON storage_mounts (owner_kind, owner_id) WHERE deleted_at IS NULL;
-- A label is unique per owner among live mounts.
CREATE UNIQUE INDEX storage_mounts_owner_label_uq ON storage_mounts (owner_kind, owner_id, lower(label)) WHERE deleted_at IS NULL;
