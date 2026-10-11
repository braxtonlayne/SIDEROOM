-- 0062: attachment rows for the upload-slot flow (open question 153). A slot knows
-- its channel before any message references it, and the client-declared content
-- type is kept apart from the sniffed one. Pending slots are garbage-collected by
-- `attachments.gc_unreferenced` after 24 hours.
ALTER TABLE attachments
    ADD COLUMN channel_id BIGINT REFERENCES channels (id) ON DELETE CASCADE,
    ADD COLUMN declared_content_type TEXT;
CREATE INDEX attachments_channel_idx ON attachments (channel_id);
CREATE INDEX attachments_pending_idx ON attachments (created_at)
    WHERE state = 'pending' AND message_id IS NULL;
