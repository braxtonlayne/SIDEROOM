-- 1501 (BYOS-CNV-003): what a finished `mount_transcode` job left in the member's bucket.
--
-- One row per (mount, source key): the latest finished conversion. `prefix` is the mount-relative folder the
-- outputs live under (`transcoded/<key>/`), `entry` the path of the file a player opens inside it (the single
-- MP4, or the first HLS rung playlist), `outputs` every file with its size. The catalog (`playable_key` of
-- migration 1480, wired by the integrator) reads `prefix` + `entry`; nothing here is served by the instance.
CREATE TABLE storage_object_transcodes (
    mount_id    BIGINT NOT NULL,
    key         TEXT NOT NULL CHECK (char_length(key) BETWEEN 1 AND 1024),
    format      TEXT NOT NULL CHECK (format IN ('hls', 'mp4')),
    preset      TEXT NOT NULL CHECK (preset IN ('low', 'standard', 'high')),
    prefix      TEXT NOT NULL,
    entry       TEXT NOT NULL,
    outputs     JSONB NOT NULL DEFAULT '[]'::jsonb,
    size_bytes  BIGINT NOT NULL DEFAULT 0 CHECK (size_bytes >= 0),
    job_id      BIGINT NOT NULL,
    finished_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (mount_id, key)
);
