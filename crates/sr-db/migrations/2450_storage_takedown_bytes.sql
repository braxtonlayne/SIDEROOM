-- 2450 (SEC-AUD-048, D-571): storage-object takedowns follow the bytes.
--
-- A notice names a work, not a path. Until now a `storage_object` takedown item delisted one key of one mount, so
-- the same file uploaded again under another name (or to another bucket) was served. Now:
--
-- * `storage_objects.content_tag` is the fingerprint of an object's bytes from the listing's entity tag, written by
--   the catalog indexer when the provider's tag is a content digest (S3-compatible stores: the MD5 of a single-part
--   upload, or the MD5 of the part digests with `-<parts>` for a multipart one; `sr_storage::index::content_tag`).
--   Any other tag (a counter, a file system's inode and time) is left NULL and never matches.
-- * `takedown_items.object_hash` / `object_size` record that fingerprint when the item is upheld (from the
--   catalog row of the mount and key; NULL when the object was never catalogued).
-- * The indexer looks every catalogued object it lists up by (hash, size) among the disabled items. A match on any
--   mount is recorded as a new `storage_object` item of the SAME notice, already `disabled`, with `derived_from`
--   naming the item it matched, and the mount's CDN path of the key is blocked under the notice. Every existing
--   reader of disabled items therefore delists it, and the notice's restore (which restores every item of the
--   notice and lifts its blocks) lifts it too. Path blocks stay as they are.
ALTER TABLE storage_objects
    ADD COLUMN content_tag TEXT CHECK (content_tag IS NULL OR content_tag ~ '^[0-9a-f]{32}(-[1-9][0-9]{0,4})?$');

ALTER TABLE takedown_items
    ADD COLUMN object_hash TEXT CHECK (object_hash IS NULL OR object_hash ~ '^[0-9a-f]{32}(-[1-9][0-9]{0,4})?$'),
    ADD COLUMN object_size BIGINT CHECK (object_size IS NULL OR object_size >= 0),
    ADD COLUMN derived_from BIGINT REFERENCES takedown_items (id);
ALTER TABLE takedown_items ADD CONSTRAINT takedown_items_object_hash_chk
    CHECK ((object_hash IS NULL) = (object_size IS NULL) AND (object_hash IS NULL OR kind = 'storage_object'));
ALTER TABLE takedown_items ADD CONSTRAINT takedown_items_derived_chk
    CHECK (derived_from IS NULL OR kind = 'storage_object');

-- The indexer's lookup: the standing byte takedowns by fingerprint.
CREATE INDEX takedown_items_bytes_idx ON takedown_items (object_hash, object_size)
    WHERE kind = 'storage_object' AND state = 'disabled' AND object_hash IS NOT NULL;
-- Upholding reads the catalog row of (mount, key); that is the existing UNIQUE (mount_id, key).
