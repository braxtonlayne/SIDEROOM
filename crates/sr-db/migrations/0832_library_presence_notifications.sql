-- 0832: "Reading" presence (LIB-RDR-017, D-193) and the bell's seen marker (LIB-SOC-007).

-- One short-lived row per account, refreshed by the reader heartbeat. Reads ignore expired rows.
CREATE TABLE reading_presence (
    user_id     BIGINT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    series_id   BIGINT NOT NULL REFERENCES reading_series (id) ON DELETE CASCADE,
    chapter_id  BIGINT NOT NULL,
    restricted  BOOLEAN NOT NULL DEFAULT FALSE,
    started_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL
);
CREATE INDEX reading_presence_expiry_idx ON reading_presence (expires_at);

-- When the member last opened the notifications bell; new chapters of followed series after this
-- (and after the follow) are "unread".
CREATE TABLE reading_notification_state (
    user_id  BIGINT PRIMARY KEY REFERENCES accounts (id) ON DELETE CASCADE,
    seen_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
