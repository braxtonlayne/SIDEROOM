-- BRG-GW-001: a message that came in from Discord over the bridge shows the Discord user (name, avatar,
-- id) as its author with a DISCORD tag. `messages.author_id` stays the bridge's integration author; this
-- side table says how to render it. `links` holds the attachment links Discord gave (its CDN links carry
-- an expiry, `ex`); the API shows "(link expired)" after `expires_at_ms`, nothing is re-hosted.
-- No foreign keys: `messages` is partitioned (same shape as message_webhook_authors).
CREATE TABLE message_bridge_authors (
    message_id      BIGINT PRIMARY KEY,
    platform        TEXT NOT NULL,
    author_name     TEXT NOT NULL,
    author_avatar   TEXT,
    discord_user_id TEXT,
    links           JSONB NOT NULL DEFAULT '[]'::jsonb
);
