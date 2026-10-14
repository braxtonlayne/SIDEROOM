-- 0650: platform admin backend, phase 10.
-- ADM-STF-001 (delegations), ADM-STF-004 (appointment offers), ADM-STF-005 (instant revocation
-- on leaving), ADM-PLT-009 (announcements), ADM-ALR-001 (alert store).
-- docs/07 sections 3-4, docs/specs/admin-staff-server.md sections 3-4.

-- The Owner's delegation of appointment rights (D-132). While a delegation is open the
-- delegate's platform standing includes APPOINT_STAFF, and what they may hand out is
-- capped by `max_permissions` (a ceiling that never includes the Owner-only bits).
CREATE TABLE appointment_delegations (
    id               BIGINT PRIMARY KEY,
    account_id       BIGINT NOT NULL REFERENCES accounts (id),
    granted_by       BIGINT NOT NULL REFERENCES accounts (id),
    max_permissions  BIGINT NOT NULL CHECK (max_permissions > 0),
    reason           TEXT NOT NULL DEFAULT '',
    granted_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at       TIMESTAMPTZ,
    revoked_by       BIGINT REFERENCES accounts (id),
    CHECK ((revoked_at IS NULL) = (revoked_by IS NULL))
);
CREATE UNIQUE INDEX appointment_delegations_open_uidx
    ON appointment_delegations (account_id) WHERE revoked_at IS NULL;

-- An offer to join the staff. The appointee answers the system DM; only an accepted offer
-- creates the `staff_appointments` row (doc 07 section 4).
CREATE TABLE staff_appointment_offers (
    id             BIGINT PRIMARY KEY,
    account_id     BIGINT NOT NULL REFERENCES accounts (id),
    offered_by     BIGINT NOT NULL REFERENCES accounts (id),
    role_ids       BIGINT[] NOT NULL CHECK (cardinality(role_ids) >= 1),
    reason         TEXT NOT NULL DEFAULT '',
    state          TEXT NOT NULL DEFAULT 'pending'
                   CHECK (state IN ('pending', 'accepted', 'declined', 'cancelled', 'expired')),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at     TIMESTAMPTZ NOT NULL,
    responded_at   TIMESTAMPTZ,
    dm_channel_id  BIGINT,
    dm_message_id  BIGINT,
    CHECK ((state = 'pending') = (responded_at IS NULL))
);
CREATE UNIQUE INDEX staff_appointment_offers_pending_uidx
    ON staff_appointment_offers (account_id) WHERE state = 'pending';
CREATE INDEX staff_appointment_offers_created_idx ON staff_appointment_offers (created_at DESC);

-- Leaving, or being removed from, the Staff Server ends the appointment and any
-- delegation for good. API access already stops at once (the platform standing is
-- evaluated on every request and needs Staff Server membership); this keeps the
-- appointment from silently coming back if the member row is ever re-created.
-- `revoked_by` is the member themselves because the database does not know who kicked.
CREATE FUNCTION sr_staff_member_left() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF EXISTS (SELECT 1 FROM guilds g WHERE g.id = OLD.guild_id AND g.is_staff_guild) THEN
        UPDATE staff_appointments
           SET revoked_at = now(), revoked_by = OLD.user_id,
               revoke_reason = 'left or was removed from the Staff Server'
         WHERE account_id = OLD.user_id AND revoked_at IS NULL;
        UPDATE appointment_delegations
           SET revoked_at = now(), revoked_by = OLD.user_id
         WHERE account_id = OLD.user_id AND revoked_at IS NULL;
    END IF;
    RETURN OLD;
END $$;
CREATE TRIGGER guild_members_staff_left
    AFTER DELETE ON guild_members
    FOR EACH ROW EXECUTE FUNCTION sr_staff_member_left();

-- Platform announcements (ADM-PLT-009): a banner every signed-in user sees between
-- `starts_at` and `ends_at`, optionally also sent as a DM from the system user.
CREATE TABLE platform_announcements (
    id          BIGINT PRIMARY KEY,
    title       TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 120),
    body        TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 2000),
    severity    TEXT NOT NULL DEFAULT 'info' CHECK (severity IN ('info', 'warning', 'critical')),
    send_dm     BOOLEAN NOT NULL DEFAULT FALSE,
    created_by  BIGINT NOT NULL REFERENCES accounts (id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    starts_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    ends_at     TIMESTAMPTZ,
    ended_at    TIMESTAMPTZ,
    CHECK (ends_at IS NULL OR ends_at > starts_at)
);
CREATE INDEX platform_announcements_active_idx ON platform_announcements (starts_at, ends_at)
    WHERE ended_at IS NULL;

-- Staff alerts (ADM-ALR-001): what `#alerts` showed, kept for the dashboard's "recent alerts"
-- and the acknowledge button. Not append-only: acknowledging updates a row.
CREATE TABLE staff_alerts (
    id          BIGINT PRIMARY KEY,
    kind        TEXT NOT NULL CHECK (kind <> ''),
    detail      TEXT NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    acked_by    BIGINT REFERENCES accounts (id),
    acked_at    TIMESTAMPTZ,
    CHECK ((acked_by IS NULL) = (acked_at IS NULL))
);
CREATE INDEX staff_alerts_created_idx ON staff_alerts (created_at DESC);
CREATE INDEX staff_alerts_kind_idx ON staff_alerts (kind, created_at DESC);
