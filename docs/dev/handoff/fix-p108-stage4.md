# Wave 108 handoff: stage4 (D-1027; SEC-AUD-499, 505)

## Clock
Started 20:33; copy, docs and handoff done and committed by about 20:45. No job, no spec, no shots (the task has none).

## Done (commits on `fix/p108-stage4`)
- Copied byte for byte from `cli-agents/` at head **`cd62e67`** (`git -C cli-agents status --short` was clean) into `scripts/dev/runner/`: `README.md`, `job.sh`, `runner.sh`, `runlib.sh`,
  `lane.sh`, `cell.sh`, `world.sh`, `strays.sh`, `runner-drill.sh`, `memwatch.sh`, `memwatch.ps1`, `heavy.sh`, `tc.sh` (thirteen files; the task's "thirteen scripts" counts twelve scripts
  plus the README), and new `SOURCE` (hash, date). Every `.sh` is committed as mode 100755 (`git update-index --chmod=+x`), because `scripts/check-ops-files.mjs` requires it for every tracked `.sh`.
- `docs/dev/job-runner.md` (new).
- `docs/dev/handoff/fix-visual-sweep.md`: "How to run the sweep" now says the two ways (built mode through a job; the dev stack below, which stays) and which is used when.
- `bash -n` on the eleven `.sh` files: all pass (nothing was run). `memwatch.ps1` not parsed (no PowerShell run).

## Not done
- AGENTS.md (the commander's): sentences below. Not run: vitest, eslint, tsc, i18n audit, doc-link check (left to the commander). Prettier: not applicable (`.prettierignore` has `*.md`; `.sh` and `.ps1` are not covered).
  The one table in `job-runner.md` is written in prettier's column style anyway.

## REACH table
Not applicable (tools builder).

## FINDERS (rule 13)
`git grep -n "scripts/" -- .github scripts/*.mjs package.json` read, plus the checks' sources:
- **Secret scan (`scripts/secret-scan.mjs`, `repo-checks.yml` runs it on every tracked file): one hit.** `scripts/dev/runner/cell.sh:24` is
  `SR_DATABASE__URL="postgres://postgres:$(pw)@127.0.0.1:5432/$db"`: the `password-in-url` pattern matches, and its placeholder test accepts `$VAR` and `${VAR}` but not `$(cmd)`. It is no
  secret (the password comes from the file in `%LOCALAPPDATA%` at run time). The other two URL builders (`runlib.sh:54`, `world.sh:21`) use `$PGPASSWORD` and pass. The wording of a page cannot
  fix a script, and a changed line would break "byte for byte", so **the commander must add to `scripts/secret-scan.allow.json`**:
  `{ "path": "scripts/dev/runner/cell.sh", "pattern": "password-in-url", "reason": "Builds the cell's database URL from the password file at run time via $(pw); no literal." }`
  (or the runner's next version writes `$PGPASSWORD` there; then the copy changes in the next docs commit). I did not run the scan. `sr-secret-assignment` and `password-query-param`: no hit (`rg`).
- `scripts/check-ops-files.mjs`: every tracked `.sh` must be 100755 (done); it runs `sh -n` only over `deploy/scripts`. Shellcheck (`repo-checks.yml`) covers `deploy/scripts/*.sh` only. No hit.
- `scripts/check-doc-links.mjs` walks every `.md`: `job-runner.md` and `scripts/dev/runner/README.md` hold no markdown links. Left to the commander to run.
- Prettier ignores `*.md`; ESLint does not lint `.sh`. No `scripts/*.test.mjs` glob reads `scripts/dev`. `git ls-files scripts/dev` was empty before (the folder is new).
- `.gitattributes` has no `eol` rule: the files were LF on copy; on Windows `core.autocrlf` could rewrite them on checkout, which a Windows bash would dislike (the running copy is not affected).

## Hash and grep proof
Copied hash `cd62e67`. `grep -rn "PGPASSWORD=\|postgres://[^$<]" scripts/dev/runner` shows five lines, none with a literal password:
`cell.sh:24` (`postgres://postgres:$(pw)@...`), `runlib.sh:54` and `world.sh:21` (`...:$PGPASSWORD@...`), `runlib.sh:52` and `world.sh:17` (`PGPASSWORD=$(tr -d '\r\n' < ".../sr_pg_pw.txt")`: read from the file),
`runner-drill.sh:381` (`PGPASSWORD="$pw"`: a variable). `grep -rniE password` over the folder shows only comments, the filter that replaces it with `<pw>`, and the drill's check that it is in no log.

## JOBS
None submitted.

## Figures against the task's lists
Not a layout task: nothing measured. Nothing under `apps/web` or `packages` changed.

## Who imports my files
None: documentation and scripts, loaded by nothing (not by first paint, not by a chunk).

## Checks
`bash -n` on all eleven copied `.sh`, 20:38, exit 0 each. `git status` showed only my files before the commit. Nothing else run.

## For AGENTS.md section 4 (item 9 already says most of it; wave 105's text stands and is correct today)
Add at its end: "The runner's scripts are copied into `scripts/dev/runner/` (commit hash in `SOURCE`) and documented in `docs/dev/job-runner.md`; the running copy stays in `cli-agents/`, is changed only in a clone and between waves, and the repository's copy is refreshed in the next wave's docs commit."
And, if the commander wants the memory rule there (D-1028): "A builder runs no vitest, eslint, web `tsc`, i18n audit or doc-link check, and only one `prettier --check` of its own files at a time: the commander runs them once on the merged tree."

## Closing text
- **SEC-AUD-499 (the runner's scripts are in no repository):** closed by D-1027: the copy at `cd62e67` is in `scripts/dev/runner/`, with `docs/dev/job-runner.md`. Left: the copy names this machine's paths and is
  refreshed by hand each wave; add the allow entry above to the secret scan.
- **SEC-AUD-505 (the runner's patches, second version):** the version ran wave 107 (83 jobs, builds and shoots only). **Proven by a real wave:** the scripts finding their own folder (the live copy is the drilled
  version running from `cli-agents/`), hold lines (`mem`, `lane`, `wt`, `cap`, `hol` all appear in the runner log), `wait` with 100. **Not proven on real cells:** the `cargo` class, `--bin` (no job used it: every wave 107
  request has an empty `bin=`), `tsc` and `vitest` jobs under the second version, STOP_AT per class, the `chrome.exe` branch of the Chromium match, and `world.sh`/`cell.sh`/`heavy.sh` started from another folder.
  (The sampler's Chromium column: the name match was seen working by the wave 106 builder; I did not check wave 107's samples.)

## One changelog sentence
The job runner's scripts and a page describing them (`docs/dev/job-runner.md`) now live in the repository under `scripts/dev/runner/`, copied from the running copy at `cd62e67`.

## Owner questions (Fable)
None. Default taken: the repository holds an archive copy and never runs it.

## FAULTS / proposed rows
- Proposed: a runner version that builds the database URL as `$PGPASSWORD` in `cell.sh:24`, so the secret scan needs no allow entry; the commander or the next tools wave drills it in a clone.
- Proposed: a `.gitattributes` line `scripts/dev/runner/* text eol=lf` (not mine: file not in my row).

## PROCESSES
`strays.sh p108-stage4` at 20:42: `strays: none found`. No job of mine queued or running.
