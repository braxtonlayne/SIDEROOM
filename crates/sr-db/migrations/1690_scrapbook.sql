-- PRF-SCR-001 (wave 34): the scrapbook vault.
--
-- A message that collects enough of a trigger reaction, or that a permitted member sends by hand, is copied
-- into the server's own bucket under `scrapbook/<yyyy>/<mm>/<message_id>/` and described by a row here.
--
-- * `scrapbook_targets`: the Guild Shelf the module writes to. The module acts as a service account whose only
--   right is `upload` on this one mount; `granted_by` is the manager who delegated it. The grant is re-checked
--   at every copy (`sr_storage::scopes::may_upload` for that member), so it ends when the mount stops being the
--   server's, when the mount is switched off or delisted, or when the grantor loses `upload`.
-- * `scrapbook_entries`: one row per archived message. `author_id` and `archived_by` are plain ids, not foreign
--   keys: erasing an account deletes the entries about it through `scrapbook_legal::erase_for`, and that path
--   must queue the bucket removals first, which a cascade would skip.
-- * `scrapbook_objects`: the copies in the bucket (mount-relative keys, never URLs). Deleted with their entry.
-- * `scrapbook_removals`: every object that has to leave a bucket. Rows are written in the same transaction
--   that deletes the entry, so a crash between "entry gone" and "object gone" cannot lose the work. A removal
--   that fails stays here with its error, its attempt count and the next try; managers see the list
--   (`GET /guilds/{id}/scrapbook/removals`) and nothing about a failed cascade is silent.

CREATE TABLE scrapbook_targets (
    guild_id   BIGINT PRIMARY KEY REFERENCES guilds (id) ON DELETE CASCADE,
    mount_id   BIGINT NOT NULL REFERENCES storage_mounts (id) ON DELETE CASCADE,
    granted_by BIGINT NOT NULL,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE scrapbook_entries (
    id          BIGINT PRIMARY KEY,
    guild_id    BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    channel_id  BIGINT NOT NULL,
    message_id  BIGINT NOT NULL,
    author_id   BIGINT NOT NULL,
    author_name TEXT NOT NULL DEFAULT '' CHECK (char_length(author_name) <= 100),
    content     TEXT NOT NULL DEFAULT '' CHECK (char_length(content) <= 4000),
    sent_at     TIMESTAMPTZ NOT NULL,
    archived_by BIGINT,
    via         TEXT NOT NULL CHECK (via IN ('reaction', 'manual')),
    tags        TEXT[] NOT NULL DEFAULT '{}',
    mount_id    BIGINT NOT NULL,
    skipped     INTEGER NOT NULL DEFAULT 0 CHECK (skipped >= 0),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (guild_id, message_id)
);
CREATE INDEX scrapbook_entries_gallery_idx ON scrapbook_entries (guild_id, id DESC);
CREATE INDEX scrapbook_entries_channel_idx ON scrapbook_entries (channel_id);
CREATE INDEX scrapbook_entries_message_idx ON scrapbook_entries (message_id);
CREATE INDEX scrapbook_entries_author_idx ON scrapbook_entries (author_id);
CREATE INDEX scrapbook_entries_tags_idx ON scrapbook_entries USING gin (tags);

CREATE TABLE scrapbook_objects (
    id           BIGINT PRIMARY KEY,
    entry_id     BIGINT NOT NULL REFERENCES scrapbook_entries (id) ON DELETE CASCADE,
    position     INTEGER NOT NULL CHECK (position >= 0),
    mount_id     BIGINT NOT NULL,
    key          TEXT NOT NULL CHECK (char_length(key) BETWEEN 1 AND 1024),
    filename     TEXT NOT NULL CHECK (char_length(filename) <= 255),
    content_type TEXT NOT NULL DEFAULT '' CHECK (char_length(content_type) <= 255),
    size_bytes   BIGINT NOT NULL CHECK (size_bytes >= 0),
    kind         TEXT NOT NULL CHECK (kind IN ('image', 'video', 'audio', 'file')),
    UNIQUE (entry_id, position)
);

CREATE TABLE scrapbook_removals (
    id              BIGINT PRIMARY KEY,
    guild_id        BIGINT NOT NULL,
    mount_id        BIGINT NOT NULL,
    key             TEXT NOT NULL CHECK (char_length(key) BETWEEN 1 AND 1024),
    message_id      BIGINT,
    state           TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'failed')),
    attempts        INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    last_error      TEXT NOT NULL DEFAULT '' CHECK (char_length(last_error) <= 500),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (mount_id, key)
);
CREATE INDEX scrapbook_removals_due_idx ON scrapbook_removals (next_attempt_at);
CREATE INDEX scrapbook_removals_guild_idx ON scrapbook_removals (guild_id, state);
