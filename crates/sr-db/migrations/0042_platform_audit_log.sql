-- 0042: platform-scoped audit log (docs/04 section 7; open questions 46 and 47).
-- Append-only: staff and operator actions that are not guild-scoped. The
-- actor is an account id (1 = the SIDE ROOM system user for CLI actions).
CREATE TABLE platform_audit_log (
    id          BIGINT PRIMARY KEY,
    actor_id    BIGINT NOT NULL REFERENCES accounts (id),
    action      TEXT NOT NULL CHECK (action <> ''),
    target_id   BIGINT,
    note        TEXT NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX platform_audit_log_created_idx ON platform_audit_log (created_at DESC);

CREATE TRIGGER platform_audit_log_append_only
    BEFORE UPDATE OR DELETE ON platform_audit_log
    FOR EACH ROW EXECUTE FUNCTION sr_forbid_mutation();
