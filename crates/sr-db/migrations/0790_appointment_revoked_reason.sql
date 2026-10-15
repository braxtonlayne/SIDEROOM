-- 0790: why an appointment ended (G-23, P10-ADM-5): the member left the Staff Server, was kicked
-- or banned from it, or staff revoked the appointment through Admin or the CLI.
ALTER TABLE staff_appointments
    ADD COLUMN revoked_reason TEXT CHECK (revoked_reason IN ('left', 'kicked', 'revoked'));

UPDATE staff_appointments
   SET revoked_reason = CASE WHEN revoked_by = account_id THEN 'left' ELSE 'revoked' END
 WHERE revoked_at IS NOT NULL AND revoked_reason IS NULL;

ALTER TABLE staff_appointments
    ADD CONSTRAINT staff_appointments_revoked_reason_chk
    CHECK ((revoked_at IS NULL) = (revoked_reason IS NULL));

-- The kick and ban paths of the chat repository set `sr.member_removal = 'kicked'` for their
-- transaction before deleting the member row; every other removal is the member leaving.
CREATE OR REPLACE FUNCTION sr_staff_member_left() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    why TEXT := CASE WHEN current_setting('sr.member_removal', true) = 'kicked'
                     THEN 'kicked' ELSE 'left' END;
BEGIN
    IF EXISTS (SELECT 1 FROM guilds g WHERE g.id = OLD.guild_id AND g.is_staff_guild) THEN
        UPDATE staff_appointments
           SET revoked_at = now(), revoked_by = OLD.user_id,
               revoke_reason = 'left or was removed from the Staff Server',
               revoked_reason = why
         WHERE account_id = OLD.user_id AND revoked_at IS NULL;
        UPDATE appointment_delegations
           SET revoked_at = now(), revoked_by = OLD.user_id
         WHERE account_id = OLD.user_id AND revoked_at IS NULL;
    END IF;
    RETURN OLD;
END $$;
