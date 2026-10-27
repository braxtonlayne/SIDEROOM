-- 1460 (BYOS-LEG-001): takedown and delisting for user-owned buckets.
--
-- The platform must not and cannot delete a member's property, so a notice against an object in a member's
-- bucket DELISTS it: `takedown_items` gains the kind `storage_object` (target_id = the mount id, object_key = the
-- mount-relative key), `sr_storage::scopes::resolve` refuses a key with a disabled storage_object item, and the
-- object stays in the owner's bucket. Counter-notice restores through the existing clock (the item goes to
-- 'restored', which is the only state that stops delisting).
--
-- Staff may also delist a whole mount: storage_mounts.status gains 'delisted' with who, when and why (the owner is
-- told why). A platform ban or suspension needs no row: `ScopeFacts::owner_active` already refuses every mount of
-- an account that is not active, and a lifted ban restores them with no further step.
ALTER TABLE takedown_items DROP CONSTRAINT takedown_items_kind_check;
ALTER TABLE takedown_items ADD CONSTRAINT takedown_items_kind_check
    CHECK (kind IN ('message', 'attachment', 'library_series', 'library_chapter',
                    'cinema_title', 'cinema_episode', 'storage_object'));
ALTER TABLE takedown_items ADD COLUMN object_key TEXT CHECK (object_key IS NULL OR char_length(object_key) BETWEEN 1 AND 1024);
ALTER TABLE takedown_items ADD CONSTRAINT takedown_items_object_key_chk
    CHECK ((kind = 'storage_object') = (object_key IS NOT NULL));
-- One key of one mount may appear once per notice; the old unique constraint cannot tell keys apart.
ALTER TABLE takedown_items DROP CONSTRAINT takedown_items_notice_id_kind_target_id_key;
CREATE UNIQUE INDEX takedown_items_notice_target_uq ON takedown_items (notice_id, kind, target_id, COALESCE(object_key, ''));
-- `delisted(mount, keys)` reads the disabled items of one mount.
CREATE INDEX takedown_items_storage_idx ON takedown_items (target_id, object_key) WHERE kind = 'storage_object' AND state = 'disabled';

ALTER TABLE storage_mounts DROP CONSTRAINT storage_mounts_status_check;
ALTER TABLE storage_mounts ADD CONSTRAINT storage_mounts_status_check
    CHECK (status IN ('active', 'failing', 'disabled', 'delisted'));
ALTER TABLE storage_mounts
    ADD COLUMN delisted_at     TIMESTAMPTZ,
    ADD COLUMN delisted_by     BIGINT REFERENCES accounts (id),
    ADD COLUMN delist_reason   TEXT NOT NULL DEFAULT '' CHECK (char_length(delist_reason) <= 2000),
    -- The status the mount had before it was delisted, so relisting puts back exactly that.
    ADD COLUMN delist_prior    TEXT CHECK (delist_prior IS NULL OR delist_prior IN ('active', 'failing', 'disabled'));
