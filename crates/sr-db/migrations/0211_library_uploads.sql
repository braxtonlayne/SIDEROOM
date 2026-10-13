-- 0211: Library uploads (HTTP upload path, LIB-ING-002/004/005). One row per uploaded file;
-- the client PUTs the file to object storage through a presigned URL, completes the upload,
-- and the `library.ingest` job converts it. The row is also the status the client polls.

CREATE TABLE reading_uploads (
    id          BIGINT PRIMARY KEY,
    chapter_id  BIGINT NOT NULL REFERENCES reading_chapters (id) ON DELETE CASCADE,
    series_id   BIGINT NOT NULL REFERENCES reading_series (id) ON DELETE CASCADE,
    user_id     BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    filename    TEXT NOT NULL CHECK (char_length(filename) BETWEEN 1 AND 255),
    -- What the file is expected to be, from the chapter's format: an archive of page images
    -- (CBZ/CB7/CBR), an EPUB or a PDF.
    kind        TEXT NOT NULL CHECK (kind IN ('archive', 'epub', 'pdf')),
    size_bytes  BIGINT NOT NULL CHECK (size_bytes > 0),
    object_key  TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'awaiting_upload'
                CHECK (status IN ('awaiting_upload', 'queued', 'running', 'done', 'failed', 'expired')),
    error       TEXT,
    warnings    TEXT[] NOT NULL DEFAULT '{}',
    page_count  INTEGER,
    -- Facts read from the file (title, number, authors ...), for the editor to offer.
    metadata    JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ
);
CREATE INDEX reading_uploads_chapter_idx ON reading_uploads (chapter_id, created_at DESC);
CREATE INDEX reading_uploads_pending_idx ON reading_uploads (created_at) WHERE status = 'awaiting_upload';
