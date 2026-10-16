-- ENT-AUTH-015: new-device sign-in alerts.
--
-- `accounts.login_alerts` is the account's switch (on by default). `account_known_devices` remembers the
-- device labels (the "Firefox on Windows" kind of label derived from the User-Agent) an account has signed in
-- from, so only a first sight of a label sends the alert mail. The first device an account ever signs in from
-- (the one it registered with) never alerts.
ALTER TABLE accounts ADD COLUMN login_alerts BOOLEAN NOT NULL DEFAULT TRUE;

CREATE TABLE account_known_devices (
    account_id    BIGINT NOT NULL REFERENCES accounts (id),
    device_key    TEXT NOT NULL,
    device_name   TEXT NOT NULL,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (account_id, device_key)
);
