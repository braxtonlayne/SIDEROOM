-- 0130: staff appointments, default staff role templates, hash-chained access log.
-- docs/07 sections 3-5, docs/specs/admin-staff-server.md, ADM-STF-001, ADM-STF-008.

-- Appointments (doc 07 section 4). A Staff Server role grants no platform power
-- unless the holder also has an open appointment AND is a Staff Server member.
CREATE TABLE staff_appointments (
    id             BIGINT PRIMARY KEY,
    account_id     BIGINT NOT NULL REFERENCES accounts (id),
    appointed_by   BIGINT NOT NULL REFERENCES accounts (id),
    reason         TEXT NOT NULL DEFAULT '',
    appointed_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at     TIMESTAMPTZ,
    revoked_by     BIGINT REFERENCES accounts (id),
    revoke_reason  TEXT,
    CHECK ((revoked_at IS NULL) = (revoked_by IS NULL))
);
CREATE UNIQUE INDEX staff_appointments_open_uidx
    ON staff_appointments (account_id) WHERE revoked_at IS NULL;

-- Default staff roles as DATA. `sr-cli bootstrap` instantiates these as roles of
-- the Staff Server. Doc 07 names the roles but gives no permission bits: these
-- are PROPOSED DEFAULTS and the owner must confirm them (docs/07 section 4).
-- Bits follow sr_core::PlatformPermissions; the Owner role is bit 63, i.e. the
-- signed bigint minimum.
CREATE TABLE staff_role_templates (
    name                  TEXT PRIMARY KEY,
    position              INTEGER NOT NULL UNIQUE CHECK (position >= 1),
    color                 INTEGER NOT NULL DEFAULT 0 CHECK (color BETWEEN 0 AND 16777215),
    platform_permissions  BIGINT NOT NULL,
    description           TEXT NOT NULL DEFAULT '',
    proposed              BOOLEAN NOT NULL DEFAULT TRUE
);
INSERT INTO staff_role_templates (name, position, color, platform_permissions, description) VALUES
    ('Support',         1, 5793266,  10241,   'Account help: deactivate, reset 2FA, invite credits and codes.'),
    ('App Reviewer',    2, 10181046, 769,     'Approve developer applications and packages.'),
    ('Library Curator', 3, 1752220,  85,      'Review and publish Library content.'),
    ('Media Curator',   4, 15105570, 203,     'Review and publish Cinema content and Live TV.'),
    ('Moderator',       5, 3066993,  7169,    'Accounts, platform bans and the server directory.'),
    ('Senior Staff',    6, 15158332, 1818623, 'Everything except donations, the access log and infrastructure.'),
    ('Owner',           7, 15844367, -9223372036854775808, 'The installation owner. Implicit for the Staff Server owner.');

-- Staff access log (doc 07 section 5, doc 09). Append-only and hash-chained:
-- hash = sha256(prev_hash || canonical(row)); see sr_admin::chain. `seq` is
-- contiguous from 1, assigned under an advisory lock, so a removed row shows up
-- as a gap. `detail` is text (not jsonb) so the hashed bytes are reproducible.
CREATE TABLE staff_access_log (
    seq          BIGINT PRIMARY KEY CHECK (seq >= 1),
    at_us        BIGINT NOT NULL,
    actor_id     BIGINT NOT NULL REFERENCES accounts (id),
    kind         TEXT NOT NULL CHECK (kind IN ('admin_action', 'read', 'private_read', 'denied')),
    action       TEXT NOT NULL CHECK (action <> ''),
    target_kind  TEXT,
    target_id    BIGINT,
    reason       TEXT NOT NULL DEFAULT '',
    ip           TEXT,
    detail       TEXT NOT NULL DEFAULT '',
    prev_hash    BYTEA NOT NULL CHECK (octet_length(prev_hash) = 32),
    hash         BYTEA NOT NULL UNIQUE CHECK (octet_length(hash) = 32),
    -- Reading private content always carries a reason (doc 07 section 5).
    CHECK (kind <> 'private_read' OR reason <> '')
);
CREATE INDEX staff_access_log_actor_idx ON staff_access_log (actor_id, seq DESC);
CREATE INDEX staff_access_log_target_idx ON staff_access_log (target_id, seq DESC) WHERE target_id IS NOT NULL;
CREATE INDEX staff_access_log_action_idx ON staff_access_log (action, seq DESC);

CREATE TRIGGER staff_access_log_append_only
    BEFORE UPDATE OR DELETE ON staff_access_log
    FOR EACH ROW EXECUTE FUNCTION sr_forbid_mutation();
-- TRUNCATE would bypass the row trigger.
CREATE TRIGGER staff_access_log_no_truncate
    BEFORE TRUNCATE ON staff_access_log
    FOR EACH STATEMENT EXECUTE FUNCTION sr_forbid_mutation();
