-- LNG-SSET-050, EXT-PKG-006: what a server manager chose in Server Settings > Extensions & Themes.
-- The server accent colour (0xRRGGBB) that members' clients lay over their interface; the server-wide theme is
-- a `package_enablements` row (scope 'guild'), so it needs nothing here.
CREATE TABLE guild_appearance (
    guild_id     BIGINT PRIMARY KEY REFERENCES guilds (id) ON DELETE CASCADE,
    accent_color INTEGER CHECK (accent_color BETWEEN 0 AND 16777215),
    updated_by   BIGINT,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The emoji, stickers and sounds a server got from an approved pack, so taking the pack away removes exactly
-- those and nothing the server uploaded itself. The item row is deleted by its own routes too, hence no FK.
CREATE TABLE guild_pack_items (
    guild_id   BIGINT NOT NULL REFERENCES guilds (id) ON DELETE CASCADE,
    package_id BIGINT NOT NULL REFERENCES packages (id) ON DELETE CASCADE,
    item_kind  TEXT NOT NULL CHECK (item_kind IN ('emoji', 'sticker', 'sound')),
    item_id    BIGINT NOT NULL,
    added_by   BIGINT NOT NULL,
    added_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (guild_id, item_kind, item_id)
);
CREATE INDEX guild_pack_items_pkg_idx ON guild_pack_items (guild_id, package_id);
