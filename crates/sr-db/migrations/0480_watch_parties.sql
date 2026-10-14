-- 0480: Watch Parties (docs/specs/watch-parties.md, WP-BE-001).
-- A party is a server-authoritative playback state for a group, bound to a channel (text, voice or DM)
-- or private (shareable link). Title ids carry no foreign keys (the catalog is a separate crate).
-- Playback state is one row guarded by `version`; every command bumps it inside a row lock.

CREATE TABLE watch_parties (
    id                  BIGINT PRIMARY KEY,
    host_id             BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    -- 'channel' (guild text/voice), 'dm' (DM or group DM) or 'private' (link only).
    binding             TEXT NOT NULL CHECK (binding IN ('channel', 'dm', 'private')),
    guild_id            BIGINT REFERENCES guilds (id) ON DELETE CASCADE,
    channel_id          BIGINT REFERENCES channels (id) ON DELETE CASCADE,
    invite_code         TEXT UNIQUE,
    -- What is playing. `episode_id = 0` is a movie. `asset_id` is the resolved video asset (the viewer
    -- still obtains HLS through the normal play checks).
    title_id            BIGINT NOT NULL CHECK (title_id > 0),
    episode_id          BIGINT NOT NULL DEFAULT 0 CHECK (episode_id >= 0),
    variant_id          BIGINT,
    asset_id            BIGINT NOT NULL CHECK (asset_id > 0),
    title_name          TEXT NOT NULL,
    episode_label       TEXT,
    duration_ms         BIGINT CHECK (duration_ms IS NULL OR duration_ms > 0),
    age_restricted      BOOLEAN NOT NULL DEFAULT FALSE,
    -- A live channel: no seeking, everybody follows the live edge.
    live                BOOLEAN NOT NULL DEFAULT FALSE,
    -- Playback state.
    playing             BOOLEAN NOT NULL DEFAULT FALSE,
    position_ms         BIGINT NOT NULL DEFAULT 0 CHECK (position_ms >= 0),
    -- Playback speed in thousandths (1000 = 1.0x).
    rate_milli          INTEGER NOT NULL DEFAULT 1000 CHECK (rate_milli BETWEEN 500 AND 2000),
    -- Server clock (unix ms) at which `position_ms` was true.
    at_ms               BIGINT NOT NULL,
    version             BIGINT NOT NULL DEFAULT 1,
    -- Paused by the buffering rule rather than by a controller; `buffer_wait_until_ms` ends the wait.
    auto_paused         BOOLEAN NOT NULL DEFAULT FALSE,
    buffer_wait_until_ms BIGINT,
    -- Settings.
    shared_control      TEXT NOT NULL DEFAULT 'host' CHECK (shared_control IN ('host', 'controllers', 'everyone')),
    queue_policy        TEXT NOT NULL DEFAULT 'controllers' CHECK (queue_policy IN ('host', 'controllers', 'everyone')),
    auto_pause_buffering BOOLEAN NOT NULL DEFAULT TRUE,
    max_wait_ms         INTEGER NOT NULL DEFAULT 10000 CHECK (max_wait_ms BETWEEN 1000 AND 60000),
    -- A bound voice-channel party that outlives its members (the Watch Party channel behaviour).
    persistent          BOOLEAN NOT NULL DEFAULT FALSE,
    max_participants    INTEGER NOT NULL DEFAULT 50 CHECK (max_participants BETWEEN 2 AND 100),
    -- Lifecycle.
    created_at_ms       BIGINT NOT NULL,
    updated_at_ms       BIGINT NOT NULL,
    empty_since_ms      BIGINT,
    ended_at_ms         BIGINT,
    ended_reason        TEXT
);
-- One live party per bound channel.
CREATE UNIQUE INDEX watch_parties_channel_idx ON watch_parties (channel_id)
    WHERE ended_at_ms IS NULL AND channel_id IS NOT NULL;
CREATE INDEX watch_parties_guild_idx ON watch_parties (guild_id) WHERE ended_at_ms IS NULL;
CREATE INDEX watch_parties_host_idx ON watch_parties (host_id) WHERE ended_at_ms IS NULL;
CREATE INDEX watch_parties_idle_idx ON watch_parties (empty_since_ms)
    WHERE ended_at_ms IS NULL AND empty_since_ms IS NOT NULL;

CREATE TABLE watch_party_members (
    party_id            BIGINT NOT NULL REFERENCES watch_parties (id) ON DELETE CASCADE,
    user_id             BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    joined_at_ms        BIGINT NOT NULL,
    -- Set by the host: may control playback in 'controllers' mode.
    is_controller       BOOLEAN NOT NULL DEFAULT FALSE,
    ready               BOOLEAN NOT NULL DEFAULT FALSE,
    -- Buffering report: since when this member has been stalled (NULL = playing fine).
    buffering_since_ms  BIGINT,
    last_seen_ms        BIGINT NOT NULL,
    rtt_ms              INTEGER,
    PRIMARY KEY (party_id, user_id)
);
CREATE INDEX watch_party_members_user_idx ON watch_party_members (user_id);

-- Removed members that may not return (kick), per party.
CREATE TABLE watch_party_bans (
    party_id    BIGINT NOT NULL REFERENCES watch_parties (id) ON DELETE CASCADE,
    user_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    banned_at_ms BIGINT NOT NULL,
    PRIMARY KEY (party_id, user_id)
);

CREATE TABLE watch_party_queue (
    id          BIGINT PRIMARY KEY,
    party_id    BIGINT NOT NULL REFERENCES watch_parties (id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    title_id    BIGINT NOT NULL CHECK (title_id > 0),
    episode_id  BIGINT NOT NULL DEFAULT 0 CHECK (episode_id >= 0),
    variant_id  BIGINT,
    asset_id    BIGINT NOT NULL,
    title_name  TEXT NOT NULL,
    episode_label TEXT,
    duration_ms BIGINT,
    age_restricted BOOLEAN NOT NULL DEFAULT FALSE,
    added_by    BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    added_at_ms BIGINT NOT NULL
);
CREATE INDEX watch_party_queue_idx ON watch_party_queue (party_id, position);

-- Audit trail of party-level actions (create, end, host handover, kick, setting and title changes).
-- Playback commands are not audited (they would flood the table); they are rate limited instead.
CREATE TABLE watch_party_audit (
    id          BIGINT PRIMARY KEY,
    party_id    BIGINT NOT NULL REFERENCES watch_parties (id) ON DELETE CASCADE,
    actor_id    BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    action      TEXT NOT NULL,
    target_id   BIGINT,
    detail      JSONB NOT NULL DEFAULT '{}'::jsonb,
    at_ms       BIGINT NOT NULL
);
CREATE INDEX watch_party_audit_idx ON watch_party_audit (party_id, at_ms DESC);
