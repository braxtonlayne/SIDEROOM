-- 0670: Web Push (LNG-NTF-003). One row per browser subscription; the VAPID application
-- server key pair is generated on first use and kept in a single-row table.
CREATE TABLE push_subscriptions (
    id           BIGINT PRIMARY KEY,
    user_id      BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    -- The push service URL is the subscription's identity; re-subscribing replaces the row.
    endpoint     TEXT NOT NULL UNIQUE CHECK (char_length(endpoint) <= 2048),
    -- Base64url, the browser's receiver key and authentication secret.
    p256dh       TEXT NOT NULL CHECK (char_length(p256dh) BETWEEN 1 AND 256),
    auth         TEXT NOT NULL CHECK (char_length(auth) BETWEEN 1 AND 64),
    device_name  TEXT NOT NULL DEFAULT '' CHECK (char_length(device_name) <= 100),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_push_at TIMESTAMPTZ
);
CREATE INDEX push_subscriptions_user_idx ON push_subscriptions (user_id, created_at);

CREATE TABLE push_vapid_key (
    singleton   BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    -- The 32 secret bytes of the P-256 key, sealed with the server secret by the caller
    -- when one is configured; never returned by any API.
    secret      BYTEA NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
