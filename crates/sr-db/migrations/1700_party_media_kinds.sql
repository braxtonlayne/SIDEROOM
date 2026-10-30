-- 1700: Watch Together media kinds (PRF-UNF-001). A party plays a Cinema title (`cinema`, the existing
-- title_id / asset_id pair), a provider's privacy-enhanced embed (`embed`: YouTube, Vimeo; controlled
-- through the iframe's postMessage commands, no provider script is loaded) or a direct media file in the
-- browser's own player (`direct`). The two new kinds carry no catalog ids: `media_url` is the embed
-- address or the file, `media_provider` names the provider (`youtube`, `vimeo`, `peertube`, `direct`).
-- Nothing is fetched from the address by the server; each member's browser loads it after a click.
ALTER TABLE watch_parties
    ADD COLUMN media_kind     TEXT NOT NULL DEFAULT 'cinema' CHECK (media_kind IN ('cinema', 'embed', 'direct')),
    ADD COLUMN media_url      TEXT CHECK (media_url IS NULL OR char_length(media_url) <= 2048),
    ADD COLUMN media_provider TEXT CHECK (media_provider IS NULL OR char_length(media_provider) <= 32);

-- A party always had media or a channel; external media counts as media. An external kind needs its address,
-- and a cinema party has none.
ALTER TABLE watch_parties DROP CONSTRAINT watch_parties_media_or_channel;
ALTER TABLE watch_parties
    ADD CONSTRAINT watch_parties_media_or_channel
    CHECK (channel_id IS NOT NULL OR title_id IS NOT NULL OR media_kind <> 'cinema'),
    ADD CONSTRAINT watch_parties_media_url_by_kind
    CHECK ((media_kind = 'cinema' AND media_url IS NULL) OR (media_kind <> 'cinema' AND media_url IS NOT NULL));
