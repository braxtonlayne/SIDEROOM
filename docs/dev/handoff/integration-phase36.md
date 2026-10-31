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

RESULTS_PLACEHOLDER
