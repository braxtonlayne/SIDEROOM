-- 0012: messages (range-partitioned by id, monthly), edit history. docs/04 section 3, LNG-MSG-001.
-- Snowflake ids embed a millisecond timestamp, so range-partitioning by id is
-- partitioning by time. Primary keys on a partitioned table must include the
-- partition key, which `id` alone satisfies.
CREATE TYPE message_visibility AS ENUM ('visible', 'user_deleted', 'staff_removed');

CREATE TABLE messages (
    id                  BIGINT NOT NULL,
    channel_id          BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    guild_id            BIGINT,
    author_id           BIGINT NOT NULL,
    webhook_id          BIGINT,
    application_id      BIGINT,
    type                SMALLINT NOT NULL DEFAULT 0,
    content             TEXT NOT NULL DEFAULT '',
    content_ast         JSONB,
    flags               INTEGER NOT NULL DEFAULT 0,
    nonce               TEXT,
    edited_at           TIMESTAMPTZ,
    reference           JSONB,
    embeds              JSONB NOT NULL DEFAULT '[]'::jsonb,
    components          JSONB NOT NULL DEFAULT '[]'::jsonb,
    sticker_ids         BIGINT[] NOT NULL DEFAULT '{}',
    poll                JSONB,
    mention_everyone    BOOLEAN NOT NULL DEFAULT FALSE,
    mention_user_ids    BIGINT[] NOT NULL DEFAULT '{}',
    mention_role_ids    BIGINT[] NOT NULL DEFAULT '{}',
    mention_channel_ids BIGINT[] NOT NULL DEFAULT '{}',
    pinned              BOOLEAN NOT NULL DEFAULT FALSE,
    reaction_counts     JSONB NOT NULL DEFAULT '{}'::jsonb,
    visibility          message_visibility NOT NULL DEFAULT 'visible',
    deleted_at          TIMESTAMPTZ,
    purge_after         TIMESTAMPTZ,
    hold_id             BIGINT REFERENCES moderation_holds (id),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (id),
    CHECK ((visibility = 'visible') = (deleted_at IS NULL))
) PARTITION BY RANGE (id);

CREATE INDEX messages_channel_idx ON messages (channel_id, id DESC);
CREATE INDEX messages_channel_visible_idx ON messages (channel_id, id DESC) WHERE visibility = 'visible';
CREATE INDEX messages_purge_idx ON messages (purge_after) WHERE purge_after IS NOT NULL;
CREATE INDEX messages_hold_idx ON messages (hold_id) WHERE hold_id IS NOT NULL;
CREATE INDEX messages_author_idx ON messages (author_id, id DESC);

-- Smallest snowflake generated at `ts`. Epoch and shift mirror sr-core::snowflake
-- (SIDEROOM_EPOCH_MS = 1767225600000, 22 low bits).
CREATE FUNCTION sr_snowflake_floor(ts TIMESTAMPTZ) RETURNS BIGINT
LANGUAGE sql IMMUTABLE AS $$
    SELECT (floor(extract(epoch FROM ts) * 1000)::BIGINT - 1767225600000) << 22
$$;

-- Idempotently create the partition covering the UTC month containing `month`.
-- Called here for the initial window and later by the partition-maintenance job.
CREATE FUNCTION sr_ensure_message_partition(month DATE) RETURNS TEXT
LANGUAGE plpgsql AS $$
DECLARE
    start_ts TIMESTAMPTZ := date_trunc('month', month::TIMESTAMP) AT TIME ZONE 'UTC';
    end_ts   TIMESTAMPTZ := start_ts + INTERVAL '1 month';
    part     TEXT := format('messages_%s', to_char(start_ts AT TIME ZONE 'UTC', 'YYYY_MM'));
BEGIN
    IF start_ts < TIMESTAMPTZ '2026-01-01 00:00:00+00' THEN
        RAISE EXCEPTION 'month % is before the SIDE ROOM epoch', month;
    END IF;
    EXECUTE format(
        'CREATE TABLE IF NOT EXISTS %I PARTITION OF messages FOR VALUES FROM (%s) TO (%s)',
        part, sr_snowflake_floor(start_ts), sr_snowflake_floor(end_ts));
    RETURN part;
END $$;

-- Epoch month through twelve months ahead of when the migration runs.
DO $$
DECLARE
    m DATE := DATE '2026-01-01';
    last_m DATE := (date_trunc('month', now()) + INTERVAL '12 months')::DATE;
BEGIN
    WHILE m <= last_m LOOP
        PERFORM sr_ensure_message_partition(m);
        m := (m + INTERVAL '1 month')::DATE;
    END LOOP;
END $$;

-- Edit history; visible to high-level staff only, purged with the message (doc 04).
-- No FK: the parent is partitioned and purge jobs delete both explicitly.
CREATE TABLE message_edits (
    id                BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    message_id        BIGINT NOT NULL,
    edited_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    previous_content  TEXT NOT NULL,
    previous_embeds   JSONB NOT NULL DEFAULT '[]'::jsonb
);
CREATE INDEX message_edits_message_idx ON message_edits (message_id, edited_at);
