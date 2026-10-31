# Integration, phase 39: operator switches, invite controls, the immutable suite, Game Servers "Coming soon" (GSV-PAGE-001, OPS-INV-001, OPS-FLG-001, OPS-IMM-003)

Branch `integration/phase39` (base `origin/main` 77d80945: waves 35b, 36 and 37 merged; `git merge origin/main` was a
no-op). Not pushed; the commander opens the PR. Decisions of this wave are rows P39-INV-1 to P39-INV-4, P39-FLG-1 to
P39-FLG-3 and P39-GSV-1 of `docs/01-decisions-log.md` (section "Phase 39"). The agents' handoffs are
`docs/dev/handoff/feat-p39-{gsv-page,ops-invites,ops-modules,ops-immutable}.md`.

## What merged

| Order | Branch | Conflicts and how they were resolved |
|---|---|---|
| 1 | `feat/p39-ops-modules` (OPS-FLG-001), de703bef | none |
| 2 | `feat/p39-ops-immutable` (OPS-IMM-003), d47c3da4 | `crates/sr-server/tests/authz_matrix.rs` `harness::apply`: kept ops-immutable's dispatch (`Normal`, `KillFeatures` -> `kill_features`, `KillAll` -> `guild_modules_off` + `kill_features`) and moved ops-modules' two changes into `kill_features` (the 31 `platform.tools.*` sub-flags are skipped and proven off by the read-back). The two 429 loops became one helper, `patch_past_429` (used by the feature kill and the guild module toggle). ops-modules' `SR_TEST_VOICE_PORT_BASE` and the `profiles.access = opt_in_public` seed merged cleanly. |
| 3 | `feat/p39-gsv-page` (GSV-PAGE-001), b609be58 | none (one `platform.game_servers` registration, in `app.rs`) |
| 4 | `feat/p39-ops-invites` (OPS-INV-001), e2039d87 | none; route matrix, settings catalog, OpenAPI, api-client and mocks needed no union (regenerated on the merged tree: identical) |

## Gaps closed at integration

