-- 0041: one-time Owner login link (open question 43). Its own purpose, so a
-- reset token can never log anyone in and a login link can never reset a
-- password. Adding an enum value is safe inside the migration transaction as
-- long as the value is not used in the same transaction.
ALTER TYPE email_token_purpose ADD VALUE IF NOT EXISTS 'login_link';
