-- 0792: per-server profile extras (G-52, P11-INT-4). D-044 makes the per-server cosmetics free:
-- a member may wear different pronouns, profile theme colours, profile effect and avatar
-- decoration in each server. NULL means "use the account's own".
ALTER TABLE guild_members
    ADD COLUMN pronouns          TEXT,
    ADD COLUMN theme_primary     INTEGER CHECK (theme_primary BETWEEN 0 AND 16777215),
    ADD COLUMN theme_accent      INTEGER CHECK (theme_accent BETWEEN 0 AND 16777215),
    ADD COLUMN profile_effect    TEXT,
    ADD COLUMN avatar_decoration TEXT;
