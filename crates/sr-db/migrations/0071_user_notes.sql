-- 0071: private notes on any user (open question 111, LNG-PRF-007). Discord limit: 256 characters.
CREATE TABLE user_notes (
    user_id     BIGINT NOT NULL REFERENCES accounts (id),
    target_id   BIGINT NOT NULL REFERENCES accounts (id),
    note        TEXT NOT NULL CHECK (char_length(note) BETWEEN 1 AND 256),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, target_id),
    CHECK (user_id <> target_id)
);
