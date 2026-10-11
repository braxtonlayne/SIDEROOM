-- 0060: nonce dedupe lookups (open question 97). Partitioned tables cannot carry a
-- unique index without the partition key, so this is a plain partial index. The
-- nonce itself lives in the existing `messages.nonce` column (0012).
CREATE INDEX messages_nonce_idx ON messages (channel_id, author_id, nonce)
    WHERE nonce IS NOT NULL;
