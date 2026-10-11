-- 0031: guild audit log (append-only). docs/04 section 4, LNG-SSET-056.
-- No FK to guilds: entries must survive until the retention job removes them, and
-- the delete trigger below would otherwise block ON DELETE CASCADE.
CREATE TABLE audit_log_entries (
    id          BIGINT PRIMARY KEY,
    guild_id    BIGINT NOT NULL,
    actor_id    BIGINT NOT NULL,
    target_id   BIGINT,
    action      SMALLINT NOT NULL,
    changes     JSONB NOT NULL DEFAULT '[]'::jsonb,
    options     JSONB,
    reason      TEXT CHECK (char_length(reason) <= 512),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX audit_log_guild_idx ON audit_log_entries (guild_id, id DESC);
CREATE INDEX audit_log_actor_idx ON audit_log_entries (guild_id, actor_id, id DESC);
CREATE INDEX audit_log_action_idx ON audit_log_entries (guild_id, action, id DESC);
CREATE INDEX audit_log_created_idx ON audit_log_entries (created_at);

CREATE FUNCTION sr_audit_log_immutable() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'UPDATE' THEN
        RAISE EXCEPTION 'audit_log_entries is append-only';
    END IF;
    -- DELETE is allowed only inside the retention job, which sets this per transaction.
    IF coalesce(current_setting('sr.audit_purge', true), '') <> 'on' THEN
        RAISE EXCEPTION 'audit_log_entries is append-only';
    END IF;
    RETURN OLD;
END;
$$;

CREATE TRIGGER audit_log_no_update
    BEFORE UPDATE ON audit_log_entries
    FOR EACH ROW EXECUTE FUNCTION sr_audit_log_immutable();
CREATE TRIGGER audit_log_no_delete
    BEFORE DELETE ON audit_log_entries
    FOR EACH ROW EXECUTE FUNCTION sr_audit_log_immutable();
