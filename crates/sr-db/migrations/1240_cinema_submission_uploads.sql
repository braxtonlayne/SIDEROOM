-- 1240: member video uploads of Cinema submissions (RDY-MED-001). One row per open multipart upload behind
-- `POST /submissions/{id}/cinema/files`: the object store's upload id, which the browser never sees, and
-- the shape of the parts, so a refreshed page can ask which parts arrived and the server can complete the
-- upload itself from `ListParts`. `asset_id` is the `video_assets` row (its `uploaded_by` is the member);
-- the row is deleted when the upload completes or is aborted. No foreign keys: the asset and the
-- submission are cleaned up by their own sweeps.
CREATE TABLE cinema_submission_uploads (
    asset_id      BIGINT      PRIMARY KEY,
    submission_id BIGINT      NOT NULL,
    target_kind   TEXT        NOT NULL CHECK (target_kind IN ('title', 'episode')),
    target_id     BIGINT      NOT NULL,
    s3_upload_id  TEXT        NOT NULL,
    part_size     BIGINT      NOT NULL CHECK (part_size > 0),
    part_count    INTEGER     NOT NULL CHECK (part_count > 0),
    size_bytes    BIGINT      NOT NULL CHECK (size_bytes > 0),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX cinema_submission_uploads_by_submission ON cinema_submission_uploads (submission_id);
