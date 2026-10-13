-- 0110: threads and forum posts. docs/04 `thread_metadata`, `thread_members`; LNG-THR-001,
-- LNG-FRM-001. A thread is a `channels` row of type 10/11/12 whose parent_id is the
-- channel it lives in (a forum or media channel for posts).
-- Deleting a parent deletes its threads in the repository (channels.parent_id is SET NULL).

CREATE TABLE thread_metadata (
    channel_id            BIGINT PRIMARY KEY REFERENCES channels (id) ON DELETE CASCADE,
    archived              BOOLEAN NOT NULL DEFAULT FALSE,
    -- Minutes of inactivity before the auto-archive job closes the thread.
    auto_archive_duration INTEGER NOT NULL DEFAULT 1440
                          CHECK (auto_archive_duration IN (60, 1440, 4320, 10080)),
    -- When `archived` last changed.
    archive_timestamp     TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked                BOOLEAN NOT NULL DEFAULT FALSE,
    invitable             BOOLEAN NOT NULL DEFAULT TRUE,
    create_timestamp      TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Last message or unarchive: the auto-archive clock runs from here.
    last_activity_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Messages minus deletions (reported capped at 50) and every message ever sent.
    message_count         INTEGER NOT NULL DEFAULT 0 CHECK (message_count >= 0),
    total_message_sent    INTEGER NOT NULL DEFAULT 0 CHECK (total_message_sent >= 0),
    -- Forum tag ids applied to a post.
    applied_tags          BIGINT[] NOT NULL DEFAULT '{}'
);
-- The auto-archive job scans active threads by idle time; listings scan archives.
CREATE INDEX thread_metadata_active_idx ON thread_metadata (last_activity_at) WHERE NOT archived;
CREATE INDEX thread_metadata_archived_idx ON thread_metadata (archive_timestamp DESC) WHERE archived;

CREATE TABLE thread_members (
    channel_id  BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    user_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    joined_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    flags       INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (channel_id, user_id)
);
CREATE INDEX thread_members_user_idx ON thread_members (user_id);

-- Active-thread listings and GUILD_CREATE read threads per guild.
CREATE INDEX channels_threads_guild_idx ON channels (guild_id) WHERE type IN (10, 11, 12);
