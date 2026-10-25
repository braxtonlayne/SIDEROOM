# Handoff: integration/phase24 (wave 24 merges, integrator work, gates)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p24-integ`, branch `integration/phase24` (base main `d7ab66b2`). Local commits only: nothing pushed, no PR, nothing merged to main. Cargo target dir `C:/Users/braxt/Desktop/SIDEROOM_wt/p24-integ-target` (`CARGO_PROFILE_DEV_DEBUG=0`). Earlier waves named this file `feat-p<wave>-integration.md`; this one is named as the wave-24 task asked.

## Resume here

**State (2026-10-25): merged and gated locally.** The three wave-24 branches are merged with `--no-ff`, the decided follow-ups are applied, and every local gate below passes on the merged tree. No cargo, Vitest, server or Playwright process of this pass is running. The scratch databases `sr_p24_integ` and `sr_p24_integ_main` were dropped; the temporary main worktree `p24-integ-main` was removed.

**What waits on CI (first run of the pull request):**

| What | Job | Why not here |
|---|---|---|
| RDY-LIB-001, the Library exit gate (`e2e/tests/library-gate.spec.ts`, three browsers) | `e2e.yml` `live`, step "Library exit gate over Garage" | Needs Garage (presigned PUT, ingest); locally the parity agent got to the PUT on memory storage. Stays unticked until green |
| First-party voice bots (sr-sdk, then `@sideroom/bot` with sr-sdk as partner) | `e2e.yml` `live`, step "First-party voice bots" | Never run on CI; ran locally on the devplat agent's machine (see its handoff) |
| `promtool check rules deploy/monitoring/alerts.yml` | `prod-smoke.yml` new `rules` job | Downloads Prometheus 3.5.0 on the runner (owner-approved, CI only); never downloaded here |
| `view-as-role.spec.ts` on Firefox and WebKit | `e2e.yml` `playwright` | Firefox does not launch here; no Playwright in this pass (ports 8080/5173 are shared) |
| shellcheck | `repo-checks.yml` | Not installed here; no `deploy/scripts/*.sh` changed in this wave |

**What waits on the owner:**

- **RDY-PERF-002**: dispatch `load-test.yml`. First run with `seed_scale=true`, `sessions=1000`, `msg_rate=100`, `rest_rps=150`, `duration=120`; then the defaults. Paste the numbers into `docs/dev/load-testing.md` "Results" and tick after a run at the defaults.
- **The `rules` job's sha256 TODO**: `prod-smoke.yml` checks the tarball against the release's own `sha256sums.txt`. The checksum could not be verified offline, so `PROM_SHA256` is empty and marked TODO. A human reads the hash once from the release page and fills it in.

## Merges

| Order | Branch | Tip | Result |
|---|---|---|---|
| 1 | `feat/p24-scale` | `33f4338c` | clean (`13ef3778`) |
| 2 | `feat/p24-parity` | `f6ba00e2` | clean (`cc4cec3a`) |
| 3 | `feat/p24-devplat` | `4eaa22d7` | clean (`dd0de45d`) |

No textual conflicts. Checked by hand anyway:

