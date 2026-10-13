-- 0120: rich messages. Polls with an expiry sweep (LNG-MSG-010) and standard stickers
-- (LNG-MSG-002). Forwards (LNG-MSG-007) and voice messages (LNG-MSG-019) need no schema:
-- the forward snapshot lives in `messages.reference`, voice metadata in the existing
-- `attachments.duration_secs` / `attachments.waveform` columns.

-- When voting closes, for polls that are not finalised yet. NULL once finalised (and for
-- messages without a poll), so the partial index stays tiny. The authoritative poll
-- definition, including `expiry_ms` and `finalized`, is `messages.poll`.
ALTER TABLE messages ADD COLUMN poll_expires_at TIMESTAMPTZ;
CREATE INDEX messages_poll_due_idx ON messages (poll_expires_at)
    WHERE poll_expires_at IS NOT NULL;

-- Messages created before this migration cannot carry a poll (creation was rejected),
-- so no backfill is needed.

-- Standard (platform) stickers belong to no guild and are usable everywhere.
ALTER TABLE stickers ALTER COLUMN guild_id DROP NOT NULL;
