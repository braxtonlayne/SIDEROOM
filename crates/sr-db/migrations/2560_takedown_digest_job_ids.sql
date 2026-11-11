-- 2560 (SEC-AUD-062, D-592): a mount that comes back (`disabled` -> `active`) or is deleted while disabled queues the
-- digest jobs of its waiting takedown items from SQL, inside the statement that changed the mount. The repository
-- that does it has no id source, so the ids come from this sequence. It starts at the singleton limit (2^40) so a
-- job id is never taken for a recurring singleton, and stays far below any Snowflake, so the two cannot meet.
CREATE SEQUENCE takedown_digest_job_seq START WITH 1099511627776 MINVALUE 1099511627776 NO CYCLE;
