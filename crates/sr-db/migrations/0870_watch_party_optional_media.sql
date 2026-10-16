-- 0870: a watch party room may be empty (D-227, H-02). A persistent party bound to a voice channel can have
-- no media: `title_id` and `asset_id` become nullable (the other media columns already have defaults or are
-- ignored while no title is set). A party without a title must be bound to a channel.
ALTER TABLE watch_parties ALTER COLUMN title_id DROP NOT NULL;
ALTER TABLE watch_parties ALTER COLUMN asset_id DROP NOT NULL;
ALTER TABLE watch_parties ALTER COLUMN title_name SET DEFAULT '';
ALTER TABLE watch_parties
    ADD CONSTRAINT watch_parties_media_or_channel
    CHECK (channel_id IS NOT NULL OR title_id IS NOT NULL);
