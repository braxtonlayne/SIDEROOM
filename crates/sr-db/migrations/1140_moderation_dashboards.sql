-- 1140: Phase 7 step 4, staff dashboards (MOD-DASH-002 to -010, MOD-BAN-003; docs/specs/moderation-compliance.md
-- section 9). The dashboards only read, so this migration adds what keeps the reads cheap (indexes that bound
-- every aggregate to a time window or to a small partial set) and one counter table.

-- Health (reports per day, time to first action, reopen rate) reads cases and reports by time window.
CREATE INDEX moderation_cases_opened_idx ON moderation_cases (opened_at);
CREATE INDEX moderation_cases_closed_idx ON moderation_cases (closed_at) WHERE closed_at IS NOT NULL;
CREATE INDEX moderation_reports_created_idx ON moderation_reports (created_at);
CREATE INDEX moderation_appeals_created_idx ON moderation_appeals (created_at);

-- Reports inbox: the cases nobody has picked up yet, oldest first.
CREATE INDEX moderation_cases_inbox_idx ON moderation_cases (opened_at)
    WHERE state = 'open' AND assignee_id IS NULL;

-- Bans list filtered by age.
CREATE INDEX person_bans_created_at_idx ON person_bans (created_at);

-- Age and child-safety metrics count the age-restricted items.
CREATE INDEX channels_nsfw_idx ON channels (id) WHERE nsfw;
CREATE INDEX reading_series_age_idx ON reading_series (id) WHERE age_restricted;
CREATE INDEX video_titles_age_idx ON video_titles (id) WHERE age_restricted;

-- One row per reset of the age confirmation (spec 7: "recent age-gate resets"). It deliberately carries no
-- account, so the dashboard counts resets and cannot say whose they were.
CREATE TABLE age_gate_resets (
    id        BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source    TEXT NOT NULL CHECK (source IN ('self', 'staff')),
    reset_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX age_gate_resets_at_idx ON age_gate_resets (reset_at);

-- Reopen rate looks at the closing and reopening events of a window.
CREATE INDEX moderation_case_events_kind_idx ON moderation_case_events (kind, created_at)
    WHERE kind IN ('state_changed', 'reopened', 'continued_in');
-- Appeal outcomes, removals and completed requests are counted per window.
CREATE INDEX moderation_appeals_decided_idx ON moderation_appeals (decided_at) WHERE decided_at IS NOT NULL;
CREATE INDEX takedown_notices_removed_idx ON takedown_notices (removed_at) WHERE removed_at IS NOT NULL;
CREATE INDEX data_requests_completed_idx ON data_requests (completed_at) WHERE completed_at IS NOT NULL;
