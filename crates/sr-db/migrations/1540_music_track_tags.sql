-- 1540 (MUS-IDX-001): the client-written tag state of a music track.
--
-- `music_tracks` (migration 1450) already carries the tag columns (title, artist, album, album_artist, track_no,
-- disc_no, year, genres, cover_key) and `overridden`, the list of fields a re-index must leave alone. The indexer
-- fills them from the file name; the browser then reads the real tags (first 1 MiB of the file, by presigned
-- range) and writes them through `PATCH /music/tracks/{id}`. This adds what the lazy tagger needs:
--
-- * `tags_state`: `pending` (never read), `read` (tags written), `none` (read, the file carries no tags) or
--   `failed` (could not be read; "Rescan tags" queues it again).
-- * `tags_read_at`: when the browser last wrote tags.
-- * `tagged`: the subset of `overridden` that came from the file's own tags, not from a member's edit. A rescan
--   overwrites `tagged` fields and never a field the member edited by hand.
-- * `cover_hash`: hash in the key of the cover the browser uploaded (`covers/<hash>.jpg` on the same mount).
ALTER TABLE music_tracks
    ADD COLUMN tags_state   TEXT NOT NULL DEFAULT 'pending'
        CHECK (tags_state IN ('pending', 'read', 'none', 'failed')),
    ADD COLUMN tags_read_at TIMESTAMPTZ,
    ADD COLUMN tagged       TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN cover_hash   TEXT CHECK (cover_hash IS NULL OR cover_hash ~ '^[0-9a-f]{8,64}$');

CREATE INDEX music_tracks_pending_idx ON music_tracks (mount_id, album, id)
    WHERE tags_state IN ('pending', 'failed') AND shelf_state = 'listed';
CREATE INDEX music_tracks_artist_idx ON music_tracks (mount_id, artist, album, track_no, id);
