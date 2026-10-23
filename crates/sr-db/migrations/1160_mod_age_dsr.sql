-- 1160: age and child-safety handling, and the last three data-subject rights (MOD-AGE-003, MOD-DSR-005 to -007;
-- docs/specs/moderation-compliance.md sections 6 and 7; D-294, D-295).

-- An account staff found to be under 18 is "reset-and-restricted": the age confirmation is reset (the gate asks
-- again) and the account cannot answer "yes" itself until staff lift the lock (MOD-AGE-003). Who and why are in
-- the access log; the columns say whether the lock is on.
ALTER TABLE accounts
    ADD COLUMN age_locked_at TIMESTAMPTZ,
    ADD COLUMN age_locked_by BIGINT REFERENCES accounts (id),
    ADD CHECK ((age_locked_at IS NULL) = (age_locked_by IS NULL));

-- A deletion request that meets a hold keeps the held rows; the hold is flagged for a decision by staff (release,
-- or retain with a stated legal basis; spec section 6, MOD-DSR-005). The flag stays on the row as history.
ALTER TABLE moderation_holds
    ADD COLUMN erasure_request_id BIGINT REFERENCES data_requests (id),
    ADD COLUMN erasure_flagged_at TIMESTAMPTZ,
    ADD COLUMN erasure_decision   TEXT CHECK (erasure_decision IN ('retained')),
    ADD COLUMN erasure_basis      TEXT,
    ADD COLUMN erasure_decided_by BIGINT REFERENCES accounts (id),
    ADD COLUMN erasure_decided_at TIMESTAMPTZ,
    ADD CHECK ((erasure_request_id IS NULL) = (erasure_flagged_at IS NULL)),
    ADD CHECK ((erasure_decision IS NULL) = (erasure_decided_at IS NULL)
           AND (erasure_decision IS NULL) = (erasure_decided_by IS NULL)
           AND (erasure_decision IS NULL) = (erasure_basis IS NULL));
CREATE INDEX moderation_holds_erasure_idx ON moderation_holds (erasure_flagged_at)
    WHERE erasure_flagged_at IS NOT NULL AND erasure_decision IS NULL AND released_at IS NULL;

-- Restriction or objection (GDPR articles 18 and 21, MOD-DSR-006): while a request is open staff can hide the
-- account's shared activity from discovery. Nothing is deleted. The hiding ends with the request (the function
-- below only looks at requests still in progress), or earlier when staff lift it (the columns go back to NULL;
-- the access log keeps both moments).
ALTER TABLE data_requests
    ADD COLUMN activity_hidden_at     TIMESTAMPTZ,
    ADD COLUMN activity_hidden_by     BIGINT REFERENCES accounts (id),
    ADD COLUMN activity_hidden_reason TEXT,
    ADD CHECK ((activity_hidden_at IS NULL) = (activity_hidden_by IS NULL)
           AND (activity_hidden_at IS NULL) = (activity_hidden_reason IS NULL));

-- Every reader of shared activity (watching presence, scoped aggregates, recommendations, profile) goes through
-- this function (0797), so one change hides the account from all of them.
CREATE OR REPLACE FUNCTION activity_shares(uid BIGINT, section TEXT) RETURNS BOOLEAN
LANGUAGE sql STABLE AS $$
    SELECT COALESCE(
               (SELECT settings #>> ARRAY['activity', 'share_' || section]
                  FROM user_settings WHERE user_id = uid), 'true') <> 'false'
       AND COALESCE(
               (SELECT settings #>> '{activity,paused}'
                  FROM user_settings WHERE user_id = uid), 'false') <> 'true'
       AND NOT EXISTS (
               SELECT 1 FROM data_requests r
                WHERE r.account_id = uid AND r.activity_hidden_at IS NOT NULL
                  AND r.state IN ('open', 'verified', 'running'))
$$;
