-- 1732: BRG-GW-001, the Discord server's members as bridged members.
--
-- Members of the mapped Discord server are shown next to ours in the member list of a bridged channel. They are
-- NOT accounts: no login, no DMs, no permissions, never in authz. This table is only what the list shows: name
-- (the server nickname, else display name, else username), the avatar (a Discord URL, proxied through
-- /cdn/external at view time), the top role's name and colour, whether it is a bot, and the presence when the
-- owner granted the privileged intent. Filled from GUILD_MEMBERS_CHUNK and GUILD_MEMBER_ADD/UPDATE/REMOVE; it
-- goes with the bridge (the teardown deletes the bridges row).
CREATE TABLE bridge_members (
    guild_id         BIGINT NOT NULL REFERENCES bridges (guild_id) ON DELETE CASCADE,
    discord_user_id  BIGINT NOT NULL,
    display_name     TEXT NOT NULL CHECK (char_length(display_name) <= 80),
    avatar_url       TEXT,
    role_id          BIGINT,
    role_name        TEXT CHECK (char_length(role_name) <= 100),
    role_color       INTEGER,
    is_bot           BOOLEAN NOT NULL DEFAULT false,
    presence         TEXT NOT NULL DEFAULT 'offline' CHECK (presence IN ('online', 'idle', 'dnd', 'offline')),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, discord_user_id)
);
CREATE INDEX bridge_members_role_idx ON bridge_members (guild_id, role_id) WHERE role_id IS NOT NULL;
