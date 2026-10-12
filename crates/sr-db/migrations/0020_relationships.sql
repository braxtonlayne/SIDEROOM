-- 0020: relationships (friends, requests, blocks, ignores) and social privacy. docs/04 section 4, LNG-FRD-001.
-- One row per (user, target). A friendship is two 'friend' rows; a request is
-- 'pending_outgoing' on the sender and 'pending_incoming' on the receiver.
-- Codes follow Discord: 1 friend, 2 blocked, 3 incoming, 4 outgoing; 6 = ignored (SIDE ROOM).
CREATE TABLE relationships (
    user_id     BIGINT NOT NULL REFERENCES accounts (id),
    target_id   BIGINT NOT NULL REFERENCES accounts (id),
    type        SMALLINT NOT NULL CHECK (type IN (1, 2, 3, 4, 6)),
    nickname    TEXT CHECK (nickname IS NULL OR char_length(nickname) BETWEEN 1 AND 32),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, target_id),
    CHECK (user_id <> target_id),
    CHECK (nickname IS NULL OR type = 1)
);
-- Reverse lookups (who has blocked / requested me) and per-type counts.
CREATE INDEX relationships_target_idx ON relationships (target_id, type);
CREATE INDEX relationships_user_type_idx ON relationships (user_id, type);

-- Who may friend-request or DM this user (LNG-FRD-004, LNG-USET-013/014).
CREATE TABLE user_privacy (
    user_id                  BIGINT PRIMARY KEY REFERENCES accounts (id),
    fr_everyone              BOOLEAN NOT NULL DEFAULT TRUE,
    fr_friends_of_friends    BOOLEAN NOT NULL DEFAULT TRUE,
    fr_server_members        BOOLEAN NOT NULL DEFAULT TRUE,
    dm_server_members        BOOLEAN NOT NULL DEFAULT TRUE,
    dm_message_requests      BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT now()
);