- `route_matrix.tsv`: one new row (scale's `GET /users/@me/guild-settings`); the `route_matrix` tests pass.
- `openapi.json`, `schema.d.ts`, MSW handlers: regenerated on the merged tree (`cargo run -p sr-server -- openapi`, `generate`, `mocks:generate`); identical to the auto-merge. `mocks:check` passes (972 operations).
- `e2e.yml` `live` job: both new steps are there (parity's "Library exit gate", devplat's "First-party voice bots"), and both logs (`server-library.log`, `server-voice-bot.log`) are in the artifact list.
- `docs/01-decisions-log.md`, `docs/inventory/20-launch-readiness.md`, `pnpm-lock.yaml`, `Cargo.lock`: merged without overlap. i18n fragments are found by glob, so there is no index to merge.
- `pnpm install --offline --frozen-lockfile` worked with devplat's `werift@0.25.0` (already in the store). Nothing was fetched.

## Integrator commits

| Commit | Ids | What |
|---|---|---|
| `97b370c4`, `2c14df1d` | RDY-OBS-001 | `prod-smoke.yml` gets a standalone `rules` job. It has no `needs` and is only appended; the existing jobs are unchanged. It uses pinned Prometheus 3.5.0 on linux-amd64, checks the sha256 against the release's `sha256sums.txt` (with a TODO to hard-code the hash), and runs `promtool check rules deploy/monitoring/alerts.yml`. The owner approved this through the coordinator. The scale handoff and the inventory note now say "added, approved by owner" |
| `9c9e1210` | RDY-PAR-004 | P24-PAR-1 now says "the messages of a still-pending Message Request" instead of "the first message". That is what `screen_dm_message` does: it checks every message while the request is `Pending`. The same correction is in the parity handoff and the inventory row. Fable's verdict is recorded: P24-PAR-1 to 5 kept (status "Kept (Fable)"), and `DM_SPAM_LINK_HOSTS` stays 3 |
| `d4729643` | RDY-PAR-002/004/005, RDY-OBS-001, RDY-DEV-001 | `answers-followups-p16.md` "Phase 24 round", rows U-01 to U-06 (see below) |
| `e0148d99` | RDY-OBS-001, RDY-PERF-002 | RUNBOOK: a fresh install pages until the first backup and the first restore-drill marker exist, because the gauges are 0 for "never" on purpose. `RestoreDrillFailed` "Raised by" names the 0 case. `load-testing.md` "The CI run" gives the first dispatch parameters. The scale handoff's questions 1 and 2 are marked decided |
| `aa51f569` | MOD-SUS-003 | The sr-db test fix (below) |
| `f5c98d65` | all wave ids | `CHANGELOG.md` "Phase 24" section |
| `a26d8c79` | RDY-PERF-004 | The `sr-memcount` crate doc example was an `ignore` block that called an undefined `build_it`. `--include-ignored` compiles ignored doctests, so it failed. Now it is a real example that runs |
| `9934de1b` | RDY-DEV-001 | `secret-scan.mjs` flagged `SR_BOT_TOKEN: run.token` in `e2e/sdk-compat/voice-bot-live.mjs`, and `secret-scan.test.mjs` "the current tree is clean" failed. Fixed with a reasoned allow-list entry for that file and pattern. The values are bot tokens that the driver mints at runtime |

### Backlog rows (U-01 to U-06, `docs/dev/answers-followups-p16.md`)

- U-01 external emoji in the picker: a server-independent emoji route, the send-side `USE_EXTERNAL_EMOJIS` check, the renderer and the picker sections. Who may use them is a product call.
- U-02 View as Role also filters the member list and the thread list.
- U-03 the DM spam filter's "All" level currently equals "Non-friends", because friends are skipped at every level. Discord's "All" includes friends.
- U-04 `sr_storage_bytes` for the object-store buckets and Library/Cinema media, through a bounded daily job (not the 15-minute health run).
- U-05 `backup.sh` writes to a temp name and renames. **Already true, so ticked:** `backup.sh` and `sr-cli backup` both write `*.dump.partial` and rename it after `pg_dump` succeeds. `newest_backup_modified` counts only `.dump`/`.json`, so a failed dump never refreshes the age. Still open in that row: a leftover `.partial` is not cleaned up.
- U-06 the TypeScript bot SDK does not rejoin voice after a fresh session. Neither SDK has an Opus encoder (that would be a new dependency).

## The sr-db test finding

`crates/sr-db/tests/p13_settings_admin.rs` `staff_people_views_link_unlink_notes_and_freeze` asserted that `set_person_notes(1, ..)` is `NotFound("person")`. **The test depended on id state; the product code is fine.** The test assumed no Person has id 1 in the shared database, but several suites insert `persons (id) VALUES (1)`.

- Integrated tree, fresh scratch DB: passes. After `INSERT INTO persons (id) VALUES (1)`: fails at line 260, the parity agent's failure exactly.
- main `d7ab66b2` (temporary detached worktree, its own scratch DB): the same. It passes fresh and fails at line 260 once person 1 exists. **It fails on main too.**
- Fix (`aa51f569`): use id `-1`, which no Person can have. It passes with person 1 present, and in the full workspace run.
- Which suite left person 1 behind in the parity agent's database is not known. Every literal-1 insert found uses its own `TestDb` clone, and a fresh full workspace run here left no person with a small id (min id was a snowflake). Most likely an earlier run in that reused database, or a run that aborted before a clone was dropped.

## Gates (merged tree, one at a time)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | pass |
| `cargo clippy -j 2 --workspace --all-targets -- -D warnings` | pass (`sr-memcount` re-checked after its doc fix) |
| `cargo test -j 2 --workspace --no-fail-fast -- --include-ignored --test-threads=1` (fresh `sr_p24_integ`, both URLs set to it) | 3500 passed, 1 failed: the `sr-memcount` doctest. Fixed (`a26d8c79`), then `cargo test -p sr-memcount -- --include-ignored` passed (2 unit + 1 doc). `p13_settings_admin` 7/7, `p24_parity`, `jobs_scale_pg_tests`, the memory-budget tests: ok |
| `route_matrix` (6) and `openapi_names` (2), `KNOWN_CLASHES` empty | pass |
| `cargo deny --offline check` (licenses, bans, sources, advisories from the cached DB) | pass |
| `pnpm -r typecheck` | pass |
| `pnpm -r lint` (eslint) and `prettier --check .` | pass |
| i18n: `i18n-verify-diff.mjs main HEAD` | MISSING 0; `i18n-audit.mjs` 41 (unchanged from before the wave) |
| Web Vitest `--maxWorkers=2` | 303 files, 2630 tests passed, first run, no flakes |
| `packages/bot` Vitest | 28 passed, 1 skipped (`voice.live.test.ts` without `SR_VOICE_LIVE`) |
| `ui` 321, `markdown-react` 45, `api-client` 6, `wasm` 14 | pass |
| `@sideroom/web build` + `check-bundle-budget.mjs` | pass (entrance 232.6/300 KB, lounge 432.4/600, settings 304.8/600, developers 236.1/400) |
| `pnpm licenses list --prod` + `check-licenses.mjs`; `check-notices-copy.mjs` | pass |
| `check-ops-files.mjs` | ok (116 series) |
| `check-doc-links.mjs` | ok |
| `secret-scan.mjs` (tree and `--diff main`), `check-dependabot.mjs`, `node --test scripts/*.test.mjs` (46), `e2e/sdk-compat` `node --test` (5) | pass (after `9934de1b`) |
| `check-pr-title.mjs` on the suggested title | ok |
| Playwright, servers on 8080/5173 | not run (wave rule) |

## Clean-up

- Dropped: `sr_p24_integ`, `sr_p24_integ_main`. `postgres` and `sr_test` were not touched.
- `TestDb` templates left on the server: `sr_tpl_160_1280_*`, `sr_tpl_161_1300_*`, `sr_tpl_161_1310_*`, `sr_tpl_162_1310_*`. They are reused by later runs and harmless. Drop them when convenient.
- Worktree `p24-integ` and its target dir stay for the PR. The agents' worktrees `p24-scale`, `p24-parity`, `p24-devplat` were not touched.

## Suggested PR title

`RDY-SCALE-002, RDY-OBS-001, RDY-PERF-001/003/004, RDY-PAR-001..006, RDY-DEV-001/002: wave 24 integration (scale, parity, developer platform)`
