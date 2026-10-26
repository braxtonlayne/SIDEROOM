-- 1300 (RDY-PERF-003): an index on pins (message_id).
--
-- pins_message_id_fkey is ON DELETE CASCADE from the partitioned messages table, and the primary key
-- (channel_id, message_id) cannot answer a lookup by message_id alone. The purge of deleted messages
-- (messages.purge_deleted) therefore scanned all of pins once per deleted message: EXPLAIN ANALYZE of one
-- 1000-message batch took 1600 ms with 50,000 pins and 16 ms with this index (docs/dev/load-testing.md).
CREATE INDEX IF NOT EXISTS pins_message_idx ON pins (message_id);
