-- 1440 (BYOS-BRW-001): the ledger of browser uploads into a member's or server's bucket.
--
-- A row is one S3 multipart upload the browser performs part by part against presigned URLs; the server
-- never sees the bytes. `s3_upload_id` is the bucket's id for it and never leaves the server (the browser
-- gets `id`). A row stays `open` until the browser completes or aborts it, or until the sweep job
-- (`storage.uploads_sweep`) finds it untouched for 24 hours and aborts it at the bucket, which frees the parts
-- the provider would otherwise keep billing. `updated_at` moves whenever a part URL is issued.
--
-- `storage_mount_part_notes` counts part URLs issued per member (the `byos.upload_parts_per_hour` quota); the
-- sweep deletes notes older than an hour.
CREATE TABLE storage_mount_uploads (
    id            BIGINT PRIMARY KEY,
    mount_id      BIGINT NOT NULL REFERENCES storage_mounts (id) ON DELETE CASCADE,
    user_id       BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    object_key    TEXT NOT NULL CHECK (char_length(object_key) BETWEEN 1 AND 1024),
    s3_upload_id  TEXT NOT NULL CHECK (char_length(s3_upload_id) BETWEEN 1 AND 1024),
    size_bytes    BIGINT NOT NULL CHECK (size_bytes >= 0),
    part_size     BIGINT NOT NULL CHECK (part_size > 0),
    part_count    INTEGER NOT NULL CHECK (part_count BETWEEN 1 AND 10000),
    content_type  TEXT NOT NULL DEFAULT 'application/octet-stream' CHECK (char_length(content_type) <= 255),
    state         TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'completed', 'aborted')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- The browser lists a member's open uploads on one mount (resume after a reload).
CREATE INDEX storage_mount_uploads_open_idx ON storage_mount_uploads (mount_id, user_id) WHERE state = 'open';
-- The sweep reads open rows by age.
CREATE INDEX storage_mount_uploads_sweep_idx ON storage_mount_uploads (updated_at) WHERE state = 'open';
-- The per-member open-upload cap.
CREATE INDEX storage_mount_uploads_user_idx ON storage_mount_uploads (user_id) WHERE state = 'open';

CREATE TABLE storage_mount_part_notes (
    user_id   BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    noted_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    parts     INTEGER NOT NULL CHECK (parts > 0)
);
CREATE INDEX storage_mount_part_notes_idx ON storage_mount_part_notes (user_id, noted_at);
