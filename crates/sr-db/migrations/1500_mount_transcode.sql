-- 1500 (BYOS-CNV-003): the transcoder as an optional worker that writes into a member's own bucket.
--
-- * `worker_tokens.scope`: `instance` (every token so far: the operator's workers, which may claim any job
--   kind) or `mount:<id>` (a token a member runs on their own machine, the BYOC variant). A mount-scoped token
--   claims `mount_transcode` jobs of that one mount and nothing else; the claim query enforces it.
-- * `transcode_jobs.kind` grows `mount_transcode`. Such a job has no `video_assets` row (a mount's catalog rows
--   are built by the indexer, not by an upload), so its `asset_id` is 0 and the foreign key is dropped; instead
--   `mount_id` names the mount whose bucket holds the source and receives the outputs. `source_key` is the
--   mount-relative object key.
-- * `requested_by` is the account that queued the job (the mount owner, or staff for a guild mount).
ALTER TABLE worker_tokens
    ADD COLUMN scope TEXT NOT NULL DEFAULT 'instance'
        CHECK (scope = 'instance' OR scope ~ '^mount:[0-9]{1,19}$');

ALTER TABLE transcode_jobs DROP CONSTRAINT transcode_jobs_kind_check;
ALTER TABLE transcode_jobs ADD CONSTRAINT transcode_jobs_kind_check
    CHECK (kind IN ('proxy', 'ladder', 'remux', 'mount_transcode'));
ALTER TABLE transcode_jobs DROP CONSTRAINT transcode_jobs_asset_id_fkey;
ALTER TABLE transcode_jobs
    ADD COLUMN mount_id     BIGINT,
    ADD COLUMN requested_by BIGINT REFERENCES accounts (id) ON DELETE SET NULL;
ALTER TABLE transcode_jobs ADD CONSTRAINT transcode_jobs_mount_shape
    CHECK ((kind = 'mount_transcode') = (mount_id IS NOT NULL));
CREATE INDEX transcode_jobs_mount ON transcode_jobs (mount_id, id DESC) WHERE mount_id IS NOT NULL;
