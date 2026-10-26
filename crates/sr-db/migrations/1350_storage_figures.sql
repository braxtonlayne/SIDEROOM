-- 1350 (RDY-OBS-002, U-04): the figures of the daily `storage.measure` job, one row per measured bucket.
--
-- `objects` is every object in the bucket, `library` and `cinema` the parts under the Library and Cinema key prefixes.
-- The sr_storage_bytes gauge and Admin > System read these rows; nothing is counted on a scrape or in the 15-minute
-- health pass. `truncated` marks a walk that stopped at its object cap or time budget (the figures are then a lower bound).
CREATE TABLE storage_figures (
    bucket      text PRIMARY KEY,
    bytes       bigint NOT NULL CHECK (bytes >= 0),
    objects     bigint NOT NULL CHECK (objects >= 0),
    truncated   boolean NOT NULL DEFAULT false,
    measured_at timestamptz NOT NULL DEFAULT now()
);
