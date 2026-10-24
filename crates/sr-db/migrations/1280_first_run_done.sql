-- RDY-ONB-001: whether the person closed or finished the platform first run. Every account that exists when
-- this migration runs has long since found its way around, so they start as done; new accounts start open.
ALTER TABLE accounts ADD COLUMN first_run_done boolean NOT NULL DEFAULT false;
UPDATE accounts SET first_run_done = true;
