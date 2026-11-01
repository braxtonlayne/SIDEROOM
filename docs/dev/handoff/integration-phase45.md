# Integration of wave 45 (OPS-CRASH-001, UX-SHOW-001): crash telemetry and the Showroom preview

Branch `integration/phase45`, base `origin/main` e80ec08b (waves 41-44), then `feat/p45-crash-telemetry` (ac2cae7e,
OPS-CRASH-001) and `feat/p45-showroom` (4eefb75f, UX-SHOW-001), both `--no-ff`, and at the end `origin/main` 3bfca86b
(PR #76: roadmap reorder, one doc comment in `sr-activities/src/arcade.rs`) and 3056e2ef (PR #77: docs only). Not
pushed; no PR opened.

Handoffs: [feat-p45-crash-telemetry](feat-p45-crash-telemetry.md), [feat-p45-showroom](feat-p45-showroom.md).
Decisions D-430 to D-434 (45-A), D-435 to D-437 (45-B). Threat model T45, T46. Migration 2160 (45-A); the integrator
added none.

## Merge conflicts and how they were resolved

- `docs/inventory/21-feature-phase.md`: one "## Wave 45 — Crash telemetry and Showroom preview (roadmap P28)" header
  (both branches wrote it identically) with "### OPS — Crash telemetry" then "### UX — Showroom".
- `docs/15-threat-model.md`: T45 then T46. `docs/01-decisions-log.md`: D-430..D-434 then D-435..D-437.
- `apps/web/src/app/main.tsx`: 45-A's `CrashBoundary` wraps the tree; 45-B's `ShowroomBanner` is mounted inside
  `CatalogGate` next to `ReportDialog`.
- Merged clean: the i18n index, `registry.json` (the `SR_UPDATE_FEATURE_CATALOG=1` run left it unchanged),
  `feature_registry.rs`, OpenAPI / api-client / MSW handlers (regenerated once after the last code merge: no drift,
  1202 operations, `mocks:check` ok; PR #76's "46-B" -> "48-B" description is in).

## What the integration changed

1. **Bots are refused on `POST /crash-reports`.** The route-matrix row (45-A) said BOT `deny` and T45 says bots are
   refused, but `auth::policy::bot_denied` allowed the path, so `route_matrix::matrix_covers_every_route` failed on
   the merged tree. The policy now denies it (`crates/sr-api/src/auth/policy.rs`); route_matrix 6, authz_matrix 11.
2. **Showroom and crash telemetry together** (`apps/web/src/showroom/__tests__/showroom-crash.test.tsx`, with the real
   router, `installCrashTelemetry` and the default sender over MSW): clicking every Play / Notify me sample button
   sends no crash report and leaves the tick ring empty; an unhandled error while the preview is on (its message,
   stack text and a frame URL stuffed with every fixture string) still sends exactly one report with
   `route: "games"` and none of the fixture strings, raw or URL-encoded. No code change was needed: fixture
   interactions only call `previewHint`, and the allow-list never copies messages, URLs or queries.
3. **The emulator feeds the tick ring** (cheap, so done): `emu/input.ts::InputPoller.poll` records each local press
   as `recordTick('emulator', button)` (the RetroPad button as an enum; L/R/L2/R2/L3/R3 become `other`, never a key
   code or pad id). Remote co-op input does not pass through `poll` and is not recorded. Test in
   `emu/__tests__/gamepad.test.ts`. `ringBuffer.ts` is already in the app shell, so the emulator route is unchanged
   (270.8 of 320 KB).
4. **Bundle budget.** The real rows are in `scripts/bundle-budget.json`: `budgets.app` 300 KB gzip (the app shell's
   whole first-load closure; 45-B's figure) and `singleChunkMaxGzipKB` 250 KB (any single chunk on the first paint;
   45-A's figure). The merged app shell is one chunk of **248.2 KB**, so both pass, with 1.8 KB of headroom under the
   single-chunk limit. The crash report builder (`app/report-*.js`) and the fixtures (`app/fixtures-*.js`, with
   `PreviewStudy`, `PreviewArcade`, `PreviewRailItems`) are lazy; the app chunk has no static import of either.
   Entrance isolation: every public surface clean.
5. **Showroom spec, first run.** It failed once: its "no `/shelves/` request" filter matched the Vite dev server's
   module URLs under `/src`. It now compares URLs (not `Request` objects) and only `/api/v1/(arcade/front|shelves/)`.
   Everything else (selectors, counts 6 / 5 / 3 / 4 / 10) was right.
6. **CI wiring.** `crash-telemetry.spec.ts` is `@gate` (the "Exit gates" step, all three engines, 45-A).
   `showroom.spec.ts` is untagged and runs in the main e2e step on every engine (comment added in `e2e.yml`): it
   signs the Owner in with a password to flip `platform.showroom`, which would fail after the @gate specs enrol a
   second factor on the Owner. The `Crash card secret-pattern grep` step in `repo-checks.yml` parses (js-yaml) and
   `check-crash-card.mjs --self-test` passes (14 patterns trip on the hostile payload, 0 on the card).
7. **i18n-pseudo** gained a "showroom preview" row (`/study` with the switch on for that row only and the device
   choice in `localStorage`; it asserts the banner, the toggle and the sample Study are visible before the scan) at
   desktop and phone width; an `afterEach` switches it off again.

## Gates

Scratch DBs `sr_p45_integ` (cargo) and `sr_p45_e2e` (Playwright), both dropped at the end. `-j 2`, one cargo or test
process at a time.

| Gate | Result |
|---|---|
| `cargo fmt --all --check`; clippy `--all-targets -D warnings` (sr-api, sr-db, sr-admin, sr-server, sr-features; sr-api again after the policy fix) | ok |
| sr-db `secret_columns` / `migration_versions` | 2 / 1 pass |
| sr-api lib (`--include-ignored`) / `route_matrix` / `openapi_names` | 826 / 6 (after the fix; 5+1 fail before) / 2 pass, `KNOWN_CLASHES` empty |
| sr-features | 139 pass |
| sr-server lib (`--include-ignored`) / `web_gate` / `feature_registry` | 253 / 22 / 7 pass |
| sr-server `immutable_rules` (`--include-ignored`) | 25 pass |
| sr-server `immutable_rules` nightly, `SR_SECURITY_NIGHTLY=1 SR_IMMUTABLE_STATES=normal,kill_features,kill_all`, all groups | pass (597 s) |
| sr-server `authz_matrix` (`--include-ignored`) | 11 pass; no ceiling, `WRITE_OBSERVED` or row changed |
| OpenAPI, api-client types, MSW handlers regenerated once (after PR #76); `mocks:check` | 1202 operations, no drift |
| `pnpm typecheck` (incl. e2e), eslint, `prettier --check .` | ok |
| vitest full `--maxWorkers=2` (wasm built first) | 444 files, 4051 tests pass |
| `node --test scripts/*.test.mjs` (threat-model, secret-scan, check-crash-card, entrance isolation, ...); `secret-scan.mjs`; `i18n-audit`; `check-css-tokens` | 107 pass; clean; TOTAL 0; 69 files clean |
| `build-web.mjs`; `check-bundle-budget.mjs`; `check-entrance-isolation.mjs` | ok; every row ok (app 248.2/300, single chunk under 250); every public surface clean |
| Playwright Chromium, fresh database, sr-server started before the Owner bootstrap (as CI), dev server: crash-telemetry, showroom, exit-gate, lounge-shell | 10 pass |
| Playwright Chromium: i18n-pseudo (all rows + "showroom preview" desktop and phone) | 32 pass |

## Not done / still open

- Firefox and WebKit not run locally (CI runs them; the crash spec skips outside Chromium by design).
- The app chunk is 1.8 KB under the 250 KB single-chunk limit: the next eager addition will trip it.
- From the branch handoffs, unchanged: no admin page for crash reports; no offline crash queue; `#crash-triage` made
  at runtime shows in staff clients on reload; a WASM panic the app catches itself is reported only where code calls
  `reportWasmPanic`; the Showroom does not overlay the Lounge channel views, Cinema or Library.

## Owner items

Rows added to [owner-checklist.md](../owner-checklist.md) section 4b: crash opt-out / GPC (D-432), auditing list
reads (D-433), crash limits as Admin > Limits settings, who sees `#crash-triage`, the Showroom switch in production
builds (D-435), the Showroom over the Lounge (D-436).

## Resume here

Nothing half-done. Next: the owner's go-ahead to push `integration/phase45` and open the PR
"OPS-CRASH-001, UX-SHOW-001: crash telemetry ring buffer and #crash-triage, Showroom populated preview (wave 45)".
