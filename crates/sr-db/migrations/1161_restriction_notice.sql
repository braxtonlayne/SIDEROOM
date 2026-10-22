-- 1161: the member is told when staff hide or show again their shared activity during a data request (D-343,
-- GDPR article 18(3)). `restriction_notified_at` is the last notice that reached them; `restriction_notice_failed`
-- says the last attempt did not, so the queue can ask staff to use the intake channel.
ALTER TABLE data_requests
    ADD COLUMN restriction_notified_at    TIMESTAMPTZ,
    ADD COLUMN restriction_notice_failed  BOOLEAN NOT NULL DEFAULT FALSE;
