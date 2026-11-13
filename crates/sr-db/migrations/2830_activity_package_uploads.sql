-- SEC-AUD-176 (D-654, wave 65): an upload of an Activity package in parts. The browser sends the zip to the
-- bucket in parts (presigned part URLs); this row is the session: which object, how it is cut, where the check of
-- the stored zip stands, and how it ended. A row holds no personal data beyond the uploader's account id.
-- Rows past `expires_at` that are not finished are aborted and set `expired` by the daily sweep, and every row is
-- deleted after seven days. The bundle the check made is only referenced (`bundle_id`); deleting the bundle
-- clears the reference.
CREATE TABLE activity_package_uploads (
    id             BIGINT PRIMARY KEY,
    application_id BIGINT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    created_by     BIGINT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at     TIMESTAMPTZ NOT NULL,
    -- The announced size of the zip, and how it is cut (every part but the last has `part_size` bytes).
    size           BIGINT NOT NULL CHECK (size > 0),
    part_size      BIGINT NOT NULL CHECK (part_size > 0),
    part_count     INTEGER NOT NULL CHECK (part_count BETWEEN 1 AND 10000),
    -- The staged object (`activities/{application}/uploads/{id}.zip`) and the store's id of the open multipart upload.
    object_key     TEXT NOT NULL,
    multipart_id   TEXT NOT NULL,
    state          TEXT NOT NULL DEFAULT 'uploading'
                   CHECK (state IN ('uploading', 'checking', 'ready', 'refused', 'expired', 'aborted')),
    -- Bytes of the zip the checker has read so far (progress of `checking`).
    checked_bytes  BIGINT NOT NULL DEFAULT 0 CHECK (checked_bytes >= 0),
    bundle_id      BIGINT REFERENCES application_activity_bundles (id) ON DELETE SET NULL,
    -- The reason a package was refused, as shown to the developer.
    error          TEXT
);
CREATE INDEX activity_package_uploads_app_idx ON activity_package_uploads (application_id, created_at DESC);
CREATE INDEX activity_package_uploads_state_idx ON activity_package_uploads (state, expires_at);
