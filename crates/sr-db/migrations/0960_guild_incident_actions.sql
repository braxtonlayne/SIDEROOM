-- 0960: Security Actions are Discord's incident actions (P14-LNG-2, D-243).
--  * invites_disabled_until / dms_disabled_until: timed pauses (at most 24 hours ahead when set).
--    They sit next to the indefinite invites_paused switch, which stays its own setting.
--  * raid_detected_at: when the join-rate alert last fired.
--  * raid_auto_pause_invites: whether the alert pauses invites for 24 hours by itself.
ALTER TABLE guild_community
    ADD COLUMN invites_disabled_until   TIMESTAMPTZ,
    ADD COLUMN dms_disabled_until       TIMESTAMPTZ,
    ADD COLUMN raid_detected_at         TIMESTAMPTZ,
    ADD COLUMN raid_auto_pause_invites  BOOLEAN NOT NULL DEFAULT TRUE;
