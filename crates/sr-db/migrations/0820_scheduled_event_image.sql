-- 0820: scheduled event cover, stage link and empty-channel bookkeeping (G-91, G-93; D-222).
-- `image_key` is the cover hash (object `guild-events/<event id>/<hash>`); `entity_id` is the
-- stage instance of a running stage event (the stage channel id); `empty_since` is when a
-- running voice or stage event's channel last became empty (the clock completes it after 15
-- minutes). All NULL for events that predate this migration.
ALTER TABLE guild_scheduled_events
    ADD COLUMN image_key   TEXT,
    ADD COLUMN entity_id   BIGINT,
    ADD COLUMN empty_since TIMESTAMPTZ;
