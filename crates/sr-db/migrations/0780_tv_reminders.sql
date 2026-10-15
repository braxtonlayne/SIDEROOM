-- 0780: Live TV reminders (TV-PLY-005, G-10). One row per (account, block). The block id is a plain reference:
-- a regenerated schedule deletes and recreates blocks, and the reminder then answers with a notice instead of
-- vanishing, so there is no foreign key to tv_blocks. The title and start are a snapshot for that notice.
CREATE TABLE tv_reminders (
    account_id  BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    block_id    BIGINT NOT NULL,
    channel_id  BIGINT NOT NULL,
    title       TEXT NOT NULL,
    start_ms    BIGINT NOT NULL,
    remind_at   BIGINT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (account_id, block_id)
);
CREATE INDEX tv_reminders_due_idx ON tv_reminders (remind_at);
