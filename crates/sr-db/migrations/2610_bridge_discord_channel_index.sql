-- Wave 60 (SEC-AUD-102, D-609): "is this Discord channel mapped anywhere on the instance" is asked on every map
-- creation and on the inbound path. A plain index only: a unique one could fail on an install that already has a
-- Discord channel mapped by two servers.
CREATE INDEX IF NOT EXISTS bridge_channel_maps_discord_channel_idx ON bridge_channel_maps (discord_channel_id);
