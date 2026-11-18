# Wave 104 drill handoff (D-987, D-989)

Status at 14:30: part 1 written and shaken down, part 2 run against runner's test class, part 3 waits for runner's real classes and a heavy slot.

## Done
- `cli-agents/runner-drill.sh`: one function per "must hold" line (1 to 7, 9, 10; 8 and the real-cell half of 3 run in `real`). `bash -n` clean. Usage in its header.
- `cli-agents/strays.sh` (read only; `bash strays.sh <name>... [--ports <n>...]`; exit 0 nothing, 1 found). Tried on my own worktree: a `node` process naming `p104-drill-marker` started from `p104-drill` was listed (PID, name, 120 chars), the script exited 1, then 0 after I stopped it by PID. The listing leaves out its own PowerShell, its ancestors and any command line naming `strays.sh`.
- The drill's `rm` probe of check 10 runs the checking function in a subshell where `rm` is a stub that only logs, so a faulty function can never delete anything.

## Dummy results (part 2, runner's test class)
See the report `C:/Users/braxt/Desktop/SIDEROOM_wt/p104-drill-report.md` (to be completed).

## Faults sent to runner
- Job names with a hyphen are refused (`name must be letters, digits, underscore`), but the task and brief use `drill-a`. The drill uses `drill_a`.

## STACK-ROUND
Not yet.

## TYPECHECK: not needed

## PROCESSES
To be filled before the reply.
