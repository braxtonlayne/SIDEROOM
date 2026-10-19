-- 1080: a server can hide the default (platform) sticker packs from its sticker picker (M-03, D-289). One row per
-- server that ever changed the setting; a server without a row shows the default packs.
CREATE TABLE guild_sticker_settings (
    guild_id           BIGINT PRIMARY KEY REFERENCES guilds (id) ON DELETE CASCADE,
    hide_default_packs BOOLEAN NOT NULL DEFAULT FALSE,
    updated_by         BIGINT,
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
