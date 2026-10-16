-- 0874: how a member joined (D-240, H-37). Every join path sets these; rows older than this
-- migration stay null and read as "unknown". `join_ref` is the inviter (invite), the application
-- (application) or the installing user (bot); `join_invite_code` the invite or vanity code.
ALTER TABLE guild_members
    ADD COLUMN join_source TEXT
        CHECK (join_source IN ('invite','vanity','directory','application','creator','staff','bot')),
    ADD COLUMN join_invite_code TEXT,
    ADD COLUMN join_ref BIGINT;
