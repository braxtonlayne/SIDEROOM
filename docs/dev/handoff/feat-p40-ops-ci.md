# feat-p40-ops-ci: OPS-CI-001, CI consolidation (wave 40)

Branch `feat/p40-ops-ci`. Measured 2026-10-31 from GitHub (`gh run list` / `gh run view --json jobs`): the last three runs of every workflow on `main`, plus the newest run of each workflow on the heads of PRs #64 to #69 (`integration/phase34`, `fix/beta-defaults`, `integration/phase35`, `integration/phase35b`, `integration/phase36`, `integration/phase37`). Nothing was pushed; no Playwright or server ran locally.

## Done

- **Runtime table** (below): every job of every workflow, minutes per run. No job is over budget. The `playwright` job peaked at 26.9 minutes (limit 36 at 25 percent spare), so **no shard rebalancing was needed** (the roadmap's trigger is 45 minutes); the matrix stays one job per browser.
- **Jobs that had no `timeout-minutes`** (default 360) got one with 25 percent spare over their slowest run: `rust` `fmt-clippy-test` 60 (slowest 43.7), `web` 30 (16.1), `npm-audit` 10, `openapi` `drift` 20 (8.0), `wasm` 15 (5.4), `repo-checks` 10, `release-dry-run` `web-bundle` 15, `server-image` 40 (22.9), `compose-config` 10.
- **`flaky-tests.yml` `hunt` had no room**: ten vitest repetitions took 89.3 minutes on the slowest of the last three runs against a 90-minute limit (46.8 and 54.4 on the others). Limit raised to 120 (25 percent spare over 89.3 is 111.6).
- **Feeding the new specs to `flaky-tests.yml`**: a second job `hunt-e2e` repeats `bridge`, `bridge-whole-guild`, `convert`, `drawer`, `emulator`, `studio`, `tool-rooms` and `watch-together` three times in Chromium with `--retries=0` (fake Discord started, emulator cores cached and verified as in `e2e.yml`). `scripts/flake-detect.mjs` now reads Playwright's list reporter (a line like `✘  4 [chromium] › file › title (1.2s)`), tested in `scripts/ci-scripts.test.mjs`. `activities-live` and `emulator-coop` need the SFU and stay in the `live` job. `noindex.spec.ts` runs in the `stack` job of `prod-smoke.yml` (it needs Caddy).
- **Firefox download retry**: the failing log of the PR #67 run is no longer retrievable (`gh run view --log-failed` on the phase35b runs finds no "Failed to download"; the E2E run there is green), but no `playwright install` step had a retry. New `scripts/playwright-install.sh` (three attempts, 15 s then 45 s apart, the last failure is the exit status) replaces the bare command in all 10 install steps: `e2e.yml` (5), `storage.yml`, `web.yml`, `prod-smoke.yml` (2), `voice-soak.yml`.
- **Two spec groups that never ran in CI, found while reviewing**: `bridge.spec.ts` and `bridge-whole-guild.spec.ts` skip themselves unless the fake Discord (`e2e/support/fake-discord.mjs`, 127.0.0.1:8090) answers, and no workflow started it; `convert.spec.ts` skips without a host ffmpeg, which the `playwright` job does not install. Two steps now run them in the `live` job (the fake Discord is started there; the ffmpeg of the cinema step is on PATH). Both are **`continue-on-error: true` until their first green run on GitHub** (never run there; the bridge specs passed locally in waves 35 and 35b). Each costs about 1 to 3 minutes of the `live` job's 25.
- **Owner checklist section 3**: rows added for `media.yml`, `voice-soak.yml`, `advisory-watch.yml`, `flaky-tests.yml` (and its new `hunt-e2e`), `prod-smoke-skip.yml` (all with the run that went green first), **`security-nightly.yml` (pending its first run; it lands with wave 39: weekly Monday 03:23 UTC, `timeout-minutes: 240`)**, and the two new `live` steps above.
- **Bundle budget** (`scripts/bundle-budget.json`): new rows measured with `node scripts/build-web.mjs`, gzip KB of the app shell plus the chunk's static closure:

| Row | Route chunk | Measured | Limit |
|---|---|---|---|
| app (existing) | `app` entry | 244.9 | 300 |
| music-player | `PlayerHost` | 262.3 | 300 |
| drawer | `DrawerHost` | 257.3 | 300 |
| converter | `ConvertStep` | 252.6 | 300 |
| studio | `StudioEditor` | 252.1 | 300 |
| emulator shell | `EmuPage` | 267.3 | 320 |
| activities | `ActivityLauncher` | 472.0 | 550 |

  The music wasm is 188.4 KB brotli (limit 600), the ffmpeg core stays lazy. Limits are the measurement plus about 15 percent. `check-bundle-budget.mjs` and `check-ops-files.mjs` pass.

## Runtime table (minutes)

"main run 1" is the newest. Workflows that are path filtered, scheduled or dispatch-only have fewer than three runs on `main`; their PR runs fill the gap. `main` runs 2 and 3 of `E2E` predate the cached emulator cores step (merged in the newest run), so that step is measured only in the newest run. `Production smoke` PR runs are mostly the `prod-smoke-skip.yml` companion (same job names, echo only), which is why they show 0.0 to 0.2. `visual-baselines` is skipped except on dispatch. `limit` is `timeout-minutes` divided by 1.25.

| Workflow | Job | timeout | main run 1 | main run 2 | main run 3 | PR runs #64-#69 (min to max) | max | limit at 25% spare | verdict |
|---|---|---|---|---|---|---|---|---|---|
| advisory-watch.yml | crypto-bigint | 15 | 0.3 | - | - | - | 0.3 | 12.0 | ok |
| backup-drill.yml | drill | 30 | 7.2 | 6.0 | - | 4.3 to 8.8 (5) | 8.8 | 24.0 | ok |
| backup-drill.yml | wal-drill | 15 | 0.3 | 0.4 | - | 0.4 to 0.7 (5) | 0.7 | 12.0 | ok |
| e2e.yml | live | 75 | 23.1 | 25.0 | 24.5 | 13.6 to 20.6 (6) | 25.0 | 60.0 | ok |
| e2e.yml | built-bundle (webkit) | 45 | 9.8 | 9.3 | 10.6 | 6.5 to 10.4 (6) | 10.6 | 36.0 | ok |
| e2e.yml | built-bundle (firefox) | 45 | 10.3 | 12.9 | 12.8 | 6.8 to 12.8 (6) | 12.9 | 36.0 | ok |
| e2e.yml | playwright (webkit) | 45 | 20.9 | 23.4 | 15.5 | 12.7 to 24.0 (6) | 24.0 | 36.0 | ok |
| e2e.yml | playwright (chromium) | 45 | 20.3 | 24.1 | 23.0 | 17.9 to 25.4 (6) | 25.4 | 36.0 | ok |
| e2e.yml | playwright (firefox) | 45 | 20.2 | 16.9 | 16.1 | 14.9 to 26.9 (6) | 26.9 | 36.0 | ok |
| e2e.yml | built-bundle (chromium) | 45 | 8.9 | 11.1 | 9.1 | 7.3 to 13.3 (6) | 13.3 | 36.0 | ok |
| e2e.yml | visual | 30 | 12.7 | 10.0 | 7.5 | 6.6 to 12.3 (6) | 12.7 | 24.0 | ok |
| flaky-tests.yml | hunt | 120 | 89.3 | 46.8 | 54.4 | - | 89.3 | 96.0 | ok |
| media.yml | media | 60 | 16.7 | - | - | 14.2 to 14.2 (1) | 16.7 | 48.0 | ok |
| openapi.yml | drift | 20 | 8.0 | 6.3 | 7.9 | 3.3 to 8.0 (6) | 8.0 | 16.0 | ok |
| prod-smoke.yml | web | 30 | 2.1 | - | - | - | 2.1 | 24.0 | ok |
| prod-smoke.yml | upgrade-base | 40 | 17.4 | - | - | - | 17.4 | 32.0 | ok |
| prod-smoke.yml | rules | 10 | 0.1 | - | - | - | 0.1 | 8.0 | ok |
| prod-smoke.yml | image | 40 | 17.5 | - | - | - | 17.5 | 32.0 | ok |
| prod-smoke.yml | upgrade | 25 | 10.9 | - | - | 0.1 to 0.2 (6) | 10.9 | 20.0 | ok |
| prod-smoke.yml | stack | 30 | 2.1 | - | - | 0.0 to 0.1 (6) | 2.1 | 24.0 | ok |
| prod-smoke.yml | systemd | 15 | 0.5 | - | - | 0.0 to 0.1 (6) | 0.5 | 12.0 | ok |
| release-dry-run.yml | server-image | 40 | - | - | - | 15.4 to 22.9 (4) | 22.9 | 32.0 | ok |
| release-dry-run.yml | compose-config | 10 | - | - | - | 0.2 to 0.3 (4) | 0.3 | 8.0 | ok |
| release-dry-run.yml | web-bundle | 15 | - | - | - | 2.5 to 4.2 (4) | 4.2 | 12.0 | ok |
| repo-checks.yml | checks | 10 | 0.2 | 0.3 | 0.3 | 0.2 to 0.3 (6) | 0.3 | 8.0 | ok |
| rust.yml | fmt-clippy-test | 60 | 25.6 | 32.9 | 18.5 | 24.8 to 43.7 (6) | 43.7 | 48.0 | ok |
| sdk-compat.yml | compat (discord.js) | 45 | 6.8 | 9.1 | - | - | 9.1 | 36.0 | ok |
| sdk-compat.yml | compat (serenity) | 45 | 10.4 | 7.2 | - | - | 10.4 | 36.0 | ok |
| storage.yml | composer | 45 | 12.6 | 12.7 | 12.7 | 7.1 to 12.4 (6) | 12.7 | 36.0 | ok |
| storage.yml | live-s3 | 40 | 9.4 | 9.0 | 7.6 | 4.4 to 9.9 (6) | 9.9 | 32.0 | ok |
| wasm.yml | wasm | 15 | 5.3 | 4.4 | 5.4 | 2.3 to 4.1 (6) | 5.4 | 12.0 | ok |
| web.yml | npm-audit | 10 | 0.3 | 0.4 | 0.3 | 0.2 to 0.3 (6) | 0.4 | 8.0 | ok |
| web.yml | web | 30 | 16.1 | 15.3 | 14.1 | 9.3 to 12.6 (6) | 16.1 | 24.0 | ok |
| web.yml | static-browser-checks | 15 | 0.9 | 1.4 | 0.9 | 0.8 to 1.0 (6) | 1.4 | 12.0 | ok |


### The steps this phase added (newest `main` runs, minutes)

| Step | Job | Minutes |
|---|---|---|
| Emulator cores (cache restore) and verify or fetch | `playwright`, each browser | 0 and 0 to 1.2 |
| Migrate, start the server, run Playwright (all specs, including drawer, studio, tool-rooms, watch-together, emulator) | `playwright (chromium)` | 9.6 (10.4 to 10.9 on the two older runs) |
| Exit gates | `playwright` | 0.3 to 1.2 |
| Voice, Activities (`activities-live`), message components, keyboard voice, `emulator-coop` | `live` | 1.3 to 3.4 |
| Voice E2EE Chromium, Firefox, WebKit | `live` | 0.5, 0.5 to 0.7, 0.1 to 0.2 |
| Voice through TURN relay | `live` | 0.4 |
| Install ffmpeg and build sr-transcoder | `live` | 0.8 to 1.0 |
| Cinema submit, Library gate (3 browsers) | `live` | 0.6, 1.8 to 2.0 |
| First-party voice bots | `live` | 4.5 to 4.7 |
| Entrance isolation, smoke and journey (built bundle) | `built-bundle` | 0.8 to 0.9 |
| Composer multipart specs | `composer` | 1.2 |
| Live S3 Rust tests (BYOS mounts and scrapbook archive are in the same step group) | `live-s3` | 1.3 to 1.6 |
| Media pipeline smoke (mount stage) | `media` | 7.3 |

## Flakiness seen in the same runs

- `Rust` on `main` failed once (run 41193216575, 2026-10-31 10:33, 18.5 minutes in) and passed on the next two; not investigated (outside this item).
- `E2E` failed on several integration branches between 2026-10-29 and 10-31 (phases 29 to 34): `[WebServer] Error: read ECONNRESET` in the Chromium job (Vite dev server connection resets) and, on `fix/beta-defaults`, the phase 6 and 7 exit-gate step in all three browsers. All later runs, including every run listed above, are green.
- `playwright (webkit)` varies 12.7 to 24.0 minutes between runs and `rust` 18.5 to 43.7: both are wide, still inside the 25 percent rule.

## Still open

- The two `live` steps and `hunt-e2e` have never run on GitHub; remove the `continue-on-error` lines after their first green run (checklist rows).
- `security-nightly.yml` lands with wave 39 and was not measured; its 240-minute limit is the author's, and its first run should confirm it has 25 percent spare.
- The Firefox download failure itself could not be reproduced or its log recovered; the retry is preventive.
- `OPS-DEP-001` (tantivy) waits for 2026-11-17 and is untouched.

## Tests and checks run

- `node scripts/build-web.mjs`, `node scripts/check-bundle-budget.mjs` (all rows ok), `node scripts/check-ops-files.mjs` (ok), `node --test scripts/ci-scripts.test.mjs` (6 pass), prettier on the touched scripts.
- Every file in `.github/workflows` parses with the repo's js-yaml.

## Decisions

- Timeouts for jobs that had none follow the same rule as the rest (slowest run plus 25 percent, rounded up to a round number).
- A new step that cannot be proven locally is `continue-on-error` with a checklist row, rather than a required failing step.

## Open owner questions

None.