- **Registry test.** `feature_registry.rs`: the `#[ignore]`d game-servers test is enabled (`game_servers_switch_is_registered`, using `sr_features::GAME_SERVERS_ID`), the branch-only negative test is deleted, `platform.game_servers` joined the roadmap list of the walk, the web fixture `registry.json` was refreshed (`SR_UPDATE_FEATURE_CATALOG=1`), and `registryScreens.test.tsx` lists the id, so Admin > Features renders it.
- **i18n-audit is zero.** The four old findings (Studio's PNG/JPEG/WebP options, the watch party link placeholder) moved to a new fragment `p39-integ-1.ts`.
- **Docs follow-ups of ops-invites.** `docs/06` 5.3 (`invites.default_credits`, the quotas, the prune job) and a new 5.3a (Lock the Doors); D-372's setting name is amended by the new row P39-INV-3 (D-372 itself is not rewritten); `deploy/OPERATOR.md` section 3 gains the invite settings table, the `invites.prune` job line, `profiles.access` and the module switches list.
- **Behaviour change recorded** (ops-modules): public profile pages need `platform.public_profiles` **and** `profiles.access = opt_in_public` (P39-FLG-1, `deploy/UPGRADE.md` release note).
- **CHANGELOG** "## Phase 39".

## Gates on the merged tree

Scratch databases `sr_p39_integ` (tests) and `sr_p39_e2e` (Playwright), both dropped at the end. `CARGO_TARGET_DIR`
`p39-ops-immutable-target`, `-j 2`, one cargo command at a time, `--test-threads=1` for Postgres suites.

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | clean |
| `clippy --all-targets -D warnings` on sr-features, sr-admin, sr-api, sr-auth, sr-db, sr-server, sr-voice, sr-activities, sr-watchparty | clean |
| sr-features, sr-admin, sr-auth, sr-watchparty, sr-activities, sr-voice (`--include-ignored`, Postgres) | all ok (sr-features 139, sr-admin 70, sr-auth 58, sr-watchparty 30) |
| sr-db `--include-ignored` (67 binaries, incl. `secret_columns` 2, `migration_versions` 1, `invite_controls` 3) | 347 passed, 0 failed |
| sr-api lib and tests (`route_matrix` 6, `openapi_names`) | 806 lib + every test binary ok |
| sr-server `--lib` (Postgres, incl. the invite prune tests) | 244 ok |
| sr-server `web_gate` / `feature_registry` / `authz_matrix` (incl. ignored) | 21 / 7 / 8 ok; no ceiling changed (`WEAK_CEILING`, `SHAPE_CEILING` identical to main) |
| sr-server `immutable_rules` (incl. ignored, 4 states x 3 groups) | 22 tests; `refusals_and_logs_hold_in_every_flag_state` **failed once** (run 1: "admin under KillFeatures", 6 failures, 127 s instead of ~25 s), then **green twice** (runs 2 and 3, every state and group proven). The failure lines of run 1 were not captured (the log was filtered); see "Not done" |
| `SR_SECURITY_NIGHTLY=1` full matrix, states normal, kill_features, kill_all, all six groups | green in 6 min 17 s: 15987 refusals compared per killed state, 0 disagree (144 and 146 moved status but stay refusals, e.g. 401/404 -> 403 on BYOS and jukebox routes); 90 / 98 admission failures of killed features dropped by design |
| OpenAPI (`sr-server openapi`), api-client `generate`, `mocks:generate`, `mocks:check` | regenerated: no diff (1147 operations) |
| Web `typecheck` (all packages) and e2e `tsc --noEmit` | ok |
| `lint` (eslint, all packages) and `prettier --check .` | clean |
| Vitest `apps/web` `--maxWorkers=2` | 422 files, 3829 tests passed |
| `i18n-audit` | TOTAL 0 |
| `build-web.mjs`, `check-entrance-isolation.mjs` | ok; landing 114.1/120 KB, auth 119.7/150, invite 111.8/120, profile 113.0/150 |
| `check-bundle-budget.mjs`, `check-css-tokens.mjs` (69 files), `check-ops-files.mjs`, `check-doc-links.mjs` | all ok |
| YAML parse (js-yaml 4.3.2) of `security-nightly.yml` and all 19 workflows | ok |
| Duplicate job ids (`*_JOB_ID`, 43) | none |
| Playwright Chromium (dev server on 5173, sr-server on 8080 with `memory:` storage): `doors-closed.spec.ts`, `exit-gate.spec.ts`, `lounge-shell.spec.ts` | 9 passed (1.0 min) |

## Not done

- **The KillFeatures flake.** One of three runs of `refusals_and_logs_hold_in_every_flag_state` failed 6 checks in the
  admin group under KillFeatures (the same group and state as wave 36's "2 failures", and that run's admin group took
  five times as long, which points at rate limiting or load rather than a weakened refusal). It did not reproduce in
  two more runs, nor in the full-matrix comparison (which runs the admin group under kill_features and kill_all). The
  failure lines were not kept. If CI shows it, read the `failures` list it prints.
- **The nightly's first CI run**: dispatch `security-nightly.yml` on the branch after the push (the commander).
  OPS-IMM-003 stays unticked until then.
- Firefox and WebKit Playwright (only Chromium locally); no e2e spec for the inert Game Servers page.
- The per-server jukebox switch hides nothing in the web strip (ops-modules "still open"); the datachannel kill has no
  live two-browser check.
- The operator handbook page for the switches belongs to wave 40 (ops-docs); `deploy/OPERATOR.md` lists them meanwhile.

## Owner items

- P39-FLG-1 is a behaviour change on upgrade (public profiles need `profiles.access = opt_in_public` too).
- Confirm the defaults P39-INV-1 ("never activated"), P39-INV-2 (the lock also blocks emergency invites) and O-6
  (`profiles.access` walled, `invites.locked` off).

## Resume here

Nothing in flight. Next: push `integration/phase39`, open the PR (title below), dispatch `security-nightly.yml`.
PR title: `Phase 39: GSV-PAGE-001, OPS-INV-001, OPS-FLG-001, OPS-IMM-003 (operator switches, invite controls, the final IMMUTABLE RULE suite, Game Servers coming soon)`.
