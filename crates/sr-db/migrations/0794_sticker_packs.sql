-- 0794: sticker packs (G-44, P11-MSG-1). A pack is a set of standard (platform) stickers
-- that every member can send anywhere. No default packs ship: the owner supplies the
-- artwork, and a pack arrives later as a reviewed layer-4 `sticker_pack` package (doc 11
-- section 4) or by an operator inserting rows. Until then `GET /sticker-packs` is empty and
-- the sticker picker shows guild stickers only.
--
-- Pack stickers are ordinary `stickers` rows with `guild_id IS NULL` (standard stickers,
-- migration 0120), so the message-create path already accepts their ids.
CREATE TABLE sticker_packs (
    id                BIGINT PRIMARY KEY,
    name              TEXT NOT NULL CHECK (char_length(name) BETWEEN 2 AND 60),
    description       TEXT CHECK (char_length(description) <= 200),
    -- The sticker shown for the pack in the picker; defaults to its first sticker.
    cover_sticker_id  BIGINT,
    -- Position in the picker, ascending.
    sort_order        INTEGER NOT NULL DEFAULT 0,
    -- The package that installed it, if any (doc 11 section 4).
    package_id        TEXT,
    published         BOOLEAN NOT NULL DEFAULT TRUE,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX sticker_packs_order_idx ON sticker_packs (sort_order, id);

ALTER TABLE stickers
    ADD COLUMN pack_id BIGINT REFERENCES sticker_packs (id) ON DELETE CASCADE,
    ADD CONSTRAINT stickers_pack_is_standard CHECK (pack_id IS NULL OR guild_id IS NULL);
CREATE INDEX stickers_pack_idx ON stickers (pack_id) WHERE pack_id IS NOT NULL;
