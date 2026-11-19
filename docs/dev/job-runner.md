# The job runner

Wave 108 (D-1027). The scripts are in `scripts/dev/runner/`, a copy of the runner's own repository at the commit named in `scripts/dev/runner/SOURCE`.
They name this machine's paths (`/c/Users/braxt/Desktop/SIDEROOM_wt`, `%LOCALAPPDATA%\sideroom`): they are an archive and a reference, not something CI runs.
The running copy lives outside the repository, in `SIDEROOM_wt/cli-agents/`, because a worktree comes and goes and the runner must not.

## What a job is, and why

Until wave 103 every builder held a lock, started its own server, Vite and database, and waited for the lock by hand. That cost up to an hour a wave
in faults (a lock read wrong, a `timeout` around `acquire`, a sleep until a planned slot), and seven web typechecks at once could take the machine
down. A **job** replaces all of it: a builder submits a request and waits for its result, and never starts a server, Vite, a database or Playwright.
One **runner** (a bash script the commander starts, no agent) takes jobs in ticket order and admits each when the memory free at that moment covers its
class cost. A **cell** is the smallest thing that can shoot one spec: one `sr-server.exe` serving a built bundle, on a lane's port, on a database
**cloned** from the wave's seeded sweep world (`CREATE DATABASE ... TEMPLATE`, about a second, instead of a 30 to 40 s seed). Each job has its own
database and its own server, so no spec disturbs another and each starts with an empty rate limiter.

## What a builder uses

```
J=C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh
bash $J submit <name> tsc                      # web typecheck of p<wave>-<name>
bash $J submit <name> build                    # builds that worktree's bundle (records the build)
bash $J submit <name> shoot base  --env SR_..._ON=1 -- tests/<spec>.spec.ts
bash $J submit <name> shoot after --env SR_..._ON=1 -- tests/<spec>.spec.ts
bash $J submit <name> vitest -- <test files>
bash $J wait <id> 100     # 0 done | 1 failed | 2 killed, cancelled or unknown | 75 still queued or running: call it again
bash $J status | cancel <id> | log <id>
```

Always `wait <id> 100`: a tool call ends at 120 s, and the old default of 150 was pushed to the background. Exit 75 is not done; call `wait` again, then read the log
(`log <id>` prints its path) and the `rc` file. `submit` also takes `--wt <worktree>|lanes`, `--fresh`, `--dist <dir>`, `--bin <folder>`, `--env K=V` (only `SR_P<digits>_*`,
`SR_E2E_SWEEP`, `SR_SWEEP_*`) and `--server-env K=V` (only `SR_VOICE__ENABLED`, `SR_PAYMENTS_RATE_SOURCES`, `SR_PAYMENTS_ESPLORA_URL`). A worktree runs one job at a time.

## Classes and what they cost

| class                                  | what                                                                                                                   | cost to admit                                                                                                     |
| -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `tsc`                                  | web typecheck, `TSC_MAX` at once                                                                                       | `COST_TSC` 3300 MB (measured in wave 105: one typecheck took available memory from about 3.9 GB to 570 MB)       |
| `build`                                | the web bundle; records the tree for `shoot after`; `BUILD_MAX` at once                                                | `COST_BUILD` 1500 MB (20 to 23 s)                                                                                 |
| `shoot base\|after`                    | one cell and the spec                                                                                                  | `COST_SHOOT` 1200 MB (a cell is about 1 GB; wave 104 peak 1023 MB)                                                |
| `vitest -- <files>`                    | vitest in `apps/web`, two workers                                                                                      | `COST_VITEST` 1200 MB                                                                                             |
| `cargo -- build\|test\|clippy\|fmt\|run` | cargo in the worktree, own target folder, debug info off, `--offline`; runs alone                                      | `COST_CARGO` 3000 MB, own limit `CARGO_LIMIT_S`                                                                   |
| `test <mb> <s> [rc]`                   | a dummy for drills                                                                                                     | `<mb>`                                                                                                            |

Conf keys (`cli-agents/run/conf`, plain `KEY=VALUE`, read on every pass) with wave 108's values: `WAVE=p108`, `LANES="1 2"`, `PORT_BASE=8840`, `METRICS_BASE=9540`,
`COST_SHOOT=1200 COST_BUILD=1500 COST_TSC=3300 COST_VITEST=1200 RESERVE=700 RECENT_S=15 HOL_S=240`, `JOB_LIMIT_S=600`, `TSC_MAX=1 BUILD_MAX=1`, `STOP_AT="22:20"`,
`LANE_WTS` (three lane worktrees), and the wave's `CELL_BIN`, `BASE_DIST` and `TEMPLATE_DB` (`p108-cell-bin`, `p108-base-dist`, `sr_p108_world`); `COST_CARGO` and
`CARGO_LIMIT_S` are not set and take their defaults (3000 MB, 1500 s), because wave 108 has no Rust.
`RESERVE` is kept free; jobs admitted in the last `RECENT_S` seconds count at full cost because they have not grown yet; a job that waited `HOL_S` blocks newer ones until it fits
(head of line); a job is killed after `JOB_LIMIT_S`; nothing starts that cannot end before `STOP_AT`. A small job may pass a bigger one that does not fit.

## What a shoot does

