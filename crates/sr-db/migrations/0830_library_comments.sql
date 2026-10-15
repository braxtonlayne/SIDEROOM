-- 0830: Library comments (LIB-SOC-003, LIB-RDR-014): threaded (one reply level) comments on a
-- series (`chapter_id` 0) or one of its chapters, likes, and reports for the review queue.

CREATE TABLE reading_comments (
    id          BIGINT PRIMARY KEY,
    user_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    series_id   BIGINT NOT NULL REFERENCES reading_series (id) ON DELETE CASCADE,
    chapter_id  BIGINT NOT NULL DEFAULT 0 CHECK (chapter_id >= 0),
    parent_id   BIGINT REFERENCES reading_comments (id) ON DELETE CASCADE,
    body        TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 2000),
    spoiler     BOOLEAN NOT NULL DEFAULT FALSE,
    like_count  INTEGER NOT NULL DEFAULT 0 CHECK (like_count >= 0),
    hidden      BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    edited_at   TIMESTAMPTZ
);
CREATE INDEX reading_comments_thread_idx ON reading_comments (series_id, chapter_id, created_at DESC, id DESC)
    WHERE parent_id IS NULL;
CREATE INDEX reading_comments_replies_idx ON reading_comments (parent_id, created_at, id)
    WHERE parent_id IS NOT NULL;
CREATE INDEX reading_comments_user_idx ON reading_comments (user_id);

CREATE TABLE reading_comment_likes (
    comment_id BIGINT NOT NULL REFERENCES reading_comments (id) ON DELETE CASCADE,
    user_id    BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (comment_id, user_id)
);

-- One report per reporter and comment. The comment is not a foreign key so the record outlives it.
CREATE TABLE reading_comment_reports (
    id           BIGINT PRIMARY KEY,
    reporter_id  BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    comment_id   BIGINT NOT NULL,
    reason       TEXT NOT NULL CHECK (char_length(reason) BETWEEN 1 AND 500),
    status       TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'actioned', 'dismissed')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_by  BIGINT,
    resolved_at  TIMESTAMPTZ,
    UNIQUE (reporter_id, comment_id)
);
CREATE INDEX reading_comment_reports_open_idx ON reading_comment_reports (status, created_at);
