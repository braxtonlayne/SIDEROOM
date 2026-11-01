# Integration: phase 41 (The Study reframe)

## Resume here

Stopped at 04:55 on 2026-11-01 because the owner restarted the app. Everything sound is committed on
`integration/phase41`. Nothing I started is still running: no sr-server, vite or fake, and the stopped `sr-db` test
process is gone. Scratch DBs: keep `sr_p41_integ` and `sr_p41_e2e` (mine). The agents' `sr_p41_study_nav`,
`sr_p41_shelves` and `sr_p41_study_zk` can be dropped at the end. Target dir:
`C:/Users/braxt/Desktop/SIDEROOM_wt/p41-study-zk-target`. The env helper (it builds DATABASE_URL from
`%TEMP%\sr_pg_pw.txt` and never prints it) is at
`%LOCALAPPDATA%\Temp\claude\...\scratchpad\env.sh`; rebuild it if the scratchpad is gone.

Next, in order, one cargo command at a time with `-j 2`:
1. `cargo test -p sr-db -- --include-ignored --test-threads=1` (the full suite; interrupted, no result yet).
2. `cargo test -p sr-api` (lib incl. module_gate; route_matrix; openapi_names).
3. `cargo test -p sr-features -p sr-theme -p sr-storage -p sr-moderation -p sr-activities` (DB tests with SR_TEST_DATABASE_URL).
4. sr-server: `--lib`, `--test web_gate`, `--test feature_registry`, `--test immutable_rules` (plain, then `-- --include-ignored --test-threads=1`), `--test authz_matrix -- --include-ignored --test-threads=1` (no ceiling grows; checks the grantee 403 and `can_edit` assertions added here).
5. Full web vitest: `cd apps/web && npx pnpm@10.12.1 exec vitest run --maxWorkers=2`.
6. Playwright Chromium (8080/5173/8081 are mine): exit-gate, lounge-shell, study-shelves, a11y-signed-in (the storefront switches go on in beforeAll).
7. Inventory section for wave 41 in `docs/inventory/21-feature-phase.md` (tick only what ran), then the gates table below, then drop the five scratch DBs.

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

## Gates run so far (merged tree)

| Gate | Result |
|---|---|
| cargo fmt --check (sr-features, sr-api, sr-server, sr-theme, sr-db, sr-storage, sr-moderation, sr-activities) | ok |
| clippy -D warnings, same crates, --all-targets | ok |
| sr-db secret_columns / migration_versions / shelf_access / shelf_repo | 2 / 1 / 4 / 3 pass (shelf_repo after the trigger fix) |
| OpenAPI + api-client + MSW regenerate, mocks:check | ok |
| pnpm -r typecheck (incl. e2e), -r lint, prettier --check . | ok |
| Vitest src/study, storage, cinema, library, lounge/voice | 54 files, 557 pass |
| i18n-audit / check-css-tokens | 0 / clean |
| build-web, bundle budget, entrance isolation | ok |
| threat-model, check-ops-files, entrance script tests | pass |
| Not run yet | everything in "Resume here" steps 1 to 6 |

## Open (owner)

See the owner checklist, section 4b "The Study (wave 41)": D-401 to D-404, the conflict between D-390 and P37-EMU-2
on the PS1 HLE BIOS, and shelf strikes vs the takedown counter. Also not built in wave 41: the owner's client
uploading its own index (the catalog comes from the server indexer, and `object_hash` is the path hash, not a content
hash), the sandbox CSP allow-list for `byos_read`, a web UI for grants and delisting, and a batch URL mint for covers.
A real-bucket, two-browser-profile Playwright run is not possible locally (no Garage). The server side of that proof
is `authz_matrix::shelf_access_holds_over_http`.
