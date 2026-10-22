-- 1090: Phase 7 step 1, cases and reports (MOD-CASE-001, MOD-RPT-001; D-209 to D-215, D-315;
-- docs/specs/moderation-compliance.md sections 3 and 4).
--
-- A case groups everything about one problem. Reports, evidence snapshots, notes and the
-- timeline hang off it. Closed cases, evidence and events are kept forever (D-212), so the
-- evidence, notes and events tables are append-only: a trigger refuses UPDATE and DELETE.
-- No foreign key points at guilds or channels: a case must outlive a deleted server.

CREATE TABLE moderation_cases (
    id                  BIGINT PRIMARY KEY,
    state               TEXT NOT NULL DEFAULT 'open'
                        CHECK (state IN ('open', 'triage', 'investigating', 'actioned', 'dismissed', 'closed')),
    -- The section 8 vocabulary (ban_reason_category) plus 'other'; text so a value is a one-line change.
    category            TEXT NOT NULL
                        CHECK (category IN ('spam', 'harassment', 'illegal_content', 'child_safety', 'ban_evasion',
                                            'fraud', 'platform_abuse', 'nsfw_policy', 'other')),
    severity            TEXT NOT NULL DEFAULT 'standard' CHECK (severity IN ('standard', 'serious')),
    -- Who works it (D-209): the guild's moderators, platform staff, or the reviewers of a library/cinema item.
    routing             TEXT NOT NULL CHECK (routing IN ('guild', 'platform', 'reviewers')),
    -- Set when guild moderators hand a guild case to platform staff.
    escalated_at        TIMESTAMPTZ,
    escalated_by        BIGINT REFERENCES accounts (id),
    summary             TEXT NOT NULL DEFAULT '' CHECK (char_length(summary) <= 200),
    target_kind         TEXT NOT NULL
                        CHECK (target_kind IN ('message', 'user', 'channel', 'guild', 'library', 'cinema', 'person', 'other')),
    target_id           BIGINT,
    target_channel_id   BIGINT,
    guild_id            BIGINT,
    subject_person_id   BIGINT REFERENCES persons (id),
    subject_account_id  BIGINT REFERENCES accounts (id),
    assignee_id         BIGINT REFERENCES accounts (id),
    opened_by_kind      TEXT NOT NULL CHECK (opened_by_kind IN ('report', 'staff', 'appeal')),
    opened_by_id        BIGINT REFERENCES accounts (id),
    opened_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    first_action_at     TIMESTAMPTZ,
    closed_at           TIMESTAMPTZ,
    outcome             TEXT NOT NULL DEFAULT '',
    -- A reopened-too-late case continues as a new case that links back (spec 4.2).
    linked_case_id      BIGINT REFERENCES moderation_cases (id),
    CHECK ((state = 'closed') = (closed_at IS NOT NULL))
);
CREATE INDEX moderation_cases_queue_idx ON moderation_cases (state, opened_at);
CREATE INDEX moderation_cases_guild_idx ON moderation_cases (guild_id, opened_at DESC) WHERE guild_id IS NOT NULL;
CREATE INDEX moderation_cases_target_idx ON moderation_cases (target_kind, target_id) WHERE target_id IS NOT NULL;
CREATE INDEX moderation_cases_assignee_idx ON moderation_cases (assignee_id) WHERE assignee_id IS NOT NULL;
CREATE INDEX moderation_cases_subject_idx ON moderation_cases (subject_person_id) WHERE subject_person_id IS NOT NULL;

-- One row per report. The reporter is stored here and shown only as D-209 allows.
CREATE TABLE moderation_reports (
    id                   BIGINT PRIMARY KEY,
    case_id              BIGINT NOT NULL REFERENCES moderation_cases (id),
    reporter_id          BIGINT NOT NULL REFERENCES accounts (id),
    target_kind          TEXT NOT NULL
                         CHECK (target_kind IN ('message', 'user', 'channel', 'guild', 'library', 'cinema', 'other')),
    target_id            BIGINT NOT NULL,
    target_channel_id    BIGINT,
    guild_id             BIGINT,
    category             TEXT NOT NULL
                         CHECK (category IN ('spam', 'harassment', 'illegal_content', 'child_safety', 'ban_evasion',
                                             'fraud', 'platform_abuse', 'nsfw_policy', 'other')),
    note                 TEXT NOT NULL DEFAULT '' CHECK (char_length(note) <= 2000),
    -- "Moderators may see my name" (spec 3.3). Off unless the reporter ticks it.
    moderators_may_see   BOOLEAN NOT NULL DEFAULT FALSE,
    state                TEXT NOT NULL DEFAULT 'received' CHECK (state IN ('received', 'under_review', 'closed')),
    -- A short line the reporter can read when the case closes. Never the other party's details.
    outcome_line         TEXT NOT NULL DEFAULT '',
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at            TIMESTAMPTZ
);
-- One open report per (reporter, target) (spec 3.3).
CREATE UNIQUE INDEX moderation_reports_open_uidx
    ON moderation_reports (reporter_id, target_kind, target_id) WHERE state <> 'closed';
