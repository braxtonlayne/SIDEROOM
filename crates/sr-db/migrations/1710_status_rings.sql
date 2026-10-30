-- 1710: PRF-RNG-001 (wave 34), 24-hour status rings.
--
-- A ring is a small thing a member pins to their avatar for one day: a photo, a short clip, a line of text or a
-- track they are listening to. It is shown to mutual friends (`audience = 'friends'`) or to the members of the
-- servers the author chose (`audience = 'guilds'` plus rows in `status_ring_guilds`) and to nobody else.
--
-- * The object of a photo or clip lives on the author's own storage mount when they have one, else in the instance
--   bucket (mount 1) under `rings/`. Text and track rings have no object.
-- * `expires_at` is at most `created_at + 24 h`. The `rings.expire` job (id 1710) deletes the row AND the object
--   once it has passed. Nothing reads a ring after `expires_at`: `status_ring_visible_to` says no.
-- * A ring that a report froze has `hold_id` set. The job and the author's delete leave a held row and its object
--   alone until the hold ends (a trigger refuses the delete outright, as for library rows), the reader never sees
--   it, and staff read the held copy through the logged read. A photo or clip on a member's own mount is copied to
--   the instance bucket (`evidence_key`) as soon as it is held, so the evidence survives the member's bucket.
-- * `status_ring_views` is the author's viewer list. It is kept with the ring (it goes when the ring goes) and is
--   only ever read by the author.
-- * `status_ring_purge` is the queue of objects whose row is already gone (author delete, expiry, erasure); the
--   job drains it, so a storage outage never leaves a row that claims an object is still there.
CREATE TABLE status_rings (
    id            BIGINT PRIMARY KEY,
    author_id     BIGINT NOT NULL REFERENCES accounts (id),
    kind          TEXT NOT NULL CHECK (kind IN ('photo', 'clip', 'text', 'track')),
    body          TEXT NOT NULL DEFAULT '' CHECK (char_length(body) <= 280),
    -- A track ring: { "title": ..., "artist": ... } (text only, no file).
    track         JSONB,
    mount_id      BIGINT REFERENCES storage_mounts (id),
    object_key    TEXT,
    content_type  TEXT,
    bytes         BIGINT CHECK (bytes IS NULL OR bytes > 0),
    duration_ms   INTEGER CHECK (duration_ms IS NULL OR duration_ms BETWEEN 1 AND 60000),
    audience      TEXT NOT NULL CHECK (audience IN ('friends', 'guilds')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL,
    -- The author deleted it while a hold kept the row: hidden at once, removed when the hold ends.
    deleted_at    TIMESTAMPTZ,
    hold_id       BIGINT REFERENCES moderation_holds (id),
    -- Copy of a held object on a member's own mount, in the instance bucket (mount 1).
    evidence_key  TEXT,
    CHECK (expires_at > created_at AND expires_at <= created_at + interval '24 hours'),
    CHECK ((kind IN ('photo', 'clip')) = (object_key IS NOT NULL)),
    CHECK ((object_key IS NULL) OR (mount_id IS NOT NULL AND content_type IS NOT NULL AND bytes IS NOT NULL)),
    CHECK ((kind = 'track') = (track IS NOT NULL)),
    CHECK (kind = 'clip' OR duration_ms IS NULL)
);
CREATE INDEX status_rings_author_idx ON status_rings (author_id, expires_at DESC);
CREATE INDEX status_rings_expiry_idx ON status_rings (expires_at) WHERE hold_id IS NULL;
CREATE INDEX status_rings_hold_idx ON status_rings (hold_id) WHERE hold_id IS NOT NULL;

CREATE TABLE status_ring_guilds (
    ring_id   BIGINT NOT NULL REFERENCES status_rings (id) ON DELETE CASCADE,
    guild_id  BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    PRIMARY KEY (ring_id, guild_id)
);
CREATE INDEX status_ring_guilds_guild_idx ON status_ring_guilds (guild_id);

CREATE TABLE status_ring_views (
    ring_id    BIGINT NOT NULL REFERENCES status_rings (id) ON DELETE CASCADE,
    viewer_id  BIGINT NOT NULL REFERENCES accounts (id),
    viewed_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (ring_id, viewer_id)
);
CREATE INDEX status_ring_views_viewer_idx ON status_ring_views (viewer_id);

CREATE TABLE status_ring_purge (
    id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    mount_id    BIGINT NOT NULL,
    object_key  TEXT NOT NULL,
    attempts    INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One row per ring ever posted, so deleting a ring does not hand back the day's allowance (`rings.max_per_day`).
-- The job prunes rows older than two days.
CREATE TABLE status_ring_posts (
    author_id  BIGINT NOT NULL REFERENCES accounts (id),
    posted_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX status_ring_posts_author_idx ON status_ring_posts (author_id, posted_at DESC);

-- A held ring cannot be deleted (the hold API's rule for every held row).
CREATE FUNCTION status_rings_refuse_held_delete() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.hold_id IS NOT NULL THEN
        RAISE EXCEPTION 'status_rings % is under a moderation hold and cannot be deleted', OLD.id
            USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN OLD;
END;
$$;
CREATE TRIGGER status_rings_held_delete BEFORE DELETE ON status_rings
    FOR EACH ROW EXECUTE FUNCTION status_rings_refuse_held_delete();

-- The ONE audience rule. Can `p_viewer` see ring `p_ring` right now?
--
--   * the ring is live: not expired, not deleted by its author, author's account active;
--   * nobody blocked, either way, between the author and the viewer;
--   * the author sees their own;
--   * `friends`: mutual friends (two `friend` rows);
--   * `guilds`: the viewer and the author are both full members (not pending screening) of one chosen server.
--
-- Every reader (the feed, the media route, the report, the profile page's `ring_tier` cap) asks this function, so a
-- ring a viewer may not see answers exactly like a ring that does not exist.
CREATE FUNCTION status_ring_visible_to(p_ring BIGINT, p_viewer BIGINT) RETURNS BOOLEAN
LANGUAGE sql STABLE AS $$
    SELECT EXISTS (
        SELECT 1
        FROM status_rings r
        JOIN accounts a ON a.id = r.author_id AND a.state = 'active'
        WHERE r.id = p_ring
          AND r.deleted_at IS NULL
          AND r.expires_at > now()
          AND NOT EXISTS (
              SELECT 1 FROM relationships b
              WHERE b.type = 2
                AND ((b.user_id = r.author_id AND b.target_id = p_viewer)
                  OR (b.user_id = p_viewer AND b.target_id = r.author_id)))
          AND (
              r.author_id = p_viewer
              OR (r.audience = 'friends' AND (
                  SELECT count(*) FROM relationships f
                  WHERE f.type = 1
                    AND ((f.user_id = r.author_id AND f.target_id = p_viewer)
                      OR (f.user_id = p_viewer AND f.target_id = r.author_id))) = 2)
              OR (r.audience = 'guilds' AND EXISTS (
                  SELECT 1
                  FROM status_ring_guilds g
                  JOIN guilds gd ON gd.id = g.guild_id AND gd.deleted_at IS NULL
                  JOIN guild_members vm ON vm.guild_id = g.guild_id AND vm.user_id = p_viewer AND NOT vm.pending
                  JOIN guild_members am ON am.guild_id = g.guild_id AND am.user_id = r.author_id AND NOT am.pending
                  WHERE g.ring_id = r.id))
          )
    )
$$;

-- ---- moderation: a ring can be reported and held ------------------------------------------------------------------
-- Each CHECK below is rebuilt from its own current definition with `status_ring` appended, so this migration adds
-- its value whatever other waves added to the same lists before it.
DO $$
DECLARE
    c RECORD;
    t TEXT;
BEGIN
    FOREACH t IN ARRAY ARRAY['moderation_cases', 'moderation_reports'] LOOP
        FOR c IN
            SELECT conname, pg_get_constraintdef(oid) AS def FROM pg_constraint
            WHERE conrelid = t::regclass AND contype = 'c'
              AND pg_get_constraintdef(oid) LIKE '%target_kind%ARRAY%'
        LOOP
            IF c.def NOT LIKE '%''status_ring''%' THEN
                EXECUTE format('ALTER TABLE %I DROP CONSTRAINT %I', t, c.conname);
                EXECUTE format('ALTER TABLE %I ADD CONSTRAINT %I %s', t, c.conname,
                               regexp_replace(c.def, '::text\]', '::text, ''status_ring''::text]'));
            END IF;
        END LOOP;
    END LOOP;
    FOR c IN
        SELECT conname, pg_get_constraintdef(oid) AS def FROM pg_constraint
        WHERE conrelid = 'moderation_holds'::regclass AND contype = 'c'
          AND pg_get_constraintdef(oid) LIKE '%scope_kind%ARRAY%'
    LOOP
        IF c.def NOT LIKE '%''status_ring''%' THEN
            EXECUTE format('ALTER TABLE moderation_holds DROP CONSTRAINT %I', c.conname);
            EXECUTE format('ALTER TABLE moderation_holds ADD CONSTRAINT %I %s', c.conname,
                           regexp_replace(c.def, '::text\]', '::text, ''status_ring''::text]'));
        END IF;
    END LOOP;
END $$;
