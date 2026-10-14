-- 0634: membership applications (LNG-SSET-042, backend half). A guild that has "apply to
-- join" on keeps a short questions form; people who are not members submit answers and
-- moderators approve (which adds the member) or reject them.
CREATE TABLE guild_application_forms (
    guild_id     BIGINT PRIMARY KEY REFERENCES guilds (id) ON DELETE CASCADE,
    enabled      BOOLEAN NOT NULL DEFAULT FALSE,
    description  TEXT,
    questions    JSONB NOT NULL DEFAULT '[]'::jsonb,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE guild_applications (
    id                BIGINT PRIMARY KEY,
    guild_id          BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    user_id           BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    answers           JSONB NOT NULL DEFAULT '[]'::jsonb,
    status            TEXT NOT NULL DEFAULT 'pending'
                      CHECK (status IN ('pending', 'approved', 'rejected')),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed_by       BIGINT,
    reviewed_at       TIMESTAMPTZ,
    rejection_reason  TEXT
);
-- One open application per person and guild.
CREATE UNIQUE INDEX guild_applications_pending_uidx
    ON guild_applications (guild_id, user_id) WHERE status = 'pending';
CREATE INDEX guild_applications_queue_idx ON guild_applications (guild_id, status, id DESC);
CREATE INDEX guild_applications_user_idx ON guild_applications (user_id, guild_id, id DESC);
