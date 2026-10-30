-- UTL-CRD-001: Cinema ticket and game-night invite cards. A card is a system-user message (type 201) with
-- buttons; this table is its state. `id` is the message id, so the buttons (`card:claim:<id>`) name the card
-- and the message can be rewritten from the row. Seats are claimed under a row lock (see the repository);
-- reminders are marked per claim and per window so a restart never sends one twice.
CREATE TABLE invite_cards (
    id           BIGINT PRIMARY KEY,
    channel_id   BIGINT NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    guild_id     BIGINT,
    kind         TEXT NOT NULL CHECK (kind IN ('cinema', 'live_tv', 'game_night')),
    title        TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 100),
    note         TEXT NOT NULL DEFAULT '' CHECK (char_length(note) <= 500),
    starts_at    TIMESTAMPTZ NOT NULL,
    duration_min INTEGER NOT NULL DEFAULT 120 CHECK (duration_min BETWEEN 15 AND 720),
    -- The IANA zone the host picked the time in (display only; every instant is stored in UTC).
    timezone     TEXT NOT NULL DEFAULT 'UTC' CHECK (char_length(timezone) BETWEEN 1 AND 64),
    seats        INTEGER NOT NULL CHECK (seats BETWEEN 1 AND 250),
    -- Cinema titles only: what the watch-party hand-off starts.
    title_id     BIGINT,
    -- An app path the card links to (a Live TV slot, a title page); relative, checked by the API.
    link_path    TEXT CHECK (link_path IS NULL OR char_length(link_path) <= 200),
    created_by   BIGINT REFERENCES accounts (id) ON DELETE SET NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    canceled_at  TIMESTAMPTZ,
    -- Set by the job when the start time passes (the card gains its watch-party button) and when the card closes.
    started_at   TIMESTAMPTZ,
    closed_at    TIMESTAMPTZ
);
CREATE INDEX invite_cards_channel_idx ON invite_cards (channel_id, id DESC);
CREATE INDEX invite_cards_open_start_idx ON invite_cards (starts_at) WHERE closed_at IS NULL AND canceled_at IS NULL;

CREATE TABLE invite_card_claims (
    card_id         BIGINT NOT NULL REFERENCES invite_cards (id) ON DELETE CASCADE,
    user_id         BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    claimed_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- The member's choice at claim time: a DM 1 h and 10 min before the start.
    remind          BOOLEAN NOT NULL DEFAULT TRUE,
    reminded_1h_at  TIMESTAMPTZ,
    reminded_10m_at TIMESTAMPTZ,
    PRIMARY KEY (card_id, user_id)
);
CREATE INDEX invite_card_claims_user_idx ON invite_card_claims (user_id);
