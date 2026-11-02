-- 2200: ACT-LND-002 (wave 46, D-388), deterministic Activity host reassignment.
--
-- `host_id` is the member who is the instance's host now: the only writer of `host:*` keys and the receiver of inputs.
-- NULL means the starter (`started_by`) is still the host, so every running instance keeps its meaning. When the host
-- leaves an instance whose reviewed bundle declares the `host_handoff` capability, the server moves the host to the
-- longest-connected remaining member (earliest `joined_at`, ties by user id); guests are never participants, so they are
-- never chosen. Without the capability the instance ends when its host leaves, as before (P36-ACT-1).
ALTER TABLE activity_instances ADD COLUMN host_id BIGINT;