1. Takes a lane (a port, `PORT_BASE + lane`) and a worktree. 2. Refuses a `shoot after` whose tree changed since its `build` (`stale build`, rc 3). 3. Clones the template database
under one clone lock, copies the world's state folder into the worktree's `e2e/.state` (the origin's port rewritten) and the search folder. 4. Starts the cell on the built bundle
(`SR_WEB__ROOT`, `SERVE_PUBLIC`, `SR_ADMIN_REQUIRE_REAUTH=0`, `SR_STORAGE__ENDPOINT=memory:`) and waits for `/readyz`. 5. Runs `npx playwright test <args> --project=chromium --workers=1
--retries=0` from the builder's own `e2e/`, with `SR_E2E_WEB=server` and `SR_E2E_BASE_URL`. 6. Stops the server's whole process tree by PID, checks the port, drops the database.
Everything printed goes through a filter that replaces the Postgres password with `<pw>`.

**`shoot base`** serves the wave's base bundle and runs the spec from the builder's `e2e/`: no file is put back, and it can run before the code is finished. **`shoot after`** serves the
builder's own `apps/web/dist`. Both are built mode, so they compare like with like; never compare with a dev-server shot. Order for an after round: edit, **commit, then `build`**, wait for
it, then `shoot after`. The build's record holds the tree's signature (`HEAD` and the porcelain status, untracked files included), so an edit or commit after the build fails the next
`shoot after` with `stale build`: build again. Edits under `e2e/` need no build. `--fresh` seeds the sweep world inside the job instead (about 25 s more) for a builder who owns the seed.

## What a spec must be to run as a job

It never imports a `/src/...` module and uses nothing only the dev server has; the origin comes from `SR_E2E_BASE_URL` only, never a port; it runs on the seeded sweep world
(`e2e/.state/sweep/world.json`, replaced by every job) and never seeds: what the world lacks it creates through the API under its own names and removes in a `finally`.
It skips unless its `ON` switch is set; `PHASE` is `base` or `after`; it writes `figures-<phase>.json` and its shots (base shots with a `base-` prefix) into `OUT`, and in the after phase
it compares only when `figures-base.json` is there. Its header comment holds the commander's command whole.

## The commander's side

- `runner.sh prepare <base worktree>`: builds the base tree, copies its `dist` to `BASE_DIST`, seeds the template database and keeps the world's state folder beside it (about one minute, wave 104).
- `runner.sh start` (background) and `runner.sh stop` (no new admission, exits when idle; `--now` kills the running jobs). `runner.sh status`, `lane.sh sync|status`, `strays.sh <name>`
  (lists processes left behind), `memwatch.sh <out.csv> [seconds]` (samples free memory and the working set of node, `sr-server`, Chromium, postgres, claude and tsc).
- **The re-shoot on lanes:** after the merge the commander builds the merged tree once and submits one job per spec with `--wt lanes` and `--dist <merged dist>`, so each runs on its own clone in
  the first free lane worktree.
- **Taking a new version:** between waves, nothing running: `runner.sh stop` (check its Windows PID is gone, `git -C cli-agents status --short` clean), `git -C cli-agents pull <clone> <branch>`,
  `bash cli-agents/runner-drill.sh` (dummy checks in its own run folder; all must pass), `runner.sh start`, one real `test` job, then one real shoot. **Going back:**
  stop, `git -C cli-agents reset --hard <previous commit>`, start. Never edit a file of a running runner.
- **Keeping the repository's copy in step:** a change is made and drilled in a clone of `cli-agents` (the drill runs from the clone's own folder), pulled into the running copy between waves,
  then copied into `scripts/dev/runner/` in the next wave's docs commit, with the commit's hash in `SOURCE`. The repository's copy is never the one that runs.

## What was measured, and the rules that came of it

- **Wave 105** (first wave on jobs): 56 jobs, fourteen builders, three cells at once, no crash, no killed job, lowest available memory 570 MB (one typecheck). The builders' sessions cost about
  125 MB each; the limit is the jobs. Waiting (median 81 s) was longer than running (20 to 50 s), so: fewer, fuller rounds, and read a failed job's log before submitting again. The gate, not the
  lanes, held most jobs.
- **Wave 107:** 83 jobs (14 build, 69 shoot) through the second version of the scripts; its hold lines (`mem`, `lane`, `wt`, `cap`, `hol`) all appear in the runner log. Eighteen builders was too many:
  they ran prettier, vitest, eslint and the token script in the same three minutes (8.4 GB of node) and available memory fell to 69 MB.
- **Rules that came of it:** builders run no vitest, eslint, web `tsc`, i18n audit or doc-link check in a busy window (the commander runs them once on the merged tree, CI runs the suites); code
  builders start five minutes after the others; no `tsc` job in a wave that changes no `.ts` or `.tsx`; `wait` always with 100; a builder stops its own processes by PID before it replies (`strays.sh`).

## Still untested on real cells

Said plainly, so nobody relies on them: the `cargo` class (a stub stood in for the drill; the real `--offline` and `CARGO_TARGET_DIR`, and the `run` and `clippy` argument shapes, never ran);
`--bin` (the request line and the refusals are drilled, but no wave job used it, so the folder reaching the server and `sr-cli` is not proven on a real cell); STOP_AT per class with real limits;
the `tsc` and `vitest` classes under the second version (wave 107 ran only builds and shoots); the Chromium match of `chrome.exe` with `ms-playwright` (only the headless-shell name was
seen); `world.sh`, `cell.sh` and `heavy.sh` started from a folder other than `cli-agents/`.
