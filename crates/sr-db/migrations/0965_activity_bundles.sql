-- J-12 (D-246, D-247): an Activity is an uploaded bundle (a zip of static files) that sr-server hosts.
-- Staff review a bundle by its SHA-256; the proxy serves only the reviewed bundle. The entry URL survives
-- only as `dev_entry_url`, served live for a `development` application. Forward-only: the entry URL and the
-- entry document hashes of 0930 go, and every Activity returns to `draft` until a bundle is approved.
-- J-15 (D-248): an instance may run in a text channel.
CREATE TABLE application_activity_bundles (
    id             BIGINT PRIMARY KEY,
    application_id BIGINT NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    -- SHA-256 (hex) of the uploaded zip.
    sha256         TEXT NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    -- Total unpacked size and number of files.
    byte_size      BIGINT NOT NULL CHECK (byte_size >= 0),
    entry_count    INTEGER NOT NULL CHECK (entry_count >= 0),
    -- [{"path", "size", "content_type"}] for the portal and the review queue.
    manifest       JSONB NOT NULL DEFAULT '[]'::jsonb,
    uploaded_by    BIGINT,
    uploaded_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    state          TEXT NOT NULL DEFAULT 'draft'
                   CHECK (state IN ('draft', 'pending', 'approved', 'rejected', 'superseded')),
    reviewed_by    BIGINT,
    reviewed_at    TIMESTAMPTZ,
    note           TEXT
);
-- One live row per (application, hash): a superseded bundle may be uploaded again.
CREATE UNIQUE INDEX application_activity_bundles_live_idx
    ON application_activity_bundles (application_id, sha256) WHERE state <> 'superseded';
CREATE INDEX application_activity_bundles_state_idx
    ON application_activity_bundles (application_id, state, uploaded_at DESC);
-- Where a bundle may be pending or approved at most once per application.
CREATE UNIQUE INDEX application_activity_bundles_pending_idx
    ON application_activity_bundles (application_id) WHERE state = 'pending';
CREATE UNIQUE INDEX application_activity_bundles_approved_idx
    ON application_activity_bundles (application_id) WHERE state = 'approved';

ALTER TABLE application_activities
    ADD COLUMN reviewed_bundle_id BIGINT REFERENCES application_activity_bundles (id) ON DELETE SET NULL,
    ADD COLUMN dev_entry_url TEXT;

UPDATE application_activities c SET dev_entry_url = c.entry_url
  FROM applications a
 WHERE a.id = c.application_id AND a.release_state = 'development' AND c.entry_url IS NOT NULL;
UPDATE application_activities
   SET review_state = 'draft', review_note = NULL, reviewed_by = NULL, reviewed_at = NULL;

ALTER TABLE application_activities
    DROP COLUMN entry_url,
    DROP COLUMN pinned_sha256,
    DROP COLUMN reviewed_sha256;

ALTER TABLE activity_instances
    ADD COLUMN channel_type TEXT NOT NULL DEFAULT 'voice' CHECK (channel_type IN ('voice', 'text'));
