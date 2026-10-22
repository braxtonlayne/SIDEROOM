-- 1130: removing one attachment from a message through a moderation case (N-21, D-122).
-- A removed attachment leaves the message at once (`message_id` is cleared, so every reader stops showing it).
-- When nothing holds it, the object and the row go straight away. When a hold covers it, the row and its stored
-- object stay for the evidence, with these columns saying where it came from and why it is not shown; once the
-- hold is released the purge job deletes the object and the row (`removed_at IS NOT NULL AND hold_id IS NULL`).
ALTER TABLE attachments
    ADD COLUMN removed_at              TIMESTAMPTZ,
    ADD COLUMN removed_by              BIGINT,
    ADD COLUMN removed_case_id         BIGINT,
    ADD COLUMN removed_from_message_id BIGINT;
CREATE INDEX attachments_removed_idx ON attachments (removed_at) WHERE removed_at IS NOT NULL;
