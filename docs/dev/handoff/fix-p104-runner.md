(READY) test class; (READY) real classes

# fix-p104-runner (wave 104, D-986): job runner

Scripts (outside the repository; cli-agents is not a git repo, so no hash for them): `cli-agents/job.sh`, `runner.sh`, `lane.sh`, `runlib.sh` (shared), `world.sh`/`cell.sh` (extended; originals `*.p103`). Full detail: `C:/Users/braxt/Desktop/SIDEROOM_wt/p104-runner-report.md`.

## Done
- Test class: 14:23 READY. drill's part 2: 9 of 9.
- Hyphen in names: accepted now (`drill-a` works); the message names the rule.
- Real classes (prepare, build, tsc, shoot base/after, `--wt lanes`, lane.sh) written; one round on a real cell worked (below).
- twin's FAULT (cell.sh down killed the wrong PID): cell.sh now records `<cell>/ports`, `down` stops each port's listener by its own PID and warns if a port still listens; the runner's `lane_clean` does the same (`kill_listener`). The winpid is read after the server is up and a cygwin pid is never used as a Windows pid.

## STACK-ROUND 1
14:32 to 14:37 (acquire exit 0, status showed `runner 14:32`; released, status `free`). prepare 1 min (build + seed, no FAIL); build job 16 s; tsc 25 s; shoot base 29 s (p102-logs spec, 1 passed); shoot after 29 s (1 passed, after the build record matched). Queue waits 2, 19, 24, 54 s. After: no sr_p104_j database, ports 884x/954x closed, password found in no file of `run/`.
A first prepare at 14:33 left the seed server running (cygwin pid recorded before exec): fixed (above), stray stopped by its PID, prepare rerun.

## TYPECHECK
not needed.

## PROCESSES
Process listing for `*p104-runner*`: no line. Listeners on 8841/8842/9541/9542 and the seed port 8849/9549: no line. `runner.sh status`: runner NOT running. `heavy.sh status` at 14:38: `loungeshots 14:37` only, no hold in my name. No `sr_p104_j*` database left; `sr_p104_world`, its state folder, `p104-base-dist` and `run/conf` kept for the commander. `p104-cell-bin` untouched (not deleted). Scratch run folder used for dummy tests is in the session scratchpad.
