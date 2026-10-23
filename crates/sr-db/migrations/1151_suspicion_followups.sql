-- 1151: Phase 19 suspicion round (D-345 to D-348; P-12, P-13, P-14).
--
-- * `evasion_signals.detail_snapshot`: what a staff member looked at when they confirmed a signal (kinds, counts,
--   overlap dates, the number of shared addresses, the window; never an address). The sign-in rows themselves are
--   deleted after 180 days with no hold exception (D-347), so the finding is kept here.
-- * `evasion_signals.case_id`: the case opened from the signal (P-14).
-- * `moderation_case_evidence.kind` gains `evasion_signal`: the snapshot copied onto the Person's open case.
-- * `suspicion_events.kind`: marks an event that is not a raise or a clear (`reregistration_refused`, P-13).

ALTER TABLE evasion_signals
    ADD COLUMN detail_snapshot JSONB,
    ADD COLUMN case_id BIGINT REFERENCES moderation_cases (id);
CREATE INDEX evasion_signals_case_idx ON evasion_signals (case_id) WHERE case_id IS NOT NULL;

ALTER TABLE moderation_case_evidence DROP CONSTRAINT moderation_case_evidence_kind_check;
ALTER TABLE moderation_case_evidence
    ADD CONSTRAINT moderation_case_evidence_kind_check
    CHECK (kind IN ('message_snapshot', 'target_summary', 'evasion_signal'));

ALTER TABLE suspicion_events ADD COLUMN kind TEXT;
