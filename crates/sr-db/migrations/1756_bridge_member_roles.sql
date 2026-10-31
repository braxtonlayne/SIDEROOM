-- 1756: BRG-SYN-003, every Discord role a bridged member holds, for display only (design 7.2).
--
-- In whole-guild mode the member list groups Discord members under the mapped hoisted roles. These ids are
-- translated through bridge_role_maps for DISPLAY and are never read by any authorization code.
ALTER TABLE bridge_members
    ADD COLUMN discord_role_ids BIGINT[] NOT NULL DEFAULT '{}';
