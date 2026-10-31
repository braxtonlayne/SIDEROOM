-- 1731: BRG-GW-001, reactions that came in over the Discord bridge.
--
-- A Discord reaction is one per person; here the bridge adds it as the integration author, one per emoji. So
-- the bridge remembers which Discord users reacted with what: the reaction is added when the first Discord user
-- reacts and taken away when the last one removes theirs. Only Unicode emoji are mirrored (a custom Discord
-- emoji does not exist here and is shown as text). Rows go with the channel map (the teardown deletes it).
CREATE TABLE bridge_reactions (
    map_id           BIGINT NOT NULL REFERENCES bridge_channel_maps (id) ON DELETE CASCADE,
    sr_message_id    BIGINT NOT NULL,
    emoji            TEXT NOT NULL CHECK (char_length(emoji) BETWEEN 1 AND 64),
    discord_user_id  BIGINT NOT NULL,
    PRIMARY KEY (sr_message_id, emoji, discord_user_id)
);
CREATE INDEX bridge_reactions_map_idx ON bridge_reactions (map_id);
