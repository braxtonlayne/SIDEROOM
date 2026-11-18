# Wave 104 drill handoff (D-987, D-989)

Clock: started 14:13; part 1 written by 14:20; part 2 (dummy checks, runner READY 14:23) run 14:21 to 14:31, 9/9; part 3 14:44 to 14:49; report and handoff 14:55.
Report: `C:/Users/braxt/Desktop/SIDEROOM_wt/p104-drill-report.md` (ten lines, memory table, proposed costs, verdict: **the re-shoot can go through the runner**).

## Done
- `cli-agents/runner-drill.sh` (outside the repository: no commit hash; `bash -n` clean): one function per "must hold" line; `real` runs part 3.
- `cli-agents/strays.sh` (read only; exit 0 nothing, 1 found). Tried on this worktree: a node process naming `p104-drill-marker`, started from `p104-drill`, was listed with PID, name and command line, exit 1; after I stopped it by PID, exit 0.
- The folder `cli-agents/run-drill/` is left in place with its logs, `evidence/` and `part3/samples.csv`.

## Results
Checks 1 to 10 all PASS (see the report). Proposals: `COST_SHOOT=1200`, `RECENT_S=15`, `RESERVE` keep 700; build/tsc "not measured, keep 1 500 and 1 900".

## Faults sent to runner
None from a failed check. Noted: job names with a hyphen were refused (cured by runner). Drill-side: `rm` probe now stubs `rm`.

## STACK-ROUND
Slot held 14:44 to 14:48 (`heavy.sh status` read "drill 14:44" before and "free" after release). Three real jobs: one solo, then two at once.

## TYPECHECK: not needed

## PROCESSES
Filled below at the end.
