-- 1150: Phase 7 step 5, inviter-suspicion tooling (MOD-SUS-001 to -005; D-031, D-125;
-- docs/specs/moderation-compliance.md section 10).
--
-- * `moderation_cases.auto_kind` marks a case the system opened because an inviter's suspicion crossed the
--   freeze or the escalation threshold. One such case per Person is open at a time (the escalation upgrades the
--   freeze case instead of opening a second).
-- * `suspicion_events.case_id` points an event at the case it opened or fed.
-- * `evasion_signals` are review items. A signal only says "look at this pair"; nothing acts on it. It holds
--   no address: the shared addresses are read from `login_events` when a staff member opens the signal, so the
--   180-day window and the scrubbing on account deletion (sessions and login events blanked) apply to them.

ALTER TABLE moderation_cases
    ADD COLUMN auto_kind TEXT CHECK (auto_kind IN ('suspicion_freeze', 'suspicion_escalation'));
CREATE UNIQUE INDEX moderation_cases_suspicion_open_uidx
    ON moderation_cases (subject_person_id)
    WHERE auto_kind IS NOT NULL AND state NOT IN ('dismissed', 'closed');

ALTER TABLE suspicion_events
    ADD COLUMN case_id BIGINT REFERENCES moderation_cases (id);

CREATE TABLE evasion_signals (
    id                BIGINT PRIMARY KEY,
    -- The Person who may be coming back, and the banned Person they resemble.
    person_id         BIGINT NOT NULL REFERENCES persons (id),
    related_person_id BIGINT NOT NULL REFERENCES persons (id),
    kind              TEXT NOT NULL
                      CHECK (kind IN ('shared_login_ip', 'shared_invite_lineage', 'new_account_after_ban')),
    -- Counts and ids only (never an address): {"shared_ips": 2} or {"relation": "sibling"} and so on.
    detail            JSONB NOT NULL DEFAULT '{}'::jsonb,
    state             TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'dismissed', 'confirmed')),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed_by       BIGINT REFERENCES accounts (id),
    reviewed_at       TIMESTAMPTZ,
    review_reason     TEXT,
    CHECK (person_id <> related_person_id),
    CHECK ((state = 'open') = (reviewed_at IS NULL))
);
-- A pair is raised once per kind; a dismissed signal is not raised again.
CREATE UNIQUE INDEX evasion_signals_pair_uidx ON evasion_signals (person_id, related_person_id, kind);
CREATE INDEX evasion_signals_open_idx ON evasion_signals (created_at DESC) WHERE state = 'open';
CREATE INDEX evasion_signals_person_idx ON evasion_signals (person_id);

-- Window reads of the sign-in history by address (the 180-day signal, doc 09).
CREATE INDEX IF NOT EXISTS login_events_ip_idx ON login_events (ip, created_at DESC) WHERE ip IS NOT NULL;
