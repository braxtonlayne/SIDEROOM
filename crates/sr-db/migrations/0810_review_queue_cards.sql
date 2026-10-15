-- G-68: the `#review-queue` card of a submission in the Staff Server, so the card can be edited to
-- the outcome (and lose its buttons) when the submission is decided, from the card or elsewhere.
CREATE TABLE review_queue_cards (
    submission_id BIGINT PRIMARY KEY REFERENCES submissions (id) ON DELETE CASCADE,
    channel_id    BIGINT NOT NULL,
    message_id    BIGINT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
