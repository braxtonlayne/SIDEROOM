# Handoff: integration/phase25 (wave 25 merges, integrator work, gates)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p25-integ`, branch `integration/phase25` (base `aaf0835e`: main `49ed1495` plus roadmap-p25 and D-374 to D-379). Local commits only: nothing pushed, no PR, nothing merged to main. Cargo target dir `C:/Users/braxt/Desktop/SIDEROOM_wt/p25-integ-target` (`CARGO_PROFILE_DEV_DEBUG=0`).

## Resume here

**State (2026-10-26): merged and gated locally.** The three wave-25 branches are merged with `--no-ff`, the decided follow-ups P25-OPS-1 and P25-SEC-1 are applied, and every local gate below passes on the merged tree. No cargo, Vitest, server or Playwright process of this pass is running. The scratch database `sr_p25_integ` was dropped.

**What waits on CI (first run of the pull request):**

| What | Job | Why not here |
|---|---|---|
| RDY-OPS-007, the upgrade drill | `prod-smoke.yml` `upgrade-base` and `upgrade` (the PR touches `deploy/**`) | No Docker on this machine; proven only against the recording fake Docker (`scripts/upgrade-drill.test.mjs`). Tick when green and under 25 minutes. Budget two or three iterations (the upgrade-drill handoff names the likely fixes) |
| `prod-smoke-skip.yml` (P25-OPS-1) | its own three echo jobs | Never run; it reports `stack`, `systemd`, `upgrade` only on PRs that do NOT touch the install surface, so this PR does not run it |
| RDY-DEV-005, `e2e/tests/developer-gate.spec.ts` | `e2e.yml` `playwright`, step "Exit gates of phase 6 and 7" (`--grep @gate`) | No Playwright here (ports 8080/5173 shared); the gates agent ran it green on Chromium and WebKit locally |
| RDY-MOD-001, `e2e/tests/moderation-gate.spec.ts` | same step | same; the Postgres half (`deadline_windows.rs`) ran here |
| The `playwright` job at its new 45-minute limit | `e2e.yml` | first run |
| Doc 13 phase 6 and 7 rows | after the `@gate` step is green | link the run (AGENTS.md wave 25 checklist line); phase 7 becomes Proven, phase 6 stays Partly until `sdk-compat` is green |

**What waits on the owner:** push and the first PR run; after two green default-branch runs of `prod-smoke.yml` (and `upgrade` each under 25 minutes), add `stack`, `systemd` and `upgrade` as required checks for `main` (never `upgrade-base`), with `prod-smoke-skip.yml` already on `main` ([owner checklist](../owner-checklist.md) section 3).

## Merges

| Order | Branch | Tip | Result |
|---|---|---|---|
| 1 | `feat/p25-upgrade-drill` | `fcd3d5ce` | clean (`7fb74974`) |
| 2 | `feat/p25-authz-writes` | `5420d7c6` | clean (`ab29bbfe`) |
| 3 | `feat/p25-gates` | `21e976f4` | clean (`a7211901`) |

No textual conflicts in any of the expected spots (answers-followups-p16, the decisions log, the inventory, i18n, OpenAPI): the branches kept to their Split rows. OpenAPI was regenerated on the merged tree (`cargo run -q -p sr-server -- openapi`, then `generate` and `mocks:generate`): byte-identical to the upgrade-drill branch's output, `mocks:check` clean (972 operations).

## Integrator changes

1. **P25-OPS-1** (decisions log, new section "Phase 25 upgrade-drill agent"):
   - `.github/workflows/prod-smoke-skip.yml`: `on: pull_request` with `paths-ignore` equal to `prod-smoke.yml`'s pull_request `paths` (checked programmatically with js-yaml), jobs `stack`, `systemd`, `upgrade` on `ubuntu-latest` that echo "install surface untouched".
   - `admin.system.storagePartial` (`p25-upgrade-drill-1.ts`) names the setting and where it lives: `ops.storage_measure_max_objects` is in the `limits` group of the settings catalog, shown in Admin > "Limits & settings" under the group "Limits" (not a "Settings > Limits" page, which does not exist).
   - Roadmap-p25 RDY-OPS-007: "merge base with the PR's target branch (`GITHUB_BASE_REF`, else `main`)".
   - Owner checklist: the prod-smoke branch-protection line names the three jobs and the companion workflow; the "Node WebRTC library" part of the optional-downloads row is struck (`werift` 0.25.0 is in `packages/bot/package.json`).
