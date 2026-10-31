-- 1752: BRG-SYN-001, the sync ops queue and the plans (dry runs).
--
-- `bridge_sync_ops`: "look at object X" (design section 11.2). An op carries no payload of the change: the
-- payload is computed when the op runs, from the current state of both sides, so the newest state always wins
-- and nothing stale is sent. One PENDING row per (guild, object kind, object id, direction): a burst of edits to
-- one object is one row (the enqueue upserts and pushes `due_at` out by the debounce). `seed_run`/`step` make the
-- seed job resumable: the ops of a seed are rows too. `priority` 0 is a revert (1 s debounce), 1 is structure
-- (2 s). After 8 attempts a row is 'dead' (a dead-op conflict shows it; "Retry" requeues it).
--
-- `bridge_sync_plans`: the dry run an owner reviews before whole_guild is switched on. At most one open plan
-- (building or ready) per server; it expires after 15 minutes. `sr_hash` and `discord_hash` are the hashes of
-- both sides' state when the plan was built: confirming recomputes them and refuses a stale plan (409).
CREATE TABLE bridge_sync_ops (
    id            BIGINT PRIMARY KEY,
    guild_id      BIGINT NOT NULL REFERENCES bridges (guild_id) ON DELETE CASCADE,
    object_kind   TEXT NOT NULL CHECK (object_kind IN
                    ('role', 'role_order', 'channel', 'channel_order', 'overwrites', 'thread', 'forum_tags', 'reconcile')),
    -- The SIDE ROOM id of the object (a Discord id for an op on an object that has no SIDE ROOM twin yet).
    object_id     BIGINT NOT NULL,
    direction     TEXT NOT NULL CHECK (direction IN ('to_discord', 'to_sr', 'look')),
    state         TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'running', 'done', 'dead')),
    seed_run      BIGINT,
    step          SMALLINT NOT NULL DEFAULT 0 CHECK (step >= 0),
    priority      SMALLINT NOT NULL DEFAULT 1 CHECK (priority BETWEEN 0 AND 2),
    attempts      INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    due_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_error    TEXT CHECK (char_length(last_error) <= 500),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX bridge_sync_ops_pending_once
    ON bridge_sync_ops (guild_id, object_kind, object_id, direction) WHERE state = 'pending';
CREATE INDEX bridge_sync_ops_due_idx ON bridge_sync_ops (priority, due_at) WHERE state = 'pending';
CREATE INDEX bridge_sync_ops_dead_idx ON bridge_sync_ops (guild_id) WHERE state = 'dead';
CREATE INDEX bridge_sync_ops_seed_idx ON bridge_sync_ops (seed_run, step) WHERE seed_run IS NOT NULL;

CREATE TABLE bridge_sync_plans (
    id              BIGINT PRIMARY KEY,
    guild_id        BIGINT NOT NULL REFERENCES bridges (guild_id) ON DELETE CASCADE,
    mode            TEXT NOT NULL CHECK (mode IN ('selective', 'whole_guild')),
    seed_structure  TEXT NOT NULL DEFAULT 'sr' CHECK (seed_structure IN ('sr', 'discord')),
    status          TEXT NOT NULL DEFAULT 'building'
                    CHECK (status IN ('building', 'ready', 'failed', 'confirmed', 'expired', 'superseded')),
    -- The diff (roles, categories, channels, threads, overwrites, not mirrored) and the owner's edits to it.
    plan            JSONB NOT NULL DEFAULT '{}'::jsonb,
    blockers        JSONB NOT NULL DEFAULT '[]'::jsonb,
    sr_hash         TEXT NOT NULL DEFAULT '' CHECK (char_length(sr_hash) <= 128),
    discord_hash    TEXT NOT NULL DEFAULT '' CHECK (char_length(discord_hash) <= 128),
    plan_hash       TEXT NOT NULL DEFAULT '' CHECK (char_length(plan_hash) <= 128),
    created_by      BIGINT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at      TIMESTAMPTZ NOT NULL,
    confirmed_at    TIMESTAMPTZ
);
CREATE UNIQUE INDEX bridge_sync_plans_open_once ON bridge_sync_plans (guild_id) WHERE status IN ('building', 'ready');
