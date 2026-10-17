-- K-17 (D-260): pictures a person uploads for their customised profile page, a background image and gallery
-- pictures. The files live in object storage (`object_key`, served by the public `/cdn/avatars` route under the
-- account); a profile document refers to a row by its id. At most 1 background and 12 gallery rows per account is
-- checked in `sr-packages`, which also removes the rows (and files) a saved document no longer references.
CREATE TABLE profile_media (
    id           BIGINT PRIMARY KEY,
    account_id   BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    kind         TEXT NOT NULL CHECK (kind IN ('background', 'gallery')),
    object_key   TEXT NOT NULL,
    content_type TEXT NOT NULL,
    width        INTEGER NOT NULL CHECK (width > 0),
    height       INTEGER NOT NULL CHECK (height > 0),
    bytes        BIGINT NOT NULL CHECK (bytes >= 0),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX profile_media_account_idx ON profile_media (account_id, kind, id);
