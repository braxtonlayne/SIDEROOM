-- 0796: server-side activity sharing (G-58, D-190). The Activity Privacy toggles live in the
-- user settings blob under `activity`: share_cinema, share_library, share_tv, hide_age_restricted
-- and paused (all booleans). Absent keys mean the D-120 defaults: sharing on, adult titles hidden,
-- not paused. The scoped aggregates, the watching presence and the profile read them through
-- these functions so every reader agrees.

-- Whether `uid` shares what they do in `section` ('cinema', 'library' or 'tv'): the section toggle
-- is not off and the history is not paused.
CREATE FUNCTION activity_shares(uid BIGINT, section TEXT) RETURNS BOOLEAN
LANGUAGE sql STABLE AS $$
    SELECT COALESCE(
               (SELECT settings #>> ARRAY['activity', 'share_' || section]
                  FROM user_settings WHERE user_id = uid), 'true') <> 'false'
       AND COALESCE(
               (SELECT settings #>> '{activity,paused}'
                  FROM user_settings WHERE user_id = uid), 'false') <> 'true'
$$;

-- Whether `uid` keeps age-restricted titles out of their shared activity (default yes).
CREATE FUNCTION activity_hides_restricted(uid BIGINT) RETURNS BOOLEAN
LANGUAGE sql STABLE AS $$
    SELECT COALESCE(
               (SELECT settings #>> '{activity,hide_age_restricted}'
                  FROM user_settings WHERE user_id = uid), 'true') <> 'false'
$$;

-- `cinema.share_watching` (0 off, 1 friends, 2 everyone) now also reads `activity`: a member who
-- turned cinema sharing off, or paused their history, shares with nobody.
CREATE OR REPLACE FUNCTION cinema_share_level(uid BIGINT) RETURNS SMALLINT
LANGUAGE sql STABLE AS $$
    SELECT CASE
        WHEN NOT activity_shares(uid, 'cinema') THEN 0
        ELSE CASE (SELECT settings #>> '{cinema,share_watching}' FROM user_settings WHERE user_id = uid)
            WHEN 'off' THEN 0
            WHEN 'everyone' THEN 2
            ELSE 1
        END
    END::smallint
$$;
