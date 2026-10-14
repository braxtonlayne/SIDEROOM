# feat/p9-test-tools handoff

Items: XC-TEST-002, XC-TEST-003, XC-TEST-004, XC-PERF-004 (all ticked in docs/inventory/18-cross-cutting.md).

## What landed

- **XC-TEST-002** `crates/sr-gateway/src/test_support.rs`, behind `cfg(test)` or feature `test-support`. `TestWorld::new(cfg)` (gateway + in-process bus + in-memory directory), `add_member`, `hide_channel`, `emit`, `connect_users(&[ids], intents)`; `TestClient` with `send`, `recv`, `expect_event`, `await_dispatch(name)`, `try_dispatch(name, within)`, `assert_silent`, `drain`, `recv_close`. Other crates: `sr-gateway = { path = "../sr-gateway", features = ["test-support"] }` in dev-dependencies. The older private `Client` in `tests.rs` was left alone (not migrated).
- **XC-TEST-003** `packages/api-client/scripts/gen-msw.mjs` writes `src/mocks/handlers.generated.ts` (537 operations); `@sideroom/api-client/mocks` exports `createHandlers({ base, overrides, only })`. `pnpm --filter @sideroom/api-client mocks:generate` / `mocks:check`; `just gen-api` regenerates; `.github/workflows/openapi.yml` runs the check. `msw` added as an api-client devDependency (lockfile updated offline). Bodies are placeholders (required properties only); apps/web keeps its hand-written handlers.
- **XC-TEST-004** `e2e/support/fixtures.ts` (`test` with `owner`, `seedUser`, `signedIn`, `scenario.ownerAndMember`) and `e2e/tests/fixtures-scenario.spec.ts`. Users register over the API using `sr-cli invite create`, and sign in over the API so the cookie jar is set without the login form.
- **XC-PERF-004** `crates/sr-db/tests/explain_hot_paths.rs`, `just explain-hot-paths`. No large-table seq scans, so no migration (0570-0579 unused). Findings in docs/04-data-model.md section 13.1.

## Verification

- `cargo fmt`, `clippy -D warnings` (sr-gateway with `test-support`, sr-db tests): clean. `cargo test -p sr-gateway test_support`: 2 passed. `explain_hot_paths` against local Postgres 17 (`sr_test` server, scratch DB): passed in about 70 s.
- api-client: eslint, tsc, vitest (6 tests) clean; `mocks:check` ok. e2e: tsc and prettier clean, `playwright test --list` shows the new specs.
- **Not run:** the new Playwright specs against a live stack (shared ports 8080/5173), full workspace cargo test, workflow in CI.

## Notes for the next agent

- The e2e `register` call posts `/sr/register` with an explicit `Origin` header; if CSRF/Origin rules change, adjust `headers()` in fixtures.ts.
- `signedIn` only supports accounts without 2FA.
- If the seeded partition window moves past the data ids, `explain_hot_paths` uses `sr_snowflake_floor(now())`, so it stays valid.
