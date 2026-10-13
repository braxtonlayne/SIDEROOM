-- 0161: second factor on email change (D-167, F-03).
-- A purpose for the "This wasn't me" token mailed to the old address. Adding an enum
-- value is safe inside the migration transaction as long as it is not used here.
ALTER TYPE email_token_purpose ADD VALUE IF NOT EXISTS 'cancel_email_change';

-- When the session last passed a second-factor check; a re-prompt within 15 minutes
-- counts for sensitive changes. NULL = never (e.g. trusted-device login).
ALTER TABLE sessions ADD COLUMN mfa_verified_at TIMESTAMPTZ;