CREATE INDEX moderation_reports_case_idx ON moderation_reports (case_id);
CREATE INDEX moderation_reports_reporter_idx ON moderation_reports (reporter_id, created_at DESC);

-- Immutable snapshots taken at report time (spec 3.2, D-214).
CREATE TABLE moderation_case_evidence (
    id          BIGINT PRIMARY KEY,
    case_id     BIGINT NOT NULL REFERENCES moderation_cases (id),
    report_id   BIGINT REFERENCES moderation_reports (id),
    kind        TEXT NOT NULL CHECK (kind IN ('message_snapshot', 'target_summary')),
    -- A DM snapshot is readable by platform staff only (D-214); guild snapshots by the case's handlers.
    private     BOOLEAN NOT NULL DEFAULT FALSE,
    payload     JSONB NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX moderation_case_evidence_case_idx ON moderation_case_evidence (case_id, id);

CREATE TABLE moderation_case_notes (
    id          BIGINT PRIMARY KEY,
    case_id     BIGINT NOT NULL REFERENCES moderation_cases (id),
    author_id   BIGINT NOT NULL REFERENCES accounts (id),
    body        TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 4000),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX moderation_case_notes_case_idx ON moderation_case_notes (case_id, id);

-- The timeline and the record of actions. `actor_id` is NULL for the system.
CREATE TABLE moderation_case_events (
    id          BIGINT PRIMARY KEY,
    case_id     BIGINT NOT NULL REFERENCES moderation_cases (id),
    actor_id    BIGINT REFERENCES accounts (id),
    kind        TEXT NOT NULL CHECK (char_length(kind) BETWEEN 1 AND 40),
    detail      JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX moderation_case_events_case_idx ON moderation_case_events (case_id, id);

CREATE FUNCTION moderation_append_only() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION '% is append-only (D-212)', TG_TABLE_NAME USING ERRCODE = 'restrict_violation';
END $$;
CREATE TRIGGER moderation_case_evidence_immutable BEFORE UPDATE OR DELETE ON moderation_case_evidence
    FOR EACH ROW EXECUTE FUNCTION moderation_append_only();
CREATE TRIGGER moderation_case_notes_immutable BEFORE UPDATE OR DELETE ON moderation_case_notes
    FOR EACH ROW EXECUTE FUNCTION moderation_append_only();
CREATE TRIGGER moderation_case_events_immutable BEFORE UPDATE OR DELETE ON moderation_case_events
    FOR EACH ROW EXECUTE FUNCTION moderation_append_only();

-- Holds (XC-MOD-001 gets its owner). `case_ref` stays for the free text older rows may carry; new holds
-- point at their case. Releasing clears `hold_id` on the held rows and stamps the hold, so the hold row
-- itself stays as history.
ALTER TABLE moderation_holds
    ADD COLUMN case_id        BIGINT REFERENCES moderation_cases (id),
    ADD COLUMN scope_kind     TEXT CHECK (scope_kind IN ('message', 'attachment', 'submission', 'user_messages')),
    ADD COLUMN scope_id       BIGINT,
    ADD COLUMN released_at    TIMESTAMPTZ,
    ADD COLUMN released_by    BIGINT REFERENCES accounts (id),
    ADD COLUMN release_reason TEXT,
    ADD COLUMN reviewed_at    TIMESTAMPTZ,
    ADD CHECK ((released_at IS NULL) = (released_by IS NULL));
CREATE INDEX moderation_holds_case_idx ON moderation_holds (case_id) WHERE case_id IS NOT NULL;
CREATE INDEX moderation_holds_active_idx ON moderation_holds (created_at) WHERE released_at IS NULL;

-- Appeals (D-215): one per decision, and a decision here is a platform ban. No second appeal, so the
-- ban id is unique. The case link is optional: older bans were made outside any case.
CREATE TABLE moderation_appeals (
    id                    BIGINT PRIMARY KEY,
    ban_id                BIGINT NOT NULL UNIQUE REFERENCES person_bans (id),
    case_id               BIGINT REFERENCES moderation_cases (id),
    appellant_account_id  BIGINT NOT NULL REFERENCES accounts (id),
    message               TEXT NOT NULL CHECK (char_length(message) BETWEEN 1 AND 2000),
    state                 TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'upheld', 'overturned')),
    decided_by            BIGINT REFERENCES accounts (id),
    decision_reason       TEXT NOT NULL DEFAULT '',
    decided_at            TIMESTAMPTZ,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((state = 'open') = (decided_at IS NULL))
);
CREATE INDEX moderation_appeals_open_idx ON moderation_appeals (created_at) WHERE state = 'open';
