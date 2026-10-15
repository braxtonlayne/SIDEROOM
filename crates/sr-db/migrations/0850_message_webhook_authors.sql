-- DEV-BOT-006: a message posted through a webhook shows the webhook (or the per-call
-- username/avatar override) as its author. `messages.author_id` stays the webhook creator
-- (their permissions applied when it was sent); this side table says how to render it.
-- No foreign keys: `messages` is partitioned and the webhook may be deleted while its
-- messages stay, keeping the name they were posted under.
CREATE TABLE message_webhook_authors (
    message_id BIGINT PRIMARY KEY,
    webhook_id BIGINT NOT NULL,
    username   TEXT NOT NULL,
    avatar_url TEXT
);
CREATE INDEX message_webhook_authors_webhook_idx ON message_webhook_authors (webhook_id);
