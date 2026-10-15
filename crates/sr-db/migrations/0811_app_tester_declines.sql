-- G-84: a declined tester invitation blocks a new one for 24 hours. The row is written when the
-- invitee declines and deleted by the `apps.housekeeping` job once it is older than a day.
CREATE TABLE app_tester_declines (
    application_id BIGINT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    account_id     BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    declined_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (application_id, account_id)
);
CREATE INDEX app_tester_declines_time_idx ON app_tester_declines (declined_at);
