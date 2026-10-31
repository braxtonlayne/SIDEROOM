# Integration, phase 36: activities with shared state (ACT-CH-001, ACT-WB-001, ACT-CT-001, ACT-TT-001, ACT-LND-001)

Branch `integration/phase36` (base `main` 5b242383; PR #67, wave 35b, had not merged when this ran, so the commander
merges main later). Not pushed; the commander opens the PR. Commander defaults of this wave are rows P36-ACT-1 to
P36-ACT-5 of `docs/01-decisions-log.md` (section "Phase 36"). The agents' handoffs are
`docs/dev/handoff/feat-p36-{act-channel,act-whiteboard,act-cards,act-tabletop,act-landio}.md`.

## What merged

| Order | Branch | Conflicts and how they were resolved |
|---|---|---|
| 1 | `feat/p36-act-channel` (ACT-CH-001), 2e14327e | none |
| 2 | `feat/p36-act-whiteboard` (ACT-WB-001), 072a8947 | `packages/embedded-app-sdk/src/{data,index,sdk}.ts`: the branch carried only act-channel's stub 6beb30a2; act-channel's final SDK taken (fbbe151a: chunked join snapshot, tombstone expiry, `sdk.self`/`sdk.hostId`/`sdk.isHost`, public `sdk.data`) |
| 3 | `feat/p36-act-cards` (ACT-CT-001), 1d779856 | same three SDK files, same resolution |
| 4 | `feat/p36-act-tabletop` (ACT-TT-001), 802971e4 | same three SDK files, same resolution |
| 5 | `feat/p36-act-landio` (ACT-LND-001), c20e6ad2 | same three SDK files, same resolution |

None of the activity branches touched the SDK beyond the stub (checked with `git diff 6beb30a2 <branch> --
packages/embedded-app-sdk`), so the merged SDK is byte-identical to act-channel's.

## Gaps closed at integration

- **Identity reconciled to the final SDK.** Every activity takes its member id and the host from `sdk.self` /
  `sdk.hostId` only: the whiteboard dropped its sessionStorage id (`memberId` now throws without `sdk.self`), holdem
  dropped `config.member_id`/`host_id`, tabletop dropped the `sdk.data` fallback (its test no longer assigns the
  read-only getters), Land.io dropped `selfId`/config. The whiteboard's `it.fails` late-join-after-compaction test is a
  plain `it` and passes.
- **One manifest.** `sideroom.json` (id, name, version, description, `capabilities: ["datachannel"]`) sits at each
  activity's folder root and ships at the bundle root: a small Vite plugin emits it for the whiteboard and holdem
  (their `dist` had none before, so the SDK would never have attached the data channel), tabletop's `build.mjs`
  copies it, Land.io's `tools/bundle.mjs` reads it (its `activity.json` is gone; it would not have been read by
  `bundle.rs`). `pack.json` is gone.