2. **P25-SEC-1** (decisions log, new section "Phase 25 authz-writes findings"):
   - (1) heartbeat: kept as built, no change.
   - (2) `/requests/{id}/fulfil`: `route_matrix.tsv` already reads `author_or_staff` / `REVIEW_READING`; only the matrix comment changed; the stale finding and owner question were removed from the authz-writes handoff.
   - (3) no wallet: `rightful_override` calls both Patron download routes as `victim` (the seeded Patron). The Library download is admitted (`ok`, 409 "this chapter has no pages"); the Cinema download answered 404 "Unknown asset" (no transcoded asset), so it went back into `WRITE_OBSERVED` with the decided reason. `WRITE_OBSERVED_LEN` 42 to 41; the invoice reasons name D-257. The write phase (`SR_AUTHZ_WRITE_ONLY=1`) ran green after the change, then the full matrix in the workspace run.
   - (4) evasion scan: `crates/sr-moderation/src/suspicion.rs::scan_person_in` keeps the newest ban per related Person and spends the ten-signal budget newest ban first (ties by higher Person id), breaking out of both loops when it runs out. New Postgres test `the_signal_budget_keeps_the_newest_ban` (`crates/sr-moderation/tests/suspicion.rs`): it failed before the fix (2 signals for the Person banned 6 days ago, expected 0) and passes after; the whole `sr-moderation` suite passes. The gates handoff note is rewritten as a fixed bug.
3. **Fixes the gates found:**
   - `scripts/secret-scan.allow.json`: entry for the fixture owner passphrase in `scripts/upgrade-drill.test.mjs` (the same kind as the existing `install.test.mjs` entry); the upgrade-drill branch made `secret-scan.mjs` fail.
   - `scripts/threat-model.test.mjs` read only `roadmap-p20.md`; threat-model row T7 now names RDY-SEC-011, defined in `roadmap-p25.md`, so `node --test "scripts/*.test.mjs"` failed. It now reads every `docs/dev/roadmap-p<N>.md` with N >= 20.
4. **Records:** inventory `docs/inventory/20-launch-readiness.md` section "Phase 25" (ticked: RDY-SEC-011, RDY-SEC-012, RDY-OBS-002, RDY-OPS-008, RDY-DOC-006; unticked pending CI: RDY-OPS-007, RDY-DEV-005, RDY-MOD-001); `CHANGELOG.md` "Phase 25 - 2026-10-26"; this file.

## Gates run on the merged tree

| Gate | Result |
|---|---|
| `cargo fmt --check` | pass |
| `cargo clippy -j 2 --workspace --all-targets -- -D warnings` | pass (rerun after the P25-SEC-1 changes) |
| `cargo test -j 2 --workspace -- --include-ignored --test-threads=1` on `sr_p25_integ` | pass: 195 test binaries, 3,521 passed, 0 failed, 28 minutes. `authz_matrix` (refusal, read and write phases, six groups) 95 s, inside the five-minute budget |
| `route_matrix` (4 tests) and `openapi_names` (`KNOWN_CLASHES` empty) | pass (inside the workspace run) |
| `cargo deny --offline check` | pass (advisories, bans, licenses, sources) |
| Web `pnpm -r typecheck` (incl. e2e, packages/bot) | pass |
| `pnpm -r lint` (eslint) and `prettier --check .` | pass |
| i18n: `i18n-verify-diff.mjs` (MISSING 0), `i18n-audit.mjs` (no touched file listed), `src/i18n` catalog tests | pass |
| Vitest `apps/web --maxWorkers=2` | pass first time: 303 files, 2,633 tests (no flaky rerun needed). Rerun of `src/help src/lib src/admin src/i18n` after the CHANGELOG and message edits: 65 files, 507 tests, pass |
| `pnpm --filter @sideroom/web build` and `check-bundle-budget.mjs` | pass (all chunks under budget) |
| `packages/bot` tests | pass (28, 1 skipped live test) |
| `node --test "scripts/*.test.mjs"` | pass, 52 (after the threat-model test fix) |
| `check-ops-files`, `check-doc-links`, `secret-scan` (after the allow entry), `check-notices-copy`, `check-dependabot` | pass |
| Licences: `pnpm licenses list --json --prod \| check-licenses.mjs`, `cargo deny` licenses | pass |
| `mocks:check` | pass |
| `prod-smoke-skip.yml` and the changed workflows parse with js-yaml 4.3.2 | pass |
| Playwright, servers on 8080/5173 | not run (by the brief) |

## Suggested PR title

`RDY-SEC-011, RDY-SEC-012, RDY-OPS-007, RDY-OBS-002, RDY-OPS-008, RDY-DEV-005, RDY-MOD-001, RDY-DOC-006: phase 25 integration (authz write phase, upgrade drill, storage figures, phase 6 and 7 exit gates)` (`check-pr-title.mjs`: ok).
