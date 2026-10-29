-- 1480 (BYOS-CNV-001): the playable copy of an object that browsers cannot play as it is.
--
-- A member converts an MKV/AVI/MOV/TS in the browser (remux through ffmpeg.wasm, or WebCodecs) and uploads the result
-- to `converted/<original>.mp4` beside the original, which stays. `playable_key` on the original's `storage_objects`
-- row names that file (mount-relative key, like `key`); readers play `COALESCE(playable_key, key)`. `converter`
-- records how it was made ('remux', 'webcodecs', 'worker', 'ffmpeg') and `converted_at` when, so a re-index can tell a
-- real conversion from a stray file and the UI can say "converted in your browser". All nullable and additive:
-- nothing changes for an existing row.
ALTER TABLE storage_objects
    ADD COLUMN playable_key  TEXT CHECK (playable_key IS NULL OR char_length(playable_key) BETWEEN 1 AND 1024),
    ADD COLUMN converter     TEXT CHECK (converter IS NULL OR converter IN ('remux', 'webcodecs', 'worker', 'ffmpeg')),
    ADD COLUMN converted_at  TIMESTAMPTZ;
CREATE INDEX storage_objects_playable_idx ON storage_objects (mount_id, playable_key) WHERE playable_key IS NOT NULL;
