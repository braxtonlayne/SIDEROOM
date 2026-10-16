-- DEV-ACT-001/002: per-application Activity settings (entry URL, URL mappings, reviewed entry hash)
-- and activity instances tied to a voice channel with their participants.
CREATE TABLE application_activities (
    application_id  BIGINT PRIMARY KEY REFERENCES applications (id) ON DELETE CASCADE,
    entry_url       TEXT,
    url_mappings    JSONB NOT NULL DEFAULT '[]'::jsonb,
    review_state    TEXT NOT NULL DEFAULT 'draft'
                    CHECK (review_state IN ('draft', 'pending', 'approved', 'rejected')),
    -- SHA-256 (hex) of the entry document the owner submitted, and of the one staff approved.
    pinned_sha256   TEXT,
    reviewed_sha256 TEXT,
    reviewed_by     BIGINT,
    reviewed_at     TIMESTAMPTZ,
    review_note     TEXT,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE activity_instances (
    id             BIGINT PRIMARY KEY,
    application_id BIGINT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    guild_id       BIGINT NOT NULL,
    channel_id     BIGINT NOT NULL,
    started_by     BIGINT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    ended_at       TIMESTAMPTZ
);
-- One running activity per voice channel.
CREATE UNIQUE INDEX activity_instances_running_idx ON activity_instances (channel_id) WHERE ended_at IS NULL;
CREATE INDEX activity_instances_app_idx ON activity_instances (application_id);

CREATE TABLE activity_participants (
    instance_id BIGINT NOT NULL REFERENCES activity_instances (id) ON DELETE CASCADE,
    user_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    joined_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    left_at     TIMESTAMPTZ,
    PRIMARY KEY (instance_id, user_id)
);
