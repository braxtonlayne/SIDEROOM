-- 2930: the Workers page counts each worker's finished and failed jobs of the last 24 hours (D-705, SEC-AUD-216).
-- Nothing indexed worker_id, and jobs are never deleted, so the counts would scan the whole table on every list.
-- Partial: only finished rows (succeeded or failed) have a finish time worth reading.
CREATE INDEX transcode_jobs_worker_done ON transcode_jobs (worker_id, finished_at)
    WHERE state IN ('succeeded', 'failed') AND worker_id IS NOT NULL;
