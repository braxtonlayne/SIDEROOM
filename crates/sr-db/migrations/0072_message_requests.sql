-- 0072: Message Request state per recipient (open question 112, LNG-DM-003).
-- 'pending' is set on the recipient's row when a non-friend's first DM lands in requests.
ALTER TABLE channel_recipients
    ADD COLUMN request_state TEXT NOT NULL DEFAULT 'accepted'
        CHECK (request_state IN ('accepted', 'pending', 'ignored', 'spam'));
