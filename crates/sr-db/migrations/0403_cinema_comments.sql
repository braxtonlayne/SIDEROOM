-- 0403: Cinema comments (one reply level, likes) and reports (CIN-SOC-001).

CREATE TABLE cinema_comments (
    id          BIGINT PRIMARY KEY,
    user_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    title_id    BIGINT NOT NULL CHECK (title_id > 0),
    episode_id  BIGINT NOT NULL DEFAULT 0 CHECK (episode_id >= 0),
    parent_id   BIGINT REFERENCES cinema_comments (id) ON DELETE CASCADE,
    body        TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 2000),
    spoiler     BOOLEAN NOT NULL DEFAULT FALSE,
    like_count  INTEGER NOT NULL DEFAULT 0 CHECK (like_count >= 0),
    hidden      BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    edited_at   TIMESTAMPTZ
);
CREATE INDEX cinema_comments_thread_idx ON cinema_comments (title_id, episode_id, created_at DESC, id DESC)
    WHERE parent_id IS NULL;
CREATE INDEX cinema_comments_replies_idx ON cinema_comments (parent_id, created_at, id) WHERE parent_id IS NOT NULL;
CREATE INDEX cinema_comments_user_idx ON cinema_comments (user_id);

CREATE TABLE cinema_comment_likes (
    comment_id BIGINT NOT NULL REFERENCES cinema_comments (id) ON DELETE CASCADE,
    user_id    BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (comment_id, user_id)
);

-- Reports on reviews and comments; one per reporter and target. Targets are not foreign keys so a
-- report outlives deleted content for the moderators' record.
CREATE TABLE cinema_reports (
    id           BIGINT PRIMARY KEY,
    reporter_id  BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    target_kind  TEXT NOT NULL CHECK (target_kind IN ('review', 'comment')),
    target_id    BIGINT NOT NULL,
    reason       TEXT NOT NULL CHECK (char_length(reason) BETWEEN 1 AND 500),
    status       TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'actioned', 'dismissed')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (reporter_id, target_kind, target_id)
);
CREATE INDEX cinema_reports_open_idx ON cinema_reports (status, created_at);
