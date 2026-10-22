-- 1101: N-02 (D-317): when the member was told of the one extension by system DM. NULL when no DM could be
-- sent (the person is unreachable in-app); staff then answer on the intake channel.
ALTER TABLE data_requests ADD COLUMN member_notified_at TIMESTAMPTZ;
