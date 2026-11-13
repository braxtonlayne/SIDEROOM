-- 2790: SEC-AUD-166 (wave 64, D-647..D-650), the beta bug reporter's store.
--
-- What this table holds about a person, and why: the reporter's account id (so staff can ask a follow-up question),
-- the member's own description of the problem (free text, at most 2000 characters, stored as written), the allow-listed
-- debug bundle the server rebuilt (build, route NAME, viewport, capability flags, connection state, up to 20 failed
-- request templates; never a message, URL, token, key or another person's id) and, only if the member chose to attach
-- one, a JPEG of their screen that they previewed and could black out. THIS TABLE IS PERSONAL DATA: a picture of a
-- screen is the most sensitive thing the platform stores about a member outside their own content.
--
-- Who reads it: any appointed staff member (STAFF_BASIC); opening a report, its picture, and closing one each write a
-- platform audit entry that cannot be switched off. Retention: the picture is deleted when the report is closed or after
-- 30 days, the report after 90 days (a daily job); erasing the account deletes its reports; the data export lists them.
CREATE TABLE bug_reports (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    reporter_id BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    status TEXT NOT NULL DEFAULT 'open',
    closed_at TIMESTAMPTZ,
    closed_by BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    commit_sha TEXT NOT NULL,
    route TEXT NOT NULL,
    description TEXT NOT NULL,
    bundle JSONB NOT NULL,
    screenshot BYTEA,
    CONSTRAINT bug_reports_status CHECK (status IN ('open', 'closed')),
    CONSTRAINT bug_reports_description_len CHECK (char_length(description) BETWEEN 1 AND 2000),
    CONSTRAINT bug_reports_screenshot_size CHECK (screenshot IS NULL OR octet_length(screenshot) <= 2097152)
);
CREATE INDEX bug_reports_status_created_idx ON bug_reports (status, created_at DESC);
CREATE INDEX bug_reports_reporter_idx ON bug_reports (reporter_id);

COMMENT ON TABLE bug_reports IS 'Beta bug reports (D-650): personal data. Picture deleted on close or after 30 days, row after 90; erased with the account.';
