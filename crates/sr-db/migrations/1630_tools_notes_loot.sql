-- UTL-DEV-001 (wave 32): the encrypted scratchpad and the loot tables of the utility drawer.
--
-- * `scratch_notes` (the roadmap says `user_notes`, but that name is the profile notes on other people since
--   migration 0071): one row per note, **ciphertext only**. The browser encrypts every note with a key that
--   never leaves it (AES-GCM, a non-extractable WebCrypto key in IndexedDB; a passphrase-wrapped copy is the
--   member's own export). The server stores opaque bytes: it cannot read a note, and neither can staff, so
--   there is no staff read path, no access-log row to write and nothing to refuse politely: the only queries
--   on this table bind `user_id` to the signed-in member. 64 KiB per note, enforced here and in the API.
--   `revision` is bumped on every write so two browsers editing the same note see a conflict, not a lost edit.
-- * `loot_tables`: weighted rows a member rolls on, saved for the account (`owner_user_id`) or for a server
--   (`owner_guild_id`, written with MANAGE_GUILD, read by every member). Plain text on purpose: a server's
--   table is shared with its members.

CREATE TABLE scratch_notes (
    user_id    BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    note_id    TEXT NOT NULL CHECK (note_id ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'),
    ciphertext BYTEA NOT NULL CHECK (octet_length(ciphertext) BETWEEN 1 AND 65536),
    revision   BIGINT NOT NULL DEFAULT 1 CHECK (revision >= 1),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, note_id)
);
CREATE INDEX scratch_notes_updated_idx ON scratch_notes (user_id, updated_at DESC);

COMMENT ON TABLE scratch_notes IS 'Client-side encrypted notes (UTL-DEV-001). Ciphertext only; never read by anyone but the owner.';

CREATE TABLE loot_tables (
    id             BIGINT PRIMARY KEY,
    owner_user_id  BIGINT REFERENCES accounts (id) ON DELETE CASCADE,
    owner_guild_id BIGINT REFERENCES guilds (id) ON DELETE CASCADE,
    name           TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 80),
    rows           JSONB NOT NULL CHECK (jsonb_typeof(rows) = 'array' AND jsonb_array_length(rows) BETWEEN 1 AND 200),
    created_by     BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((owner_user_id IS NOT NULL) <> (owner_guild_id IS NOT NULL))
);
CREATE INDEX loot_tables_user_idx ON loot_tables (owner_user_id, id) WHERE owner_user_id IS NOT NULL;
CREATE INDEX loot_tables_guild_idx ON loot_tables (owner_guild_id, id) WHERE owner_guild_id IS NOT NULL;
CREATE UNIQUE INDEX loot_tables_user_name ON loot_tables (owner_user_id, lower(name)) WHERE owner_user_id IS NOT NULL;
CREATE UNIQUE INDEX loot_tables_guild_name ON loot_tables (owner_guild_id, lower(name)) WHERE owner_guild_id IS NOT NULL;
