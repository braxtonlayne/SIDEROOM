-- 2040: ARC-ROOM-001 (wave 42), the Arcade room.
--
-- * A game manifest (`sideroom.json`) declares a content tier and may carry creator tip links. The tier is one of
--   exactly two values; there is no tier for fan IP, and the CHECK is the same closed list the manifest validator
--   (`sr_activities::arcade`) enforces. A bundle uploaded before tiers has `tier IS NULL` and is not listed on
--   the Arcade. `tip_links` holds what `sr_core::tips` accepted: `[{"kind", "url"}]`.
-- * `arcade_featured`: the Operator's Front Page. One row per application; the Operator chooses, orders and
--   writes the card's one-line blurb. The action is audited (`arcade.feature` / `arcade.unfeature`).
-- * `arcade_coming_soon` and `arcade_coming_soon_notify`: the showcase of games in development, with the repo
--   link and each member's "Notify me" choice.
-- * `arcade_guild_cartridges`: unlisted games and mods a server lists from its shelf. They never appear on the
--   Front Page; only members of the server see them.
ALTER TABLE application_activity_bundles
    ADD COLUMN tier TEXT CHECK (tier IN ('TIER_1_ORIGINAL', 'TIER_2_REIMPLEMENTATION')),
    ADD COLUMN tip_links JSONB NOT NULL DEFAULT '[]'::jsonb;

CREATE TABLE arcade_featured (
    application_id BIGINT PRIMARY KEY REFERENCES applications (id) ON DELETE CASCADE,
    position       INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
    blurb          TEXT CHECK (char_length(blurb) <= 120),
    featured_by    BIGINT NOT NULL REFERENCES accounts (id),
    featured_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX arcade_featured_order_idx ON arcade_featured (position, featured_at);

CREATE TABLE arcade_coming_soon (
    id         BIGINT PRIMARY KEY,
    title      TEXT NOT NULL CHECK (char_length(title) BETWEEN 2 AND 60),
    summary    TEXT NOT NULL CHECK (char_length(summary) BETWEEN 1 AND 240),
    tier       TEXT NOT NULL CHECK (tier IN ('TIER_1_ORIGINAL', 'TIER_2_REIMPLEMENTATION')),
    repo_url   TEXT NOT NULL CHECK (char_length(repo_url) <= 200 AND repo_url LIKE 'https://%'),
    position   INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
    created_by BIGINT NOT NULL REFERENCES accounts (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE arcade_coming_soon_notify (
    coming_soon_id BIGINT NOT NULL REFERENCES arcade_coming_soon (id) ON DELETE CASCADE,
    user_id        BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (coming_soon_id, user_id)
);
CREATE INDEX arcade_coming_soon_notify_user_idx ON arcade_coming_soon_notify (user_id);

CREATE TABLE arcade_guild_cartridges (
    id           BIGINT PRIMARY KEY,
    guild_id     BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    title        TEXT NOT NULL CHECK (char_length(title) BETWEEN 2 AND 60),
    description  TEXT NOT NULL DEFAULT '' CHECK (char_length(description) <= 240),
    kind         TEXT NOT NULL CHECK (kind IN ('game', 'mod')),
    tier         TEXT NOT NULL CHECK (tier IN ('TIER_1_ORIGINAL', 'TIER_2_REIMPLEMENTATION')),
    -- A locator on the server's shelf, resolved by the shelf code (wave 41); never a URL the browser fetches.
    source_ref   TEXT NOT NULL CHECK (char_length(source_ref) BETWEEN 1 AND 300),
    capabilities TEXT[] NOT NULL DEFAULT '{}',
    tip_links    JSONB NOT NULL DEFAULT '[]'::jsonb,
    added_by     BIGINT NOT NULL REFERENCES accounts (id),
    added_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX arcade_guild_cartridges_guild_idx ON arcade_guild_cartridges (guild_id, added_at DESC);
