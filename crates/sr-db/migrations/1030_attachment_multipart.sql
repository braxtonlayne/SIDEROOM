-- 1030: resumable multipart uploads of chat attachments above 64 MB (LNG-CMP-007).
-- One row per upload slot that was started as an S3 multipart upload; it goes away with the slot.
CREATE TABLE attachment_multipart (
    attachment_id BIGINT PRIMARY KEY REFERENCES attachments (id) ON DELETE CASCADE,
    multipart_id  TEXT NOT NULL,
    part_size     BIGINT NOT NULL CHECK (part_size > 0),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
