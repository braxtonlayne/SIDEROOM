-- 1001: The chat card of a watch party (WP-BE-008). The system user posts one message of type 200 in the
-- party's bound channel when the party starts and rewrites it while the party runs and when it ends. This
-- table remembers which message belongs to which party, so a restart or another node can still find it.
-- No foreign keys: the party and the message live in other modules and either may be deleted first.
CREATE TABLE watch_party_cards (
    party_id   BIGINT PRIMARY KEY,
    channel_id BIGINT NOT NULL,
    message_id BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX watch_party_cards_created_idx ON watch_party_cards (created_at);
