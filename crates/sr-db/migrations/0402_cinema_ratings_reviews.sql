-- 0402: Cinema ratings (1 to 10) and reviews (CIN-SOC-001). Public independently of history (D-120).

CREATE TABLE cinema_ratings (
    user_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    title_id    BIGINT NOT NULL CHECK (title_id > 0),
    episode_id  BIGINT NOT NULL DEFAULT 0 CHECK (episode_id >= 0),
    score       SMALLINT NOT NULL CHECK (score BETWEEN 1 AND 10),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, title_id, episode_id)
);
CREATE INDEX cinema_ratings_title_idx ON cinema_ratings (title_id, episode_id);
CREATE INDEX cinema_ratings_recent_idx ON cinema_ratings (updated_at DESC);

CREATE TABLE cinema_reviews (
    id            BIGINT PRIMARY KEY,
    user_id       BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    title_id      BIGINT NOT NULL CHECK (title_id > 0),
    body          TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 8000),
    spoiler       BOOLEAN NOT NULL DEFAULT FALSE,
    helpful_count INTEGER NOT NULL DEFAULT 0 CHECK (helpful_count >= 0),
    hidden        BOOLEAN NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, title_id)
);
CREATE INDEX cinema_reviews_title_idx ON cinema_reviews (title_id, created_at DESC);
CREATE INDEX cinema_reviews_helpful_idx ON cinema_reviews (title_id, helpful_count DESC, id DESC);

CREATE TABLE cinema_review_votes (
    review_id  BIGINT NOT NULL REFERENCES cinema_reviews (id) ON DELETE CASCADE,
    user_id    BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (review_id, user_id)
);
