-- 2160: OPS-CRASH-001 (wave 45-A), the deduplicated crash triage store.
--
-- One row per distinct crash: the fingerprint is a SHA-256 over (commit, normalized top stack frames), so a hundred
-- browsers hitting the same bug are one row with a count. `card` is the allow-listed crash card exactly as the server
-- rebuilt it (never the client's payload), `replay` the last 60 deterministic ticks (opaque enums), and `channel_id` /
-- `message_id` point at the card in `#crash-triage` of the Staff Server so a repeat can refresh it in place.
--
-- Nothing here names a person: no account id, no address, no message text, no URL beyond a route NAME. The reporter's
-- identity is used only for the per-account rate limit, in memory. A crash report is therefore not personal data and
-- needs no entry in the erasure transaction.
CREATE TABLE crash_reports (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    fingerprint TEXT NOT NULL UNIQUE,
    commit_sha TEXT NOT NULL,
    kind TEXT NOT NULL,
    route TEXT NOT NULL,
    report_count INTEGER NOT NULL DEFAULT 1,
    first_seen TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen TIMESTAMPTZ NOT NULL DEFAULT now(),
    card JSONB NOT NULL,
    replay JSONB NOT NULL,
    channel_id BIGINT,
    message_id BIGINT,
    CONSTRAINT crash_reports_fingerprint_hex CHECK (fingerprint ~ '^[0-9a-f]{64}$'),
    CONSTRAINT crash_reports_count_positive CHECK (report_count >= 1)
);
CREATE INDEX crash_reports_last_seen_idx ON crash_reports (last_seen DESC);
