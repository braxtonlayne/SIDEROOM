-- 0620: lounge servers backend gaps (phase 10; LNG-GLD-008/011/012/013, LNG-INV-004/005,
-- LNG-CHN-004).
--
-- * Server tag (LNG-GLD-011): a short tag and a badge on the guild; a member opts in by
--   choosing one guild as their primary one (`accounts.primary_guild_id`).
-- * Vanity URL (LNG-INV-004): `guilds.vanity_code` already exists (0010); this adds its
--   shape check and a use counter.
-- * Temporary membership (LNG-INV-005): `guild_members.temporary` is set when the member
--   joined through a temporary invite; it is dropped when they leave without a role.
-- * Membership screening (LNG-GLD-008): rules shown before a new member may talk.
-- * Server Directory (LNG-GLD-013): listing requests and the staff decision (D-012).
-- * Per-server profile (LNG-GLD-012): `guild_members.avatar_key/banner_key/bio` exist
--   since 0010; nothing to add.

ALTER TABLE guilds
    ADD COLUMN tag        TEXT CHECK (tag IS NULL OR tag ~ '^[0-9A-Za-z]{2,4}$'),
    ADD COLUMN tag_badge  TEXT CHECK (tag_badge IS NULL OR tag_badge ~ '^[a-z][a-z0-9-]{1,31}$'),
    ADD COLUMN vanity_uses INTEGER NOT NULL DEFAULT 0 CHECK (vanity_uses >= 0),
    ADD CONSTRAINT guilds_vanity_code_shape
        CHECK (vanity_code IS NULL OR vanity_code::text ~ '^[0-9A-Za-z-]{3,32}$');

ALTER TABLE accounts
    ADD COLUMN primary_guild_id      BIGINT REFERENCES guilds (id) ON DELETE SET NULL,
    ADD COLUMN primary_guild_enabled BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE guild_members
    ADD COLUMN temporary BOOLEAN NOT NULL DEFAULT FALSE;
CREATE INDEX guild_members_temporary_idx ON guild_members (user_id) WHERE temporary;

ALTER TABLE guild_community
    ADD COLUMN screening_enabled     BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN screening_description TEXT CHECK (char_length(screening_description) <= 300),
    -- serde form of Vec<String>: the rules a new member agrees to (at most 16).
    ADD COLUMN screening_rules       JSONB NOT NULL DEFAULT '[]'::jsonb
        CHECK (jsonb_array_length(screening_rules) <= 16);

CREATE TABLE guild_directory (
    guild_id       BIGINT PRIMARY KEY REFERENCES guilds (id) ON DELETE CASCADE,
    status         TEXT NOT NULL CHECK (status IN ('pending', 'approved', 'rejected', 'removed')),
    category       TEXT NOT NULL CHECK (category ~ '^[a-z][a-z-]{1,31}$'),
    description    TEXT NOT NULL DEFAULT '' CHECK (char_length(description) <= 300),
    featured       BOOLEAN NOT NULL DEFAULT FALSE,
    requested_by   BIGINT NOT NULL REFERENCES accounts (id),
    requested_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    decided_by     BIGINT REFERENCES accounts (id),
    decided_at     TIMESTAMPTZ,
    decision_note  TEXT CHECK (char_length(decision_note) <= 500),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX guild_directory_status_idx ON guild_directory (status, category);
