-- 2080: ARC-GST-001 (wave 43, D-385, D-397, D-399), guest links and the guest sessions that join through them.
--
-- A member of a running Activity whose bundle declares `guest_play` mints a lobby token
-- (`POST /activity-instances/{instance_id}/guest-links`). The token is random and shown once; only its SHA-256
-- digest is stored (like the proxy tokens of 2070). It stops working when the link expires, when the lobby host
-- revokes it, or when the instance ends (the cascade removes the rows; the join query also checks `ended_at`).
--
-- A person without an account joins with `POST /play/{lobby_token}/join` and gets a guest session: a second random
-- token, again stored as a digest only, with a random callsign. The session is short-lived, never longer than its
-- link, and is accepted by the guest socket alone (no REST route takes it).
CREATE TABLE activity_guest_links (
    id           BIGINT PRIMARY KEY,
    instance_id  BIGINT NOT NULL REFERENCES activity_instances (id) ON DELETE CASCADE,
    created_by   BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    lobby_sha256 BYTEA NOT NULL UNIQUE CHECK (octet_length(lobby_sha256) = 32),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL,
    revoked_at   TIMESTAMPTZ
);
CREATE INDEX activity_guest_links_instance_idx ON activity_guest_links (instance_id);

CREATE TABLE activity_guest_sessions (
    token_sha256 BYTEA PRIMARY KEY CHECK (octet_length(token_sha256) = 32),
    link_id      BIGINT NOT NULL REFERENCES activity_guest_links (id) ON DELETE CASCADE,
    callsign     TEXT NOT NULL CHECK (char_length(callsign) BETWEEN 1 AND 40),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL
);
CREATE INDEX activity_guest_sessions_link_idx ON activity_guest_sessions (link_id);
