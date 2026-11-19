# SIDE ROOM job runner (cli-agents)

Builders submit jobs; one runner takes them in ticket order and runs them when memory allows. State lives in a run folder
(`SR_RUN_DIR`, default `<this folder>/run`). Every script finds its own folder, so a clone runs itself.

## Commands
- `job.sh submit <name> <class> ...` prints the job id. `job.sh wait <id> 100` (0 done, 1 failed, 2 killed/cancelled/unknown, 75 still going: call again; always pass 100, a tool call ends at 120 s).
- `job.sh status` (runner alive, free memory, running, queued, the last five `hold` lines) / `cancel <id>` / `log <id>` (prints the log's path).
- `runner.sh start | stop [--now] | status | prepare <base worktree>`; `lane.sh sync|status`; `strays.sh <name>... [--ports n...]`; `runner-drill.sh [n...|real]`.
- `memwatch.sh <out.csv> [seconds]`: samples free memory and the working set of node, sr-server, Playwright's Chromium, postgres, claude and tsc/vitest/eslint.

## Classes
| class | what | cost (conf) |
|---|---|---|
| `tsc` | web typecheck of the job's worktree, at most `TSC_MAX` at once | `COST_TSC` |
| `build` | web bundle of the worktree; records the build for `shoot after`, at most `BUILD_MAX` at once | `COST_BUILD` |
| `vitest -- <files>` | vitest in `apps/web` | `COST_VITEST` |
| `shoot base\|after [--fresh] [--dist d] [--bin folder] [--env K=V] [--server-env K=V] -- <playwright args>` | one cell (server on a lane's port and a cloned database), the spec, then everything stopped | `COST_SHOOT` |
| `cargo -- build\|test\|clippy\|fmt\|run [args]` | cargo in the worktree, `CARGO_TARGET_DIR=$W/<WAVE>-<name>-target`, `CARGO_PROFILE_DEV_DEBUG=0`, `--offline` (not for fmt). Starts only when nothing else runs; nothing starts while it runs. Own limit `CARGO_LIMIT_S`. | `COST_CARGO` |
| `test <mb> <seconds> [rc]` | a dummy for drills | `<mb>` |

`--bin <folder>`: the job's server and cli come from `<folder>` (under `SIDEROOM_wt/`, holding `sr-server.exe` and `sr-cli.exe`, never `p80-base-bin` or `p92-base-bin`); refused at submit otherwise, checked again at run time.

## Conf keys (`<run folder>/conf`, plain `KEY=VALUE`, read on every pass), with wave 106's values
`WAVE=p106 CELL_BIN BASE_DIST TEMPLATE_DB DB_PREFIX LANES="1 2 3" PORT_BASE=8840 METRICS_BASE=9540`
`COST_SHOOT=1200 COST_BUILD=1500 COST_TSC=3300 COST_VITEST=1200 COST_CARGO=3000 RESERVE=700 RECENT_S=15 HOL_S=240`
`JOB_LIMIT_S=600 CARGO_LIMIT_S=1500 TSC_MAX=1 BUILD_MAX=1 LANE_WTS="<lane worktrees>" STOP_AT="18:15"`
`SR_CARGO_CMD` (environment of the runner, not conf): another command in place of `cargo`; the drill's stub.

## Why a job waits
`runner.log` has one line per job and reason when the reason first holds or changes: `hold <id> mem avail= recent= reserve= cost=`, `lane`, `wt <worktree>`, `cap tsc|build`, `cargo`, `hol <head id>`, `stop_at`.

## Taking a new version into cli-agents safely
1. `bash cli-agents/runner.sh stop` (waits for running jobs; `--now` kills them). Check the runner's Windows PID is gone.
2. `git -C cli-agents pull ../cli-agents-next <branch>` (a fast-forward from the clone). Never edit files of a running runner.
3. `bash cli-agents/runner-drill.sh` (dummy checks, its own run folder `cli-agents/run-drill`). All must pass.
4. `bash cli-agents/runner.sh start`, one real `test` job, then one real shoot on the cloned world.
Go back: `git -C cli-agents reset --hard <previous commit>`.
