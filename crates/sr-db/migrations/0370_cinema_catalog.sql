-- 0370: Cinema catalog. docs/04 section 9, CIN-DATA-001 (the catalog side).
-- Ids are application-generated Snowflakes. Tables carry the `video_` prefix so they cannot clash
-- with future generic names (`seasons`, `episodes`, `markers`, `collections`).
--
-- Out of scope here, owned by other work: the playable asset tables (assets, renditions, audio and
-- subtitle tracks, transcode jobs, workers: p6-cinema-ingest) and everything a viewer does (watch
-- progress, lists, ratings, reviews, comments: p6-cinema-social). Those use plain `title_id` /
-- `episode_id` / `asset_id` columns WITHOUT foreign keys to this schema, and the `asset_id` columns
-- below are likewise plain references (nullable) to the asset a title / episode / variant plays.
--
-- Visibility: a title is public when `published` and not merged away; an episode when it and its
-- title are published. Age-restricted titles are visible to everyone (D-184); only opening them
-- needs `accounts.age_confirmed_at` (checked by the service, not by SQL).

CREATE TABLE video_titles (
    id                  BIGINT PRIMARY KEY,
    -- `anime` is episodic when it has seasons, otherwise a single film.
    kind                TEXT NOT NULL CHECK (kind IN ('movie', 'series', 'anime')),
    title               TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 300),
    original_title      TEXT CHECK (char_length(original_title) BETWEEN 1 AND 300),
    alt_titles          TEXT[] NOT NULL DEFAULT '{}',
    -- Lower-cased title without a leading article, for the title sort.
    sort_title          TEXT NOT NULL,
    year                SMALLINT CHECK (year BETWEEN 1880 AND 2200),
    -- Minutes; the runtime of a film or the typical episode runtime of a series.
    runtime_min         INTEGER CHECK (runtime_min BETWEEN 1 AND 6000),
    content_rating      TEXT CHECK (char_length(content_rating) BETWEEN 1 AND 16),
    synopsis            TEXT NOT NULL DEFAULT '',
    tagline             TEXT CHECK (char_length(tagline) BETWEEN 1 AND 300),
    genres              TEXT[] NOT NULL DEFAULT '{}',
    tags                TEXT[] NOT NULL DEFAULT '{}',
    -- Audio languages and subtitle languages offered (BCP 47), as catalog text.
    languages           TEXT[] NOT NULL DEFAULT '{}',
    subtitle_languages  TEXT[] NOT NULL DEFAULT '{}',
    -- People and credits are free text lists (D-089), no person table.
    cast_list           TEXT[] NOT NULL DEFAULT '{}',
    directors           TEXT[] NOT NULL DEFAULT '{}',
    writers             TEXT[] NOT NULL DEFAULT '{}',
    studios             TEXT[] NOT NULL DEFAULT '{}',
    -- Other crew as "Role: Name" lines.
    crew                TEXT[] NOT NULL DEFAULT '{}',
    age_restricted      BOOLEAN NOT NULL DEFAULT FALSE,
    poster_key          TEXT,
    backdrop_key        TEXT,
    logo_key            TEXT,
    external_ids        JSONB NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(external_ids) = 'object'),
    -- The playable asset of a film (plain reference, no foreign key).
    asset_id            BIGINT,
    published           BOOLEAN NOT NULL DEFAULT FALSE,
    published_at        TIMESTAMPTZ,
    -- Set when this title was merged into another; readers are redirected to it.
    merged_into         BIGINT REFERENCES video_titles (id) ON DELETE SET NULL,
    -- Denormalised from the published episodes, refreshed in the transaction that changes them.
    season_count        INTEGER NOT NULL DEFAULT 0 CHECK (season_count >= 0),
    episode_count       INTEGER NOT NULL DEFAULT 0 CHECK (episode_count >= 0),
    -- The later of the publication and the newest published episode.
    last_content_at     TIMESTAMPTZ,
    created_by          BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- Browse orderings over the visible (published, not merged) set.
CREATE INDEX video_titles_updated_idx ON video_titles (kind, last_content_at DESC, id DESC)
    WHERE published AND merged_into IS NULL;
CREATE INDEX video_titles_added_idx ON video_titles (published_at DESC, id DESC)
    WHERE published AND merged_into IS NULL;
CREATE INDEX video_titles_sort_idx ON video_titles (sort_title, id)
    WHERE published AND merged_into IS NULL;
CREATE INDEX video_titles_genres_idx ON video_titles USING GIN (genres);
CREATE INDEX video_titles_tags_idx ON video_titles USING GIN (tags);
CREATE INDEX video_titles_asset_idx ON video_titles (asset_id) WHERE asset_id IS NOT NULL;

CREATE TABLE video_seasons (
    id          BIGINT PRIMARY KEY,
    title_id    BIGINT NOT NULL REFERENCES video_titles (id) ON DELETE CASCADE,
    -- 0 is the specials season.
    number      INTEGER NOT NULL CHECK (number BETWEEN 0 AND 999),
    name        TEXT CHECK (char_length(name) BETWEEN 1 AND 200),
    synopsis    TEXT,
    poster_key  TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (title_id, number)
);

