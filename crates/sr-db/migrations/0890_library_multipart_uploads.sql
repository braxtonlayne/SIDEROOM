-- 0890: Resumable multipart uploads of chapter files (REV-UP-001). One row per upload slot that
-- was opened as a multipart upload: the id the object store gave it and the part size the client
-- must use. The parts themselves live in the object store (`ListParts`), so a refreshed browser
-- learns what arrived from the store, not from this table.

CREATE TABLE reading_upload_multipart (
    upload_id    BIGINT PRIMARY KEY REFERENCES reading_uploads (id) ON DELETE CASCADE,
    multipart_id TEXT NOT NULL CHECK (char_length(multipart_id) BETWEEN 1 AND 1024),
    part_size    BIGINT NOT NULL CHECK (part_size >= 1)
);
