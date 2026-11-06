-- 2360: PeerTube parties (PRF-UNF-001, D-515/D-516). A party started from a PeerTube link plays the file or HLS
-- playlist the instance's public API names, as `direct` media. `media_page_url` keeps the pasted link (the card's
-- title and link); `media_format` says how the browser loads `media_url` (`file` or `hls`) and is set only for
-- `direct` media.
ALTER TABLE watch_parties
    ADD COLUMN media_page_url TEXT CHECK (media_page_url IS NULL OR char_length(media_page_url) <= 2048),
    ADD COLUMN media_format   TEXT CHECK (media_format IS NULL OR media_format IN ('file', 'hls'));

ALTER TABLE watch_parties
    ADD CONSTRAINT watch_parties_media_format_by_kind
    CHECK (media_format IS NULL OR media_kind = 'direct');