CREATE TABLE video_episodes (
    id               BIGINT PRIMARY KEY,
    -- Denormalised from the season for visibility checks and the episode lists.
    title_id         BIGINT NOT NULL REFERENCES video_titles (id) ON DELETE CASCADE,
    season_id        BIGINT NOT NULL REFERENCES video_seasons (id) ON DELETE CASCADE,
    number           INTEGER NOT NULL CHECK (number BETWEEN 0 AND 9999),
    -- Running number across seasons (anime), when known.
    absolute_number  INTEGER CHECK (absolute_number BETWEEN 0 AND 99999),
    title            TEXT NOT NULL DEFAULT '' CHECK (char_length(title) <= 300),
    synopsis         TEXT NOT NULL DEFAULT '',
    air_date         DATE,
    runtime_min      INTEGER CHECK (runtime_min BETWEEN 1 AND 6000),
    still_key        TEXT,
    -- The playable asset of this episode (plain reference, no foreign key).
    asset_id         BIGINT,
    published        BOOLEAN NOT NULL DEFAULT FALSE,
    published_at     TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (season_id, number)
);
CREATE INDEX video_episodes_title_idx ON video_episodes (title_id, season_id, number);
CREATE INDEX video_episodes_asset_idx ON video_episodes (asset_id) WHERE asset_id IS NOT NULL;

-- Other cuts of a title ("Director's Cut", "Extended"). The title row itself is the default cut.
CREATE TABLE video_title_variants (
    id           BIGINT PRIMARY KEY,
    title_id     BIGINT NOT NULL REFERENCES video_titles (id) ON DELETE CASCADE,
    label        TEXT NOT NULL CHECK (char_length(label) BETWEEN 1 AND 120),
    notes        TEXT CHECK (char_length(notes) <= 1000),
    runtime_min  INTEGER CHECK (runtime_min BETWEEN 1 AND 6000),
    asset_id     BIGINT,
    position     INTEGER NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (title_id, label)
);

CREATE TABLE video_trailers (
    id           BIGINT PRIMARY KEY,
    title_id     BIGINT NOT NULL REFERENCES video_titles (id) ON DELETE CASCADE,
    label        TEXT NOT NULL CHECK (char_length(label) BETWEEN 1 AND 120),
    kind         TEXT NOT NULL DEFAULT 'trailer' CHECK (kind IN ('trailer', 'teaser', 'clip')),
    language     TEXT CHECK (char_length(language) BETWEEN 2 AND 16),
    -- Hosted trailer. A trailer without an asset is not shown to members.
    asset_id     BIGINT,
    position     INTEGER NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX video_trailers_title_idx ON video_trailers (title_id, position, id);

-- Skip intro / skip credits / recap windows for a title (films) or an episode.
CREATE TABLE video_markers (
    id          BIGINT PRIMARY KEY,
    title_id    BIGINT NOT NULL REFERENCES video_titles (id) ON DELETE CASCADE,
    episode_id  BIGINT REFERENCES video_episodes (id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('intro', 'credits', 'recap')),
    start_ms    INTEGER NOT NULL CHECK (start_ms >= 0),
    end_ms      INTEGER NOT NULL,
    -- 'manual' (reviewers) or 'auto' (a later detector).
    source      TEXT NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'auto')),
    created_by  BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (end_ms > start_ms)
);
-- One window of each kind per title (episode_id NULL) or per episode.
CREATE UNIQUE INDEX video_markers_one_idx ON video_markers (title_id, COALESCE(episode_id, 0), kind);
CREATE INDEX video_markers_episode_idx ON video_markers (episode_id) WHERE episode_id IS NOT NULL;

-- Curated rows, spotlights and banners, placed on the Discover / Movies / Series / Anime pages.
CREATE TABLE video_collections (
    id          BIGINT PRIMARY KEY,
    slug        TEXT NOT NULL UNIQUE CHECK (slug ~ '^[a-z0-9][a-z0-9-]{0,62}$'),
    name        TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 120),
    eyebrow     TEXT CHECK (char_length(eyebrow) BETWEEN 1 AND 120),
    blurb       TEXT CHECK (char_length(blurb) <= 1000),
    display     TEXT NOT NULL DEFAULT 'row' CHECK (display IN ('row', 'hero', 'banner')),
    surfaces    TEXT[] NOT NULL DEFAULT '{}'
                CHECK (surfaces <@ ARRAY['discover', 'movies', 'series', 'anime']::text[]),
    position    INTEGER NOT NULL DEFAULT 0,
    published   BOOLEAN NOT NULL DEFAULT FALSE,
    created_by  BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX video_collections_surfaces_idx ON video_collections USING GIN (surfaces);

CREATE TABLE video_collection_items (
    collection_id  BIGINT NOT NULL REFERENCES video_collections (id) ON DELETE CASCADE,
    title_id       BIGINT NOT NULL REFERENCES video_titles (id) ON DELETE CASCADE,
    position       INTEGER NOT NULL,
    PRIMARY KEY (collection_id, title_id)
);
CREATE INDEX video_collection_items_pos_idx ON video_collection_items (collection_id, position);
CREATE INDEX video_collection_items_title_idx ON video_collection_items (title_id);
