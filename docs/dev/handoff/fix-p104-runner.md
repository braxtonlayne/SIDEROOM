(READY) test class

# fix-p104-runner (wave 104, D-986): job runner

Scripts (outside the repository): `cli-agents/job.sh`, `runner.sh`, `lane.sh`, `runlib.sh` (shared functions), `world.sh`/`cell.sh` (extended; originals kept as `*.p103`).
Test class tried by hand in a scratch run folder (14:13 to 14:23): ticket order, gate with fake-avail, pass-over, HOL_S, same-worktree serialisation, limit kill, wait 0/1/2/75, cancel, runner killed and restarted (orphan 137), STOP_AT, password filter.

## Status
- 14:23 test class READY; real classes next.
