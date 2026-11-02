# Integration of wave 46 (ACT-A3D-001, ACT-LND-002): the 3D starter and the Land.io polish

Branch `integration/phase46`, base `origin/integration/phase45` 91628f9d (wave 45, PR #78 in CI), then
`origin/feat/p46-external` 21176e18 (the outside agent's arena3d and Land.io work) `--no-ff`. PR #78 had not merged
at first; `origin/main` 4d482ddd (PR #78 merged: three CI-flake fixes) was merged in before the final gates. Not
pushed; no PR opened.

Outside report: [feat-p46-external](feat-p46-external.md) (brief: [wave46-external-brief](wave46-external-brief.md)).
Decisions D-440 to D-448. Threat model T47, T48. Migration 2200 (`activity_instances.host_id`).

## Merge

Clean: the outside branch touched only `packages/starter-packs/activities/{arena3d,landio}/**`, its report and the
`jolt-physics` 1.1.1 lines of `pnpm-lock.yaml` (the only approved new dependency; no other was added).

## What the integration did (outside report section 4, in order)

a. **build-activities registers arena3d** (`scripts/build-activities.mjs`). `checkTree` now refuses a built manifest
   that lost or changed the package manifest's `capabilities` or `connect` list, so `connect: ["self"]` reaches the
   zip the server reviews; the server already reads it into the reviewed connect list and serves `connect-src 'self'`
   (`sr_activities::sandbox::tests::the_arena3d_manifest_survives_review_with_connect_self` on the real file). The
   built-in `connect` is the default (D-441), listed for the owner. `scripts/build-activities.test.mjs` (5). All five
   activities build and zip.
b. **The gate proof spec.** `e2e/playwright.arena3d.config.ts` serves `arena3d/dist` with `e2e/tools/serve-dir.mjs`
   (no sr-server) and `e2e/arena3d/replay.spec.ts` waits for `#hash[data-state="done"]` and compares the final and the
   float32-exact hash with `node replay/run-node.mjs --json` (`final`, `finalExact`, `ticks`), once per engine; every
   engine equal to the same Node result means all three equal each other. CI: new `arena3d-replay` job in `web.yml`
   (no WASM or web build; installs chromium, firefox and webkit; `pnpm --filter @sideroom/e2e test:arena3d`).
   Local: Chromium and WebKit both `6fdea5719b5383d6` / `e35977c2a4cddeed`, equal to Node. Firefox does not launch on
   this machine (VC runtime, `spawn UNKNOWN`); CI runs it. If CI shows an engine disagreeing: stop and tell the owner
   (D-448), no engine switch.
c. **arena3d smoke** (`e2e/arena3d/smoke.spec.ts`, Chromium): `?user=a` and `?user=b` in one context over the fake
   platform; both press Play, a hosts, b joins, both rosters list `a` and `b`; no page errors. Headless runs on the
   WebGL2 fallback.
d. **guest-play.** The wave 43 spec passed unchanged (8/8) against the new build. Then a step was appended to the
   real-stack test: Alice (host) switches to Draft Night from her start screen; a phone guest (touch, 375 x 740) opens
   the link, presses Play, gets "Keeper pick", taps the first card, which lands in their hand while the offer moves on
   to the life's normal draft. 8/8 again.
e. **Host reassignment (D-388) as the `host_handoff` capability** (D-444, D-445):
   - server: `host_handoff` is an active flag (`arcade::FLAGS`, `bundle::HOST_HANDOFF`); migration 2200 adds
     `activity_instances.host_id` (NULL = the starter). `remove_participant` moves the host to the earliest-joined
     remaining participant (ties by user id) only for a data-channel game that declares the flag and only if the
     leaver is still the host; `InstanceView.host_id` carries it; the guest seating's "Host" label follows it. Other
     games still end with their host. No route changed (no route or authz matrix rows).
   - SDK: `HOST_CHANGED { host_id, previous }`; `ActivityData.changeHost` (the host is no longer fixed at attach): only
     the new host writes `host:*`, gets inputs and answers `sync`; it re-stamps the host keys it holds and broadcasts
     a snapshot, others ask it for the document. `sdk.on('host')`, `sdk.hostId`, `config.host_id` follow.
   - web bridge: `ready` names `hostOf(instance)`; `hostChanged()` sends HOST_CHANGED once per change; the guest bridge
     re-announces the host to the lobby; the guest runner forwards HOST_CHANGED (added to the guest event allow-list).
   - Land.io: declares `host_handoff`; `sdk-channel` follows the host; `transport.js` turns on snapshots
     (`HANDOFF_SNAPSHOT_TICKS` 35, 5 s at 7 Hz) and calls `transport.handoff.hostChanged()`. The call is in
     `client/net/transport.js` rather than `sideroom-platform.js`, because that is where the channel and the
     transport meet; the platform supplies the channel.
   - Not done: a live two-browser Playwright handoff (the proofs are the server DB test, SDK tests, bridge tests and
     the Land.io real-SDK test); arena3d does not declare `host_handoff` (it would need to resume from `host:snap`);
     the guest relay ends when the link's creator was the host (owner checklist).
f. **`byos_read` shelf objects** (D-446): `sdk.commands.shelfObjectUrl { shelf_id, object_hash }`, the member asked
   once per object and frame, minted by the 41-C route with the member's session; refused without the capability and
   absent from the guest allow-list. The CSP is not widened: the conservative default, with the owner's options in
   [byos-read-activities](../design/byos-read-activities.md). arena3d's `ShelfAssetSource` uses it (path
   `<shelf id>/<object hash>`).
g. **Land.io timing tests** (D-447): `test/support/timing.js` times mode and baseline in 7 alternating 400-tick batches
   on fresh same-seed states and compares the median per-pair difference with the same 0.3 ms budget. Load moves
   whole batches, which the median drops; a real +0.3 ms is in most pairs and still fails. `test/timing.test.js`
   proves both with a fake clock. Applied to draft, shrink and teams (the three that flaked); koth, lastland and
   infection keep their old single-run checks (they did not flake).
h. **Crash reporter reading `window.arena3d`: not done.** The game runs in the sandboxed opaque-origin frame, so the
   app page cannot read its `window`; it would take a new SDK command and an allow-list entry, which is not cheap.
i. Docs: inventory rows, D-440..D-448, T47/T48, CHANGELOG "Phase 46", owner checklist rows, this file.

## Gates

Scratch DBs `sr_p46_integ` (cargo) and `sr_p46_e2e` (Playwright), both dropped at the end. `-j 2`, one cargo or test
process at a time. By the owner's speed decision, gates marked **CI** were left to GitHub CI on the PR.

| Gate | Where | Result |
|---|---|---|
| `cargo fmt --all --check`; clippy `--all-targets -D warnings` (sr-activities, sr-db, sr-api, sr-server) | local | ok |
| sr-db `secret_columns` / `migration_versions` (migration 2200) | local | 2 / 1 pass |
| sr-activities lib / arcade / db / trust_db (`--include-ignored`) | local | 48 / 7 / 13 / 9 pass |
| sr-api lib (`--include-ignored`) / `route_matrix` / `openapi_names` | local | 826 / 6 / 2 pass; no route changed, `KNOWN_CLASHES` empty |
| sr-server `feature_registry` | local | 7 pass |
| sr-server `authz_matrix` (`--include-ignored`) | local | 11 pass on the first run; after the merge 10 pass, 1 fail ONLY on `staff_alerts` (see below) |
| sr-server `immutable_rules` (`--include-ignored`) | local | 24 of 25 pass; `refusals_and_logs_hold_in_every_flag_state` failed only on background-job tables (see below) |
| OpenAPI, api-client types, MSW handlers regenerated once; `mocks:check` | local | 1202 operations, `InstanceView.host_id` added, no drift |
| `pnpm -r typecheck` (incl. e2e, both activities, SDK) | local | 0 errors |
| eslint (`pnpm -r lint`), `prettier --check .` | local | clean (only the gitignored `e2e/.state` file, since removed) |
| vitest full `--maxWorkers=2` (web) | local (before the speed decision) | 446 files, 4062 tests pass |
| SDK vitest | local | 34 pass |
| `node --test scripts/*.test.mjs`; `secret-scan`; `i18n-audit`; `check-css-tokens` | local | 110 pass (112, 2 skipped); clean; TOTAL 0; 69 clean |
| `build-web.mjs`; bundle budget; entrance isolation | local | ok; app chunk 248.2 KB (unchanged, 1.8 KB headroom), activities 484.5/550; every public surface clean |
| Land.io suite / arena3d suite / both tsc / both bundles | local | 330 pass, 1 skipped (40 files) / 56 pass / clean / 93 and 51 files, every check |
| `build-activities.mjs` all five | local | whiteboard, holdem, tabletop, landio, arena3d zipped |
| Playwright `arena3d/replay.spec.ts` | local Chromium + WebKit (pre-merge tree); **CI** for Firefox and the final tree | equal to Node: `6fdea5719b5383d6` / `e35977c2a4cddeed` |
| Playwright `arena3d/smoke.spec.ts` | local Chromium (pre-merge tree); **CI** final | pass |
| Playwright `guest-play.spec.ts` | local Chromium, final tree | 8 pass (unchanged spec first, then with the Draft Night step) |
| Playwright `exit-gate`, `lounge-shell` | local Chromium, final tree | 6 pass |
| Firefox / WebKit for the server specs, the nightly immutable matrix | **CI** | not run locally |

**The two Rust failures are environmental, not wave 46.** Both are the "refused calls changed tables" check:
- `invite_prune_runs`: the `invites.prune` singleton first runs 60 s after server start and only it writes that table;
  it landed inside the admin group in 3 of 3 runs. Added to `NOISY` in `authz_matrix.rs` (the same kind of fix as
  main's insights tables). After that it no longer appears.
- `staff_alerts`: the health job (also +60 s) raises a disk alert because this machine's C: is 88% full and the
  default `alerts.disk_percent` is 80. The `SettingsMax` state (threshold 99) is the one state with no
  `staff_alerts` change, which confirms it. NOT added to `NOISY` (a route can ack alerts, so ignoring the table could
  hide a real write). CI's disks are not that full; if CI shows it, the fix is a harness-level disk threshold, not
  `NOISY`. The immutable suite was not re-run after the `NOISY` change (owner speed decision): **CI**.

## Not done / open

- Firefox replay hash: CI only (cannot launch locally).
- Live two-browser host handoff E2E; arena3d host handoff; guest relay surviving the creator's departure.
- Crash reporter reading arena3d's state hash (h, above).
- The outside report's open arena3d points (hidden-tab host lag, GPU loss recovery, touch controls), KTX2.

## Owner items

Rows in [owner-checklist](../owner-checklist.md) section 4b, "The 3D starter and Land.io polish (wave 46)".

## Resume here

Nothing half-done; `origin/main` (PR #78) is merged. Next: the owner's go-ahead to push
`integration/phase46` and open the PR "ACT-A3D-001, ACT-LND-002: arena3d 3D starter with the cross-engine replay gate,
Land.io Draft Night run, bot backfill and host handoff (wave 46)". Watch the new `arena3d-replay` job: it is the gate
proof on Firefox.
