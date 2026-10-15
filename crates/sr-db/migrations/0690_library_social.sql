-- 0690: Library social features. LIB-SOC-001 (ratings), LIB-SOC-003 (reviews), LIB-SOC-006 (lists).
-- Ratings and reviews are public acts, independent of reading history (D-120). Owned by the account.

CREATE TABLE reading_ratings (
    user_id    BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    series_id  BIGINT NOT NULL REFERENCES reading_series (id) ON DELETE CASCADE,
    score      SMALLINT NOT NULL CHECK (score BETWEEN 1 AND 10),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, series_id)
);
CREATE INDEX reading_ratings_series_idx ON reading_ratings (series_id);

CREATE TABLE reading_chapter_ratings (
    user_id    BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    chapter_id BIGINT NOT NULL REFERENCES reading_chapters (id) ON DELETE CASCADE,
    score      SMALLINT NOT NULL CHECK (score BETWEEN 1 AND 10),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, chapter_id)
);
CREATE INDEX reading_chapter_ratings_chapter_idx ON reading_chapter_ratings (chapter_id);

CREATE TABLE reading_reviews (
    id            BIGINT PRIMARY KEY,
    user_id       BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    series_id     BIGINT NOT NULL REFERENCES reading_series (id) ON DELETE CASCADE,
    body          TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 8000),
    spoiler       BOOLEAN NOT NULL DEFAULT FALSE,
    helpful_count INTEGER NOT NULL DEFAULT 0 CHECK (helpful_count >= 0),
    hidden        BOOLEAN NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, series_id)
);
CREATE INDEX reading_reviews_series_idx ON reading_reviews (series_id, created_at DESC);
CREATE INDEX reading_reviews_helpful_idx ON reading_reviews (series_id, helpful_count DESC, id DESC);

CREATE TABLE reading_review_votes (
    review_id  BIGINT NOT NULL REFERENCES reading_reviews (id) ON DELETE CASCADE,
    user_id    BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (review_id, user_id)
);

-- Reading lists. A featured list (set by staff) must be public.
CREATE TABLE reading_lists (
    id          BIGINT PRIMARY KEY,
    owner_id    BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    name        TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 100),
    description TEXT NOT NULL DEFAULT '' CHECK (char_length(description) <= 500),
    visibility  TEXT NOT NULL DEFAULT 'private' CHECK (visibility IN ('private', 'friends', 'public')),
    featured_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (featured_at IS NULL OR visibility = 'public')
);
CREATE INDEX reading_lists_owner_idx ON reading_lists (owner_id, created_at);
CREATE INDEX reading_lists_featured_idx ON reading_lists (featured_at DESC) WHERE featured_at IS NOT NULL;

CREATE TABLE reading_list_items (
    list_id   BIGINT NOT NULL REFERENCES reading_lists (id) ON DELETE CASCADE,
    series_id BIGINT NOT NULL REFERENCES reading_series (id) ON DELETE CASCADE,
    position  BIGINT NOT NULL,
    note      TEXT NOT NULL DEFAULT '' CHECK (char_length(note) <= 500),
    added_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (list_id, series_id)
);
CREATE INDEX reading_list_items_order_idx ON reading_list_items (list_id, position);
CREATE INDEX reading_list_items_series_idx ON reading_list_items (series_id);
