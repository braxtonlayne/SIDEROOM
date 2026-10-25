-- 1310 (RDY-PAR-004): a request can also sit in "Likely spam" in Message Requests, either because the
-- recipient's DM spam filter flagged the first message or because the recipient filed it as spam.
ALTER TABLE channel_recipients
    DROP CONSTRAINT IF EXISTS channel_recipients_request_reason_check;
ALTER TABLE channel_recipients
    ADD CONSTRAINT channel_recipients_request_reason_check
        CHECK (request_reason IN ('server_filter', 'new_account', 'spam'));