- **Workspace packages.** `pnpm-workspace.yaml` lists `packages/starter-packs/activities/*`; each activity has a
  `package.json` (`build`, `test`, plus `typecheck`/`lint` where it had them; whiteboard and holdem depend on
  `@sideroom/embedded-app-sdk: workspace:*`). The lockfile was updated with `pnpm install --offline`: importer rows and
  peer-variant snapshots of packages already in the store, no new package. `pnpm -r test` (and so CI's `web.yml`) now
  runs the four suites.
- **How built-ins reach the shelf (P36-ACT-4).** `node scripts/build-activities.mjs [name...]` builds each package's
  `dist/`, checks `index.html` and a `sideroom.json` declaring `datachannel` at the root and no zip inside, and writes
  `packages/starter-packs/activities/dist/<name>.zip`; an operator uploads it in the developer portal and staff approve
  it. e2e installs them the same way (`builtInActivityBundle`). There is no `sr-cli` seed for Activities yet.
- **The host leaving ends the instance (P36-ACT-1).** `remove_participant` in `crates/sr-activities/src/service.rs`
  ends an instance whose reviewed bundle declares `datachannel` (every capability in development) when its starter
  leaves it or the call; other Activities keep the last-one-out rule. Postgres test added to
  `an_instance_carries_the_capabilities_of_its_reviewed_bundle`. T32 in `docs/15-threat-model.md` says so.
- **Live cases.** The four agents' cases moved to `e2e/support/activity-cases/*.ts` and run from
  `e2e/tests/activities-live.spec.ts` ("Built-in activities through the data channel"), each after
  `startActivityForTwo` (`e2e/support/activities.ts`) installs the real bundle over the API and starts it for two
  people in one call. The whiteboard's "reload" became Leave and Join of the instance (a reload drops the call);
  Land.io's last step checks that the host leaving closes the guest's frame.

- **Bug found by the live run (ACT-TT-001).** `runHost` had no default roll seed, so every roll in the real tabletop
  threw on the host (the unit tests always injected one). `cryptoSeed()` (the browser's CSPRNG) is the default now,
  with a test that rolls without an injected seed.
- **Live window.** The cases run at 1280 x 1000: at the 720 px default the Activity frame (60vh, below the call
  header and the jukebox) ends below the viewport and the whiteboard's page bar could not be clicked. A real member
  on a short window has to scroll the channel the same way; worth a look by whoever owns the activity panel layout.
- **Main merged twice.** First at 5b242383 (nothing to do), then 28bdcbf9 (wave 35b, PR #67) before the final gates:
  `CHANGELOG.md` and `docs/01-decisions-log.md` conflicted only by both adding a section (kept both, Phase 36 first).

## Gates (merged tree, after main 28bdcbf9)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | ok |
| clippy `-D warnings --all-targets`: sr-core, sr-e2ee, sr-e2ee-wasm, sr-voice, sr-activities, sr-sdk, sr-db, sr-bridge, sr-server | ok |
| `sr-e2ee` (37 + 1), `sr-voice` (43 unit, 6 datachannel, e2e/loopback/video/music), `sr-sdk` incl. `voice_data`, `sr-core --lib` (43) | ok (run before the second main merge, which touched none of them) |
| `sr-activities` lib + db (Postgres, scratch DB) | 17 + 8 ok |
| `sr-db --test secret_columns --test migration_versions` | ok |
| `sr-api --test route_matrix --test openapi_names` | ok (no new route) |
| `sr-server --lib` (Postgres) | 238 ok |
| `sr-server --test immutable_rules` | 14 ok after the merge. The run before the merge failed once: 2 failures in the `admin` group under `KillFeatures` (the detail lines were filtered out of the log); not reproduced on the rerun; nothing of this wave touches admin routes or flags. Watch it in CI |
| `sr-server --test web_gate` | 19 ok |
| `sr-server --test authz_matrix` | 7 ok, no ceiling changed |
| OpenAPI from `sr-server openapi` vs `packages/api-client/openapi.json`; api-client `generate`; `mocks:check` | identical; no diff; up to date (1121 operations) |
| `pnpm -r --if-present typecheck` (incl. e2e and the activities) | ok |
| `pnpm -r --if-present lint`, `prettier --check .` | ok |
| Vitest apps/web `--maxWorkers=2` | 404 files, 3692 tests ok |
| Vitest embedded-app-sdk / whiteboard / holdem / tabletop / landio | 23 / 71 / 62 / 86 / 299 (+1 skipped) ok |
| `build-web`, `check-entrance-isolation`, `check-bundle-budget` | ok, ok, ok |
| Playwright `activities-live.spec.ts`, Chromium, real sr-server (voice + activities, memory storage, scratch DB) and Vite | 5 passed (test activity, whiteboard, holdem, tabletop, landio), twice: before and after the main merge |

Scratch databases `sr_p36_integ` and `sr_p36_e2e` were dropped. Ports 8080/5173/8081 are free (the servers this run
started were stopped).

## Not done

- **Seeding built-ins** (`sr-cli package seed` for Activities): the sticker seed does not know applications;
  P36-ACT-4 records the developer-portal path for now.
- **Host handover**: not built (P36-ACT-1); the instance ends instead.
- **Images from a mount** in the whiteboard and the tabletop, **PNG download** from the whiteboard (it opens in a new
  tab): need owner decisions (below).
- Firefox and WebKit live runs (CI's), translations of the activities' strings (P36-ACT-5), the whiteboard's live
  cursors and object moving, tabletop commit-reveal of roll seeds, Land.io's host scheduler in a Worker.

## Owner items

1. **Images in Activities.** The Activity CSP allows images only from `self`, `data:` and `blob:`, so a whiteboard
   picture or a tabletop map cannot come from a mount by URL. Options: an SDK command that hands the Activity a mount
   file the member may read (bytes or a `blob:` URL), or an `img-src` allowance for the instance's media host.
2. **A download right for Activities.** The sandbox has no `allow-downloads`; the whiteboard's PNG export opens in a
   new tab. Options: `allow-downloads`, or a host `saveFile` command that asks the member.
3. **Host handover.** Today the host leaving ends the instance (P36-ACT-1). A successor protocol needs the gateway to
   name a new `started_by` and each activity to hand over its state (Land.io's arena, the dealer's deck).
4. Commander defaults P36-ACT-1 to P36-ACT-5 stand until the owner changes them.

## CHANGELOG

"## Phase 36 - 2026-10-31" at the top of `CHANGELOG.md` (four bullets, from the five handoffs).
