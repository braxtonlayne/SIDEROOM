-- 0200: Library catalog. docs/04 section 11, LIB-DATA-001.
-- Ids are application-generated Snowflakes. Tables carry the `reading_` prefix so they cannot
-- clash with future generic names (`volumes`, `chapters`, `pages`).
-- Visibility: a chapter is readable only when it AND its series are published; age-restricted
-- series are additionally hidden from accounts without `age_confirmed_at` (LIB-DATA-005).

CREATE TABLE reading_series (
    id                BIGINT PRIMARY KEY,
    kind              TEXT NOT NULL
                      CHECK (kind IN ('manga', 'manhwa', 'manhua', 'webtoon', 'comic', 'novel', 'book')),
    title             TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 300),
    alt_titles        TEXT[] NOT NULL DEFAULT '{}',
    authors           TEXT[] NOT NULL DEFAULT '{}',
    artists           TEXT[] NOT NULL DEFAULT '{}',
    status            TEXT NOT NULL DEFAULT 'ongoing'
                      CHECK (status IN ('ongoing', 'completed', 'hiatus', 'cancelled')),
    year              SMALLINT CHECK (year BETWEEN 1800 AND 2200),
    synopsis          TEXT NOT NULL DEFAULT '',
    genres            TEXT[] NOT NULL DEFAULT '{}',
    tags              TEXT[] NOT NULL DEFAULT '{}',
    reading_direction TEXT NOT NULL DEFAULT 'rtl'
                      CHECK (reading_direction IN ('rtl', 'ltr', 'vertical')),
    age_restricted    BOOLEAN NOT NULL DEFAULT FALSE,
    cover_key         TEXT,
    published         BOOLEAN NOT NULL DEFAULT FALSE,
    published_at      TIMESTAMPTZ,
    -- Set when this series was merged into another; readers are redirected to it.
    merged_into       BIGINT REFERENCES reading_series (id) ON DELETE SET NULL,
    -- Denormalised from published chapters, kept by the repository in the same transaction.
    chapter_count     INTEGER NOT NULL DEFAULT 0 CHECK (chapter_count >= 0),
    last_chapter_at   TIMESTAMPTZ,
    follower_count    INTEGER NOT NULL DEFAULT 0 CHECK (follower_count >= 0),
    created_by        BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- Browse orderings over the visible (published, not merged) set.
CREATE INDEX reading_series_browse_idx ON reading_series (kind, published_at DESC)
    WHERE published AND merged_into IS NULL;
CREATE INDEX reading_series_updated_idx ON reading_series (last_chapter_at DESC NULLS LAST)
    WHERE published AND merged_into IS NULL;
CREATE INDEX reading_series_title_idx ON reading_series (lower(title))
    WHERE published AND merged_into IS NULL;
CREATE INDEX reading_series_genres_idx ON reading_series USING GIN (genres);
CREATE INDEX reading_series_tags_idx ON reading_series USING GIN (tags);

CREATE TABLE reading_editions (
    id          BIGINT PRIMARY KEY,
    series_id   BIGINT NOT NULL REFERENCES reading_series (id) ON DELETE CASCADE,
    -- BCP 47 language tag ("en", "ja", "pt-BR").
    language    TEXT NOT NULL CHECK (char_length(language) BETWEEN 2 AND 16),
    -- "Official English", "Scanlation X".
    label       TEXT NOT NULL CHECK (char_length(label) BETWEEN 1 AND 120),
    is_default  BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (series_id, language, label)
);
-- At most one default edition per series.
CREATE UNIQUE INDEX reading_editions_default_idx ON reading_editions (series_id) WHERE is_default;

CREATE TABLE reading_volumes (
    id          BIGINT PRIMARY KEY,
    edition_id  BIGINT NOT NULL REFERENCES reading_editions (id) ON DELETE CASCADE,
    number      NUMERIC(8, 2) NOT NULL CHECK (number >= 0),
    title       TEXT,
    cover_key   TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (edition_id, number)
);

CREATE TABLE reading_chapters (
    id            BIGINT PRIMARY KEY,
    -- Denormalised from the edition for the updates feed and visibility checks.
    series_id     BIGINT NOT NULL REFERENCES reading_series (id) ON DELETE CASCADE,
    edition_id    BIGINT NOT NULL REFERENCES reading_editions (id) ON DELETE CASCADE,
    volume_id     BIGINT REFERENCES reading_volumes (id) ON DELETE SET NULL,
    number        NUMERIC(8, 2) NOT NULL CHECK (number >= 0),
    title         TEXT,
    page_count    INTEGER NOT NULL DEFAULT 0 CHECK (page_count >= 0),
    format        TEXT NOT NULL DEFAULT 'images' CHECK (format IN ('images', 'pdf', 'epub')),
    -- Storage key of the original file for pdf/epub chapters.
    source_key    TEXT,
    release_group TEXT,
    published     BOOLEAN NOT NULL DEFAULT FALSE,
    published_at  TIMESTAMPTZ,
    created_by    BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (edition_id, number)
);
CREATE INDEX reading_chapters_series_idx ON reading_chapters (series_id, number);
CREATE INDEX reading_chapters_feed_idx ON reading_chapters (published_at DESC, id DESC) WHERE published;

CREATE TABLE reading_pages (
    chapter_id       BIGINT NOT NULL REFERENCES reading_chapters (id) ON DELETE CASCADE,
    idx              INTEGER NOT NULL CHECK (idx >= 0),
    -- Full-size WebP object key.
    key              TEXT NOT NULL,
    -- Sized variants: {"720": key, "1080": key, "1600": key}.
    variants         JSONB NOT NULL DEFAULT '{}'::jsonb,
    width            INTEGER NOT NULL CHECK (width > 0),
    height           INTEGER NOT NULL CHECK (height > 0),
    is_double_spread BOOLEAN NOT NULL DEFAULT FALSE,
    blurhash         TEXT,
    PRIMARY KEY (chapter_id, idx) DEFERRABLE INITIALLY IMMEDIATE
);
