# OPS-TDEP-001: test-only same-host activity origin mode (feat/test-deploy-origin)

Base: integration/phase46 (wave 46), merged with origin/main (PR #79 merged). Decisions D-450 to D-452, threat model
T49, ops note docs/ops/test-deploy-quick-tunnel.md.

## Done

- `[activities] origin_mode = subdomain | same_host_sandboxed` (`crates/sr-core/src/config.rs`, env
  `SR_ACTIVITIES__ORIGIN_MODE`); default `subdomain`, behaviour unchanged.
- Server: `/_activity/<id>/...` path routing in the activity layer (`crates/sr-api/src/routes/activities/proxy.rs`,
  `crates/sr-activities/src/rules.rs::app_of_path`); same policy with `sandbox`, `connect-src 'self'` narrowed to the
  activity path (`sandbox.rs::CspInput::self_source`); same-origin non-frame loads refused 403 and `Vary:
  sec-fetch-site, sec-fetch-dest` on answers (D-452); `.activities.` hosts 404 in this mode; writes need
  `Origin: null` + proxy token; implicit OAuth redirect follows the mode; guest join answer `frame_path`; guest-play
  `frame-src 'self'` (`web.rs::guest_play_csp_for`); startup WARNs (`crates/sr-server/src/test_deploy.rs`); client flag
  `deploy.test_same_host` in `GET /sr/features/flags`.
- Client: `activityBase` (`apps/web/src/activities/api.ts`), frame follows the flag; guest runner validates
  `frame_path`; "Test deployment" notice (`apps/web/src/testdeploy/`, fragment `i18n/messages/en/ops-tdep-1.ts`);
  `sw.js` bypasses `/_activity/`; vite dev/preview proxy `/_activity`.
- SDK `applicationIdFromLocation` / `isActivityLocation`; whiteboard, holdem, tabletop, landio, arena3d and the e2e
  test bundle read the id from the path form too.
- OpenAPI/api-client regenerated (`ArcadeGuestJoined.frame_path`); no route changed.

## Not done / open

- Same-host guest-play was not run end to end (it needs voice, which the tunnel test does not have).
- Release binaries were not built by me (see the ops note for the command).

## Resume here

Run the ops note. To re-prove: `SR_E2E_SAME_HOST=1 SR_E2E_WEB=server` with a server in the same-host mode,
`playwright test test-deploy-same-host.spec.ts --project=chromium`.

## Gates

| Gate | Result |
|---|---|
| cargo fmt --check; clippy --all-targets -D warnings (sr-core, sr-activities, sr-api, sr-server) | ok (clippy before the last small Vary edit; that edit compiled and its tests passed) |
| sr-core config / sr-activities lib / sr-api `routes::activities` (`--include-ignored`, Postgres) / sr-server lib `test_deploy` + `web::` | 16 / 50 / 20 / 14 pass |
| sr-server `immutable_rules` (`--include-ignored`, Postgres) | 25 pass |
| OpenAPI + api-client types + MSW handlers regenerated | `frame_path` added, handlers unchanged (1202 ops) |
| typecheck web, SDK, e2e; eslint web/SDK/holdem/whiteboard; prettier on touched files; i18n-audit; css tokens | ok |
| vitest: SDK 37; web testdeploy, guest-play, activities, moderation, security, i18n, pwa (303 + new); activities: arena3d, holdem, tabletop 86, whiteboard 71, landio 41 files pass (landio prints one vitest "onTaskUpdate" RPC timeout, also without the new test: environmental) | ok |
| build-web; bundle budget (app first-paint chunk 248.4/250 KB); entrance isolation | ok |
| Playwright Chromium `test-deploy-same-host.spec.ts` (SR_E2E_WEB=server, same-host mode) | 2 pass |
| Playwright Chromium `activities-live.spec.ts` + `guest-play.spec.ts`, default mode (dev web, voice on) | 13 pass |
| sr-server `authz_matrix`, full sr-api lib, `route_matrix`, `openapi_names`, `feature_registry`, `web_gate`, building the five activity bundles via `scripts/build-activities.mjs` (only whiteboard, plus the specs' builds of all five in activities-live) | **left to CI** (stopped on the coordinator's request) |
