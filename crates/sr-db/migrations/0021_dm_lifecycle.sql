-- 0021: DM / group DM lifecycle columns on channel_recipients. LNG-DM-001.
-- seq gives a stable join order (owner transfer picks the longest-standing member);
-- hidden_at records "Close DM" per user (reopened by the next message).
ALTER TABLE channel_recipients
    ADD COLUMN seq        BIGINT GENERATED ALWAYS AS IDENTITY,
    ADD COLUMN hidden_at  TIMESTAMPTZ;
CREATE INDEX channel_recipients_channel_seq_idx ON channel_recipients (channel_id, seq);
