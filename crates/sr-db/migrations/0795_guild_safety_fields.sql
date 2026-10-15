-- 0795: server-settings backend items of G-36 (P10-SFE-3).
--  * guilds.mfa_level exists since 0010 (0 none, 1 elevated: moderator actions need a recent
--    second factor); this migration adds the other Discord parity fields:
--    system_channel_flags (bit field) and the widget settings.
--  * guild_community gains the DM and spam protection settings (LNG-SSET-052, 054).
ALTER TABLE guilds
    ADD COLUMN system_channel_flags INTEGER NOT NULL DEFAULT 0 CHECK (system_channel_flags BETWEEN 0 AND 63),
    ADD COLUMN widget_enabled       BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN widget_channel_id    BIGINT REFERENCES channels (id) ON DELETE SET NULL;

ALTER TABLE guild_community
    ADD COLUMN dm_spam_filter            BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN explicit_media_filter_dms BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN hide_suspicious_links     BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN warn_new_account_days     INTEGER NOT NULL DEFAULT 0 CHECK (warn_new_account_days BETWEEN 0 AND 365);
