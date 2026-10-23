-- 1230: CSP violation counters (RDY-SEC-009). `POST /internal/csp-report` stores nothing about the reporter:
-- one counter per violated directive, per blocked origin (scheme and host only) and per document path (query and
-- fragment dropped, id-like segments folded). Each kind is capped at 200 keys by the repository; later keys count
-- under '(other)'.
CREATE TABLE csp_report_counters (
    kind        TEXT        NOT NULL CHECK (kind IN ('directive', 'origin', 'path')),
    key         TEXT        NOT NULL CHECK (char_length(key) <= 160),
    count       BIGINT      NOT NULL DEFAULT 0,
    first_seen  TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (kind, key)
);
