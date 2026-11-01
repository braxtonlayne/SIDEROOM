-- 2020: Phase 41 wave C (STU-ZK-001, P28-SHELF-1/2, P28-LEG-1; D-393, D-394): server-side shelf access with
-- per-object delisting. A shelf IS a storage mount (`shelf_id` = `storage_mounts.id`); the credentials stay sealed
-- in `storage_mounts.secret_key_enc` and never reach a grantee, who gets only 10-minute presigned URLs.
--
-- * `shelf_grants`: account-bound grants (friend or guild role), so access works on any device. A grant is
--   revoked by setting `revoked_at`; the row stays for the audit trail. Existing friend shares
--   (`storage_mount_shares`) and guild grants (`guild_storage_grants`) are untouched and keep working.
-- * `shelf_delist_strikes`: one row per staff delisting of an object, kept forever (D-212).
-- * `shelf_objects`: the plaintext catalog by `object_hash` (hex sha256 of the object's path in the mount;
--   an owner client may later supply a content hash). Kept in step with `storage_objects` by a trigger; the
--   owner-edited title/cover/note and the delisting survive a re-index.

CREATE TABLE shelf_delist_strikes (
    id               BIGINT PRIMARY KEY,
    shelf_id         BIGINT NOT NULL REFERENCES storage_mounts (id),
    object_hash      TEXT NOT NULL CHECK (object_hash ~ '^[0-9a-f]{64}$'),
    path             TEXT NOT NULL CHECK (char_length(path) BETWEEN 1 AND 1024),
    -- The account the strike is against (the shelf's owner); NULL for a server shelf.
    subject_account  BIGINT REFERENCES accounts (id),
    notice_id        BIGINT REFERENCES takedown_notices (id),
    reason           TEXT NOT NULL CHECK (char_length(reason) BETWEEN 1 AND 2000),
    delisted_by      BIGINT NOT NULL REFERENCES accounts (id),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    restored_at      TIMESTAMPTZ,
    restored_by      BIGINT REFERENCES accounts (id)
);
CREATE INDEX shelf_delist_strikes_shelf_idx ON shelf_delist_strikes (shelf_id, object_hash);
CREATE INDEX shelf_delist_strikes_subject_idx ON shelf_delist_strikes (subject_account) WHERE subject_account IS NOT NULL;
CREATE TRIGGER shelf_delist_strikes_keep BEFORE DELETE ON shelf_delist_strikes
    FOR EACH ROW EXECUTE FUNCTION takedown_keep_forever();

CREATE TABLE shelf_objects (
    shelf_id         BIGINT NOT NULL REFERENCES storage_mounts (id) ON DELETE CASCADE,
    object_hash      TEXT NOT NULL CHECK (object_hash ~ '^[0-9a-f]{64}$'),
    path             TEXT NOT NULL CHECK (char_length(path) BETWEEN 1 AND 1024),
    size             BIGINT NOT NULL DEFAULT 0 CHECK (size >= 0),
    kind             TEXT CHECK (kind IN ('cinema', 'library', 'music', 'games')),
    class            TEXT NOT NULL DEFAULT '',
    -- False once the object vanished from the bucket or moved to another path.
    listed           BOOLEAN NOT NULL DEFAULT TRUE,
    owner_title      TEXT NOT NULL DEFAULT '' CHECK (char_length(owner_title) <= 300),
    owner_cover      TEXT NOT NULL DEFAULT '' CHECK (char_length(owner_cover) <= 1024),
    owner_note       TEXT NOT NULL DEFAULT '' CHECK (char_length(owner_note) <= 2000),
    delisted_at      TIMESTAMPTZ,
    delist_strike_id BIGINT REFERENCES shelf_delist_strikes (id),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (shelf_id, object_hash),
    CHECK ((delisted_at IS NULL) = (delist_strike_id IS NULL))
);
CREATE INDEX shelf_objects_listing_idx ON shelf_objects (shelf_id, object_hash) WHERE listed AND delisted_at IS NULL;
CREATE UNIQUE INDEX shelf_objects_path_uq ON shelf_objects (shelf_id, path) WHERE listed;

CREATE FUNCTION shelf_object_hash(p TEXT) RETURNS TEXT LANGUAGE sql IMMUTABLE AS
$$ SELECT encode(sha256(convert_to(p, 'UTF8')), 'hex') $$;

CREATE FUNCTION shelf_objects_sync() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        UPDATE shelf_objects SET listed = FALSE, updated_at = now()
         WHERE shelf_id = OLD.mount_id AND object_hash = shelf_object_hash(OLD.key);
        RETURN OLD;
    END IF;
    IF TG_OP = 'UPDATE' AND OLD.key <> NEW.key THEN
        UPDATE shelf_objects SET listed = FALSE, updated_at = now()
         WHERE shelf_id = OLD.mount_id AND object_hash = shelf_object_hash(OLD.key);
    END IF;
    -- storage_objects has no foreign key to its mount: a row of a mount that is gone (deleted mid-index) is not
    -- catalogued instead of failing the indexer (the back-fill below joins the mounts for the same reason).
    IF NOT EXISTS (SELECT 1 FROM storage_mounts WHERE id = NEW.mount_id) THEN
        RETURN NEW;
    END IF;
    INSERT INTO shelf_objects (shelf_id, object_hash, path, size, kind, class, listed)
    VALUES (NEW.mount_id, shelf_object_hash(NEW.key), NEW.key, NEW.size_bytes, NEW.kind, NEW.class,
            NEW.state = 'indexed')
    ON CONFLICT (shelf_id, object_hash) DO UPDATE
       SET path = EXCLUDED.path, size = EXCLUDED.size, kind = EXCLUDED.kind, class = EXCLUDED.class,
           listed = EXCLUDED.listed, updated_at = now();
    RETURN NEW;
END $$;
CREATE TRIGGER storage_objects_shelf_sync AFTER INSERT OR UPDATE OR DELETE ON storage_objects
    FOR EACH ROW EXECUTE FUNCTION shelf_objects_sync();

-- Back-fill from the objects already indexed.
INSERT INTO shelf_objects (shelf_id, object_hash, path, size, kind, class, listed)
SELECT o.mount_id, shelf_object_hash(o.key), o.key, o.size_bytes, o.kind, o.class, o.state = 'indexed'
  FROM storage_objects o JOIN storage_mounts m ON m.id = o.mount_id
ON CONFLICT DO NOTHING;

CREATE TABLE shelf_grants (
    id           BIGINT PRIMARY KEY,
    shelf_id     BIGINT NOT NULL REFERENCES storage_mounts (id) ON DELETE CASCADE,
    grantee_kind TEXT NOT NULL CHECK (grantee_kind IN ('friend', 'guild_role')),
    -- The friend's account id, or the role id.
    grantee_id   BIGINT NOT NULL,
    -- The guild of the role; set exactly for a guild_role grant.
    guild_id     BIGINT REFERENCES guilds (id) ON DELETE CASCADE,
    granted_by   BIGINT NOT NULL REFERENCES accounts (id),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at   TIMESTAMPTZ,
    revoked_by   BIGINT REFERENCES accounts (id),
    CHECK ((grantee_kind = 'guild_role') = (guild_id IS NOT NULL))
);
CREATE UNIQUE INDEX shelf_grants_active_uq ON shelf_grants (shelf_id, grantee_kind, grantee_id) WHERE revoked_at IS NULL;
CREATE INDEX shelf_grants_grantee_idx ON shelf_grants (grantee_kind, grantee_id) WHERE revoked_at IS NULL;
CREATE INDEX shelf_grants_shelf_idx ON shelf_grants (shelf_id, created_at DESC);
