# Integration: phase 41 (The Study reframe)

## Status

Complete on `integration/phase41` (2026-11-01). Not pushed, no PR. All scratch databases are dropped (`sr_p41_integ`,
`sr_p41_e2e`, `sr_p41_study_nav`, `sr_p41_shelves`, `sr_p41_study_zk`). No server or Vite is left running, and
ports 8080, 5173 and 8081 are free. The work was interrupted once by an app restart (04:55) and resumed at 05:08.

## Merges and reconciliation (done)

- `origin/main` = 57b91ca9, no-op. Merged `--no-ff` in this order: feat/p41-study-zk, feat/p41-study-nav, feat/p41-study-shelves (one conflict, in `crates/sr-api/src/routes/storage/mod.rs`).
- **shelf_objects:** 41-C owns it. `2010_shelf_objects.sql` was dropped: it never shipped, and it would have broken a fresh database because 2020 does a plain `CREATE TABLE`. 41-B's `GET|PUT /storage/mounts/{mount_id}/notes` routes, its store methods (`file_notes`, `set_file_note`, `delete_file_note`), its `shelf_repo` test, and its route_matrix and authz rows are dropped. 41-C's catalog and `PATCH /shelves/{shelf_id}/objects/{object_hash}` cover them. Both matrices stay consistent: 41-C's rows only.
- **Edit rights (D-402):** the PATCH now decides with `scopes::may_manage` (owner; on a server shelf, a manager). A grantee gets 403; anyone who cannot reach the shelf gets 404. The catalog page carries `can_edit`.
- **Trigger fix:** the 2020 sync trigger now skips `storage_objects` rows whose mount is gone (`storage_objects` has no FK). The `sr-db shelf_repo` tests failed on the merged tree without this. 2020 has not shipped, so it was edited in place. Any database that already applied the old 2020 (the agents' scratch DBs) has a checksum mismatch: recreate it.
- **Web:** `visibleShelves` fetches `GET /shelves/visible`. `owner_handle` and `guild_name` are the badge fallbacks. Notes come from the catalog and are saved through the PATCH (hash = sha256 of the path, `study/objectUrl.ts`). Covers, the viewer, the player and the readers read from `GET /shelves/{id}/objects/{hash}/url`. If the catalog returns 404 (the indexer has not reached the file yet), they fall back to the mount presign, which applies the same scope and delisting rules. `ShelfPlayer`, `CbzReader` and `PresignedReader` take an optional `presign` prop.
- **Nav:** RoomsNav's The Study goes to `/study`. `StudyAreas` adds Shelves before Media and Reading. `/study` shows the room bar.
- **Threat model:** T39 is unique. Its stale reference `::object_url` is fixed to `::shelf_object_url`, and its residual cites D-401.
- **Decisions:** a new "Phase 41" section in the decisions log. D-387 to D-399 from roadmap-p28 section 0. 41-A's D-393 is renumbered D-400 (docs 00, 10 and 13 updated). The integrator defaults are D-401 to D-404.
- **D-404:** `module_gate` 404s `/submissions`, `/submissions/*` and `/library/submissions/*` while `platform.ingest_queues` is off.
- **e2e:** `support/studySwitches.ts` turns the five switches on in beforeAll and off in afterAll, for cinema-submit, library-gate, a11y-signed-in and visual. `study-shelves.spec.ts` now fakes 41-C's routes and proves the video src is the per-object URL.
- **Regenerated once:** OpenAPI, api-client, MSW (`mocks:check` ok).

## Fixes found by the gates on the merged tree

- `openapi_names`: 41-C's `GrantBody` and `GrantView` used the same names as the donations DTOs, so the OpenAPI
  document carried only one of each. The shelf types are now `ShelfGrantBody`, `ShelfGrantView` and `ShelfGrantList`,
  and the client is regenerated.
- `sr-db shelf_repo`: the 2020 trigger failed the indexer for a mount that is gone. Fixed in place.
- Full Vitest: `review/__tests__/requests.test.tsx` assumed the Requests board was on. It now switches it on.
- `threat-model.test.mjs`: T39 named the old handler `object_url` and said "open" without an RDY item. Fixed.

## Gates (merged tree, 2026-11-01)

| Gate | Result |
|---|---|
| cargo fmt --check (sr-features, sr-api, sr-server, sr-theme, sr-db, sr-storage, sr-moderation, sr-activities) | ok |
| clippy -D warnings, same crates, --all-targets | ok |
| sr-db full suite (--include-ignored, incl. secret_columns, migration_versions, shelf_access, shelf_repo) | 351 pass |
| sr-api full suite (lib incl. module_gate, route_matrix, openapi_names) | 887 + 1 pass after the schema rename (route_matrix 6, openapi_names 2) |
| sr-features, sr-theme, sr-storage, sr-moderation, sr-activities (--include-ignored) | 498 pass |
| sr-server --lib (--include-ignored) / web_gate / feature_registry | 245 / 21 / 7 pass |
| sr-server immutable_rules (plain / --include-ignored, all states) | 14 / 23 pass |
| sr-server authz_matrix (full, --include-ignored) | 9 pass, incl. shelf_access_holds_over_http, module_kill_switches_stop_their_routes; no ceiling changed in the wave diff |
| OpenAPI + api-client + MSW regenerated (after the rename), mocks:check | ok |
| pnpm -r typecheck (incl. e2e), -r lint, prettier --check . | ok |
| Vitest full suite, --maxWorkers=2 | 426 files, 3866 tests (the 5 Requests board tests failed, were fixed and rerun green) |
| i18n-audit / check-css-tokens | 0 / clean |
| build-web, bundle budget, entrance isolation | ok |
| threat-model, check-ops-files, entrance script tests, doc links | pass |
| Playwright Chromium: exit-gate, lounge-shell, study-shelves | 8 pass |
| Playwright Chromium: a11y-signed-in (the storefront switches on for the run) | 38 pass |

Local Playwright ran without Garage. `sr-server` was started with the storage settings of `.env.example`, so
`/readyz` stayed 503 on storage and the runner waited on `/healthz` instead. cinema-submit and library-gate skip
locally (`SR_E2E_LIVE`). The live CI job runs them with the switches on through `e2e/support/studySwitches.ts`.
The visual spec was not run (its baselines are CI's).

## Roadmap gate proof, where it stands

- **Zero outbound requests to metadata providers by default:** proved.
  `lookup_wiring::with_default_settings_no_provider_is_offered` covers the server, the web source scan in
  `study/__tests__/study.test.tsx` covers the client, and the study-shelves spec records no foreign host.
- **Storefront routes 404 with their switches off:** proved. Covered by `module_gate` unit tests and
  `authz_matrix::module_kill_switches_stop_their_routes`.
- **A grantee never sees a credential; a delisted hash is 404 on mint and gone from every catalog:** proved.
  Covered by `authz_matrix::shelf_access_holds_over_http`.
- **Shelf browsing and playback end to end through per-object URLs:** partly proved. Locally the
  `study-shelves.spec.ts` page flow runs against a faked server. The real two-browser-profile run against a real
  bucket needs Garage (CI live job), and no such spec exists yet.

## Open (owner)

See the owner checklist, section 4b "The Study (wave 41)": D-401 to D-404, the conflict between D-390 and P37-EMU-2
on the PS1 HLE BIOS, and shelf strikes vs the takedown counter. Also not built in wave 41: the owner's client
uploading its own index (the catalog comes from the server indexer, and `object_hash` is the path hash, not a content
hash), the sandbox CSP allow-list for `byos_read`, a web UI for grants and delisting, and a batch URL mint for covers.
A real-bucket, two-browser-profile Playwright run is not possible locally (no Garage). The server side of that proof
is `authz_matrix::shelf_access_holds_over_http`.
