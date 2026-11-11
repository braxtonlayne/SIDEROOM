-- 2510 (SEC-AUD-053 / SEC-AUD-054, D-583 / D-584): byte takedowns match on the bytes when the entity tag cannot, and
-- a byte-matched item tells its owner.
--
-- * `takedown_items.object_sha256`: the sha256 of the upheld object, read once by `disable_item` (NULL when the read
--   failed or the notice was upheld before this migration; the item then keeps the tag and size only).
-- * `storage_objects.content_sha256`: the sha256 the indexer computed because the object's size equalled a standing
--   byte takedown's (D-583). It is kept while the entity tag and size stay the same, so an object is never read twice;
--   the indexer clears it when either changes.
-- * `takedown_items.notified_at`: when the owner of a byte-matched (derived) item was sent the notice's removal DM
--   (D-584). Derived items now carry `subject_account_id` (the mount's owner, or on a server's bucket the uploader as
--   P28-LEG-1 resolves it) so the restore DM reaches them too; they never carry `subject_person_id`, so no strike.
ALTER TABLE storage_objects
    ADD COLUMN content_sha256 TEXT CHECK (content_sha256 IS NULL OR content_sha256 ~ '^[0-9a-f]{64}$');

ALTER TABLE takedown_items
    ADD COLUMN object_sha256 TEXT CHECK (object_sha256 IS NULL OR object_sha256 ~ '^[0-9a-f]{64}$'),
    ADD COLUMN notified_at TIMESTAMPTZ;

-- A fingerprint is a tag and/or a sha256, always with a size.
ALTER TABLE takedown_items DROP CONSTRAINT takedown_items_object_hash_chk;
ALTER TABLE takedown_items ADD CONSTRAINT takedown_items_object_hash_chk
    CHECK ((object_size IS NULL) = (object_hash IS NULL AND object_sha256 IS NULL)
           AND (object_size IS NULL OR kind = 'storage_object'));

-- The indexer's sieve and match: the standing byte takedowns that carry a sha256, by size.
CREATE INDEX takedown_items_sha_idx ON takedown_items (object_size, object_sha256)
    WHERE kind = 'storage_object' AND state = 'disabled' AND object_sha256 IS NOT NULL;
-- Derived items whose owner still has to be told.
CREATE INDEX takedown_items_unnotified_idx ON takedown_items (notice_id)
    WHERE derived_from IS NOT NULL AND notified_at IS NULL AND subject_account_id IS NOT NULL;
