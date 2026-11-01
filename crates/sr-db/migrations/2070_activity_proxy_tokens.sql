-- 2070: ARC-SBX-001 (wave 42 integration, D-399), the instance's own credential for URL-mapping writes.
--
-- A game runs in an opaque-origin sandbox, so its writes reach the activity proxy with `Origin: null`, which any
-- sandboxed page on the web can also send. The proxy therefore accepts `Origin: null` on a write under a URL
-- mapping only with a proxy token: a random value the app shell mints for one participant of one running
-- instance (`POST /activity-instances/{instance_id}/proxy-token`) and hands to the frame in `ready`. Only the
-- SHA-256 digest is stored. A token works while its instance runs and its member has not left; minting again
-- replaces the member's previous token.
CREATE TABLE activity_proxy_tokens (
    instance_id  BIGINT NOT NULL REFERENCES activity_instances (id) ON DELETE CASCADE,
    user_id      BIGINT NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    token_sha256 BYTEA NOT NULL UNIQUE CHECK (octet_length(token_sha256) = 32),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (instance_id, user_id)
);
