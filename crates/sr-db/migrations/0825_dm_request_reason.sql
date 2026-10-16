-- 0825: why a conversation sits in a recipient's Message Requests because of a server's
-- Protection settings (G-95, D-224): `server_filter` (every shared server filters DMs from
-- non-friends) or `new_account` (the sender is younger than a shared server's threshold).
-- NULL for requests that come from the recipient's own DM settings.
ALTER TABLE channel_recipients
    ADD COLUMN request_reason TEXT
        CHECK (request_reason IN ('server_filter', 'new_account'));
