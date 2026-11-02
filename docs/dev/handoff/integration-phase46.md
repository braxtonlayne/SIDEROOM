# Integration of wave 46 (ACT-A3D-001, ACT-LND-002): the 3D starter and the Land.io polish

Branch `integration/phase46`, base `origin/integration/phase45` 91628f9d (wave 45, PR #78 in CI), then
`origin/feat/p46-external` 21176e18 (the outside agent's arena3d and Land.io work) `--no-ff`. PR #78 had not merged
when this was written, so `origin/main` is not merged in yet (do it when #78 lands). Not pushed; no PR opened.

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

GATES_TABLE

## Not done / open

- Firefox replay hash: CI only (cannot launch locally).
- Live two-browser host handoff E2E; arena3d host handoff; guest relay surviving the creator's departure.
- Crash reporter reading arena3d's state hash (h, above).
- The outside report's open arena3d points (hidden-tab host lag, GPU loss recovery, touch controls), KTX2.
- Merge `origin/main` once PR #78 merges.

## Owner items

Rows in [owner-checklist](../owner-checklist.md) section 4b, "The 3D starter and Land.io polish (wave 46)".

## Resume here

Nothing half-done. Next: when PR #78 merges, merge `origin/main`, then the owner's go-ahead to push
`integration/phase46` and open the PR "ACT-A3D-001, ACT-LND-002: arena3d 3D starter with the cross-engine replay gate,
Land.io Draft Night run, bot backfill and host handoff (wave 46)". Watch the new `arena3d-replay` job: it is the gate
proof on Firefox.
