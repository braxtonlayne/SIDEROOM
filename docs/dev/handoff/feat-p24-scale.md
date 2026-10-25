# Handoff: feat/p24-scale (wave 24, agent "scale")

Branch `feat/p24-scale` (worktree `SIDEROOM_wt/p24-scale`, base `integration/phase24`). Items: RDY-SCALE-002, RDY-OBS-001, RDY-PERF-001 to 004. Migrations used: **1300** only (`pins_message_idx`); 1301 to 1309 stay unused.

## Resume here

Nothing is half-done. The one open piece is the owner-dispatched run of `.github/workflows/load-test.yml`: after it, paste the numbers and the run link into `docs/dev/load-testing.md` ("Results"), fix or document each miss, and tick RDY-PERF-002 in `docs/inventory/20-launch-readiness.md`.

## Done (ticked, proof ran)

| Item | What | Proof |
|---|---|---|
| RDY-SCALE-002 | New route `GET /users/@me/guild-settings` (all stored scopes with channel overrides in one answer; `NotificationRepository::list_scopes` in memory, Postgres and `DynRepo`). Web: `notifSettings.loadAll`, `useLoadNotifSettings` now sends ONE request, defaults for servers the batch does not list, no per-server fallback when the batch fails, a server joined later is fetched alone. | Vitest `p24SettingsBatch.test.tsx` (100 servers: 1 request; 60 servers under a 50-request limit: no 429; failure does not stampede); API test `guild_settings_list_returns_all_scopes_in_one_request`; route matrix row, OpenAPI, client and MSW mocks regenerated; `route_matrix` and `openapi_names` tests pass (KNOWN_CLASHES empty). |
| RDY-OBS-001 | The eleven series in `observability.rs` with doc comments; `admin.health.check` sets job, backup, restore, WAL (absent while `archive_mode` is off), disk and `attachments` storage gauges; `payments.watch` sets the invoice gauges; the CSP handler counts per directive; eight rules in `alerts.yml` with runbook anchors; `sr-voice` scrape job; `check-ops-files.mjs` also reads `crates/sr-voice/src/metrics.rs`; RUNBOOK "Raised by" lines, OPERATOR monitoring paragraph, one Grafana panel per gauge. | Postgres test `one_health_run_sets_the_job_backup_disk_and_storage_gauges` (sr-server), `the_playbook_series_are_exported_under_their_names` and the CSP test in sr-api, `check-ops-files.mjs` ok, `check-ops-files.test.mjs` 11/11. |
| RDY-PERF-001 | `sr-cli seed --scale` (`crates/sr-cli/src/seed_scale.rs`), COPY, one transaction, idempotent per `--seed-name`, refuses non-local without `--force`, count overrides. | Postgres test at 40 accounts / 5 guilds / 300 messages; a 3,000 account / 300k message load in 30 s (debug). |
| RDY-PERF-003 | Named jobs changed, one per commit: `messages.purge_deleted` (bounded tx + SKIP LOCKED + migration 1300), `audit.purge` (5,000-row batches), `search.reindex` (commit every 50k docs, one attachment lookup per page: 380 s to 30 s for 200,024 messages, debug). Suspicion housekeeping, `insights.daily`, trending and backup were already bounded or are not jobs; timed or explained in `load-testing.md`. Helper `sr_db::bounded`. | `jobs_scale_pg_tests` (locked row skipped, taken next run), `bounded` tests, `audit_purge_works_through_more_than_one_batch`, existing purge/takedown/search tests green; EXPLAIN ANALYZE numbers in the doc. |
| RDY-PERF-004 | `crates/sr-gateway/src/memory_budget_tests.rs` with the test-only counting allocator crate `crates/sr-memcount` (the workspace forbids `unsafe`, so the allocator lives in its own crate without the workspace lint; it is a dev-dependency of sr-gateway only). 7.4 KB per session, 14.3 KB per guild subscription, READY 3.5 KB and GUILD_CREATE 113 B at 100 servers, roster-independent. Budgets 1.5x. | The two budget tests. |

## Not done

- **RDY-PERF-002 stays unticked: "written, not yet run".** `load-test.yml` now seeds the dataset, runs 2,000 sessions / 200 msg/s / 300 REST/s, adds a fan-out p95 < 250 ms target and a first-paint probe (a 60-server member opens the Lounge mid-run: READY plus the three shell requests, first paint < 1.5 s, zero 429s; `sr-loadtest` options `--first-paint-guilds`, `--first-paint-ms`, `--fanout-p95-ms`), and records `VmRSS`. Numbers need the dispatch.
- The 5 million message figures in `load-testing.md` are extrapolated from the 300k local seed; the 30-minute seed target at full size, resident memory per session on the real server and the live "no 429 at 60 servers" check come from that run.
- `promtool check rules` in `prod-smoke.yml` (the OBS-001 acceptance): added, approved by owner. The integrator added a standalone `rules` job (Prometheus 3.5.0 downloaded on the CI runner only, sha256 checked against the release's `sha256sums.txt`; a TODO pins the hash once a human has read it). It runs for the first time in CI.
- No Playwright check at 60 servers (no browsers on 8080/5173 for this agent); the load-test probe covers it at the API level.

## Tests run

fmt check; clippy `-D warnings` on sr-api, sr-chat, sr-db, sr-search, sr-gateway, sr-memcount, sr-loadtest, sr-server, sr-cli (all targets); `sr-api` lib (669 pass) plus `route_matrix` and `openapi_names`; sr-chat, sr-search, sr-gateway, sr-loadtest unit tests; sr-server lib with `--include-ignored` (130 pass, Postgres); sr-db and sr-cli with `--include-ignored --test-threads=1` (Postgres, scratch DB `sr_p24_scale`); `node --test scripts/check-ops-files.test.mjs`, `node scripts/check-ops-files.mjs`; web: typecheck, eslint, prettier on the touched files, vitest (see the final report for the full run).

## Decisions

- Batch route name `/users/@me/guild-settings` (static path, no clash with `/users/@me/guilds/{id}`); it lists only scopes the person has changed, the client treats a missing server as defaults.
- Backup/restore gauges are 0 when there is no backup or no drill marker, so `BackupMissingOrStale` and `RestoreDrillFailed` fire for "never" as well as "stale". The WAL series are left absent while `archive_mode` is off (as the roadmap says).
- `sr_storage_bytes{bucket="attachments"}` is the sum of `attachments.size_bytes` under a 3 s statement timeout (the System probe has no object-store figure); `sr_disk_free_ratio` covers the data folder only.
- The health job still runs every 15 minutes, so the queue gauges are as fresh as that; if the job runner itself is stuck the gauges freeze (the `up` and error-rate rules cover that case).
- `unsafe_code = "forbid"` is workspace-wide, so the counting allocator needed its own crate; flagging it as the one lint exception.
- Only `pins(message_id)` was added as an index: it is the only missing index on a job path that EXPLAIN showed hurting. 145 other unindexed foreign keys were found and left alone (list reproducible with the query in the load-testing doc's description).

## Open questions for the owner

1. Dispatch `load-test.yml` (it runs about an hour); is a standard runner enough for the 20,000-account seed plus 2,000 sessions, or should the first run use fewer sessions?
2. Should `sr_storage_bytes` also cover the object store buckets (needs a listing API call per run), or is attachments-in-the-database enough?
3. `promtool check rules` in `prod-smoke.yml`: added, approved by owner (the integrator's `rules` job).

## Note on commits

The `sr-loadtest` changes (fan-out target, first-paint probe, READY frame size) landed in the RDY-PERF-004 commit by accident; the subject names the budget test only.
