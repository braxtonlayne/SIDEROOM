-- Wave 59 (SEC-AUD-081, SEC-AUD-088; D-606): two small hardening steps.
--
-- 1. A branch action is claimed by moving its request `approved` -> `executing` before it runs, so two racing
--    calls cannot both act (SEC-AUD-081). The state column's CHECK learns the new state.
ALTER TABLE moderation_requests DROP CONSTRAINT IF EXISTS moderation_requests_state_check;
ALTER TABLE moderation_requests ADD CONSTRAINT moderation_requests_state_check
    CHECK (state IN ('pending', 'approved', 'executing', 'executed', 'rejected', 'cancelled'));

-- 2. Append-only audit tables (D-606). Only `staff_access_log` refused TRUNCATE (0130), and `application_audit` and
--    `payment_audit` refused UPDATE but not DELETE. A row trigger never fires for TRUNCATE, so every append-only
--    audit table gets a statement trigger too. DELETE stays open only where a sanctioned purge exists: the D-563
--    retention purge of `audit_log_entries` (its own trigger allows it inside the retention job, which is itself
--    audited); nothing else may delete.
CREATE FUNCTION sr_forbid_truncate() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION '% is append-only: TRUNCATE is refused', TG_TABLE_NAME
        USING ERRCODE = 'restrict_violation';
END $$;

CREATE TRIGGER application_audit_no_delete
    BEFORE DELETE ON application_audit
    FOR EACH ROW EXECUTE FUNCTION sr_forbid_mutation();
CREATE TRIGGER payment_audit_no_delete
    BEFORE DELETE ON payment_audit
    FOR EACH ROW EXECUTE FUNCTION sr_forbid_mutation();

CREATE TRIGGER platform_audit_log_no_truncate
    BEFORE TRUNCATE ON platform_audit_log
    FOR EACH STATEMENT EXECUTE FUNCTION sr_forbid_truncate();
CREATE TRIGGER audit_log_entries_no_truncate
    BEFORE TRUNCATE ON audit_log_entries
    FOR EACH STATEMENT EXECUTE FUNCTION sr_forbid_truncate();
CREATE TRIGGER invite_credits_ledger_no_truncate
    BEFORE TRUNCATE ON invite_credits_ledger
    FOR EACH STATEMENT EXECUTE FUNCTION sr_forbid_truncate();
CREATE TRIGGER moderation_case_events_no_truncate
    BEFORE TRUNCATE ON moderation_case_events
    FOR EACH STATEMENT EXECUTE FUNCTION sr_forbid_truncate();
CREATE TRIGGER moderation_case_evidence_no_truncate
    BEFORE TRUNCATE ON moderation_case_evidence
    FOR EACH STATEMENT EXECUTE FUNCTION sr_forbid_truncate();
CREATE TRIGGER moderation_case_notes_no_truncate
    BEFORE TRUNCATE ON moderation_case_notes
    FOR EACH STATEMENT EXECUTE FUNCTION sr_forbid_truncate();
CREATE TRIGGER takedown_events_no_truncate
    BEFORE TRUNCATE ON takedown_events
    FOR EACH STATEMENT EXECUTE FUNCTION sr_forbid_truncate();
CREATE TRIGGER application_audit_no_truncate
    BEFORE TRUNCATE ON application_audit
    FOR EACH STATEMENT EXECUTE FUNCTION sr_forbid_truncate();
CREATE TRIGGER payment_audit_no_truncate
    BEFORE TRUNCATE ON payment_audit
    FOR EACH STATEMENT EXECUTE FUNCTION sr_forbid_truncate();
-- staff_access_log already has staff_access_log_no_truncate (0130).
