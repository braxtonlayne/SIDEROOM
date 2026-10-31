-- 2010 (STU-SHF-001, wave 41-B): the owner's own words on a file of their shelf, in the table 41-C names.
--
-- Shaped as the `shelf_objects` of roadmap-p28 41-C (shelf_id, object_hash, path, size, kind, owner_title, owner_cover,
-- owner_note, delisted_at, delist_strike_id) so the integrator does not migrate twice. Today a shelf is a storage mount
-- (`shelf_id` = mount id) and `object_hash` is the SHA-256 hex of `path`; 41-C may later fill `size`, `kind` and the
-- content hash from the owner's uploaded index. `IF NOT EXISTS`: if 41-C's migration lands first this one is a no-op
-- and the columns used here are a subset. A row exists here only when the owner typed a title, cover or note.
-- Access to the listing and to the bytes stays decided by `sr_storage::scopes`; nothing is fetched from outside.
CREATE TABLE IF NOT EXISTS shelf_objects (
    shelf_id         BIGINT      NOT NULL REFERENCES storage_mounts (id) ON DELETE CASCADE,
    object_hash      TEXT        NOT NULL,
    path             TEXT        NOT NULL CHECK (char_length(path) BETWEEN 1 AND 1024),
    size             BIGINT,
    kind             TEXT,
    owner_title      TEXT        CHECK (owner_title IS NULL OR char_length(owner_title) BETWEEN 1 AND 300),
    owner_cover      TEXT        CHECK (owner_cover IS NULL OR char_length(owner_cover) BETWEEN 1 AND 1024),
    owner_note       TEXT        CHECK (owner_note IS NULL OR char_length(owner_note) BETWEEN 1 AND 2000),
    delisted_at      TIMESTAMPTZ,
    delist_strike_id BIGINT,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (shelf_id, object_hash)
);
