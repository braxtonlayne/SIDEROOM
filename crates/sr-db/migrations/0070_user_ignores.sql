-- 0070: Ignore is independent of friendship (open question 110, LNG-FRD-005).
-- Ignoring hides messages, activity and DM notifications; friend rows stay.
-- Relationship type 6 ("ignored") is retired: its rows move here, then the code is dropped.
CREATE TABLE user_ignores (
    user_id     BIGINT NOT NULL REFERENCES accounts (id),
    target_id   BIGINT NOT NULL REFERENCES accounts (id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, target_id),
    CHECK (user_id <> target_id)
);
CREATE INDEX user_ignores_target_idx ON user_ignores (target_id);

INSERT INTO user_ignores (user_id, target_id, created_at)
SELECT user_id, target_id, created_at FROM relationships WHERE type = 6;
DELETE FROM relationships WHERE type = 6;

ALTER TABLE relationships DROP CONSTRAINT relationships_type_check;
ALTER TABLE relationships ADD CONSTRAINT relationships_type_check CHECK (type IN (1, 2, 3, 4));
