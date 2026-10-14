-- 0460: Live TV channels (TV-ENG-001/002, docs/specs/cinema-live-tv.md). A channel is a number, a name, a
-- time zone and, for rule-based channels, the rules the schedule engine generates blocks from.

CREATE TABLE tv_channels (
    id             BIGINT PRIMARY KEY,
    number         INTEGER NOT NULL UNIQUE CHECK (number BETWEEN 1 AND 9999),
    name           TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 80),
    call_sign      TEXT CHECK (call_sign IS NULL OR char_length(call_sign) <= 40),
    era_label      TEXT CHECK (era_label IS NULL OR char_length(era_label) <= 60),
    theme          TEXT CHECK (theme IS NULL OR char_length(theme) <= 40),
    logo_key       TEXT CHECK (logo_key IS NULL OR logo_key LIKE 'tv/%'),
    enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    kind           TEXT NOT NULL CHECK (kind IN ('rule_based', 'manual')),
    timezone       TEXT NOT NULL DEFAULT 'UTC',
    age_restricted BOOLEAN NOT NULL DEFAULT FALSE,
    filler_policy  TEXT NOT NULL DEFAULT 'bumpers'
                   CHECK (filler_policy IN ('bumpers', 'next_program_early', 'slate', 'off_air')),
    rules          JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- Seed of the shuffle generator.
    seed           BIGINT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
