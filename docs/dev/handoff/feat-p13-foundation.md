# feat/p13-foundation handoff

Branch `feat/p13-foundation` (base `integration/phase13`). Migration used: `0880_login_alerts.sql`.

## Done

- **G-19 route matrix AUTHZ pass.** All 203 `other` rows verified against their handlers. New vocabulary in `route_matrix.rs`: `user`, `author`, `author_or_staff`, `visibility`, `bot`, `party_access`, `party_member`, `party_control`, `party_manager` (`other` is rejected). New test `authz_agrees_with_handler_extractors` reads each handler signature (staff wrappers found from the sources) and fails when a staff extractor sits on a non-staff row, a `/admin` staff row has none, or a `bot` row lacks `BotOnly`. PR template has the "route matrix rows verified" box.
- **The `signing.test.ts` failure.** Root cause: the repo has two vitest majors (apps/web wants 3, the packages 2.1). Under vitest 2.1.x `.rejects.toThrow('text')` throws `Cannot read properties of undefined (reading 'indexOf')` for any rejection (reproduced with a trivial promise), and `signing.test.ts` holds the only such call in apps/web. Any web run that resolves vitest 2 (stale or hoisted install, wrong binary) fails exactly that test. Fix: the test reads the rejection itself; `src/test/vitestVersion.test.ts` fails with an explanation when the runner has the broken matcher. It is not wasm related (the wasm-dependent tests fail differently: `channelPerms.test.ts` cannot load without `packages/wasm/dist`).
- **Inventory 01 / 02 / 18 audit** (see the ticks): REPO-001/002, CI-002 (n/a by design), CI-004, FEAT-005/006/007, CLI-002/003, DS-TOK-001/005, DS-PRIM-016, DS-SR-006, ENT-AUTH-001/014/015, ENT-INV-003/006/008/009/010, XC-PERF-003, XC-OPS-001/002/003.
- **FND-CLI-003**: `sr-cli backup | restore | upgrade | reindex` (`crates/sr-cli/src/ops.rs`, `sr_db::migration_status`). Round trip verified on a scratch Postgres (upgrade, backup, restore into an empty DB, refusal on non-empty, tamper detection).
- **FND-FEAT-006/007**: feature slot registry replaces the per-feature `if` chain; `UnsavedBar` on both feature screens.
- **DS-PRIM-016**: `Kit.stories.tsx` plus `coverage.test.ts`.
- **ENT-INV-009**, **ENT-AUTH-014/015**: `SR_INVITE_CREDITS_UPDATE`; new-device sign-in mail with a switch (`GET/PUT /sr/auth/login-alerts`, Settings > Devices); security notices when two-factor changes. Device = the UA label ("Firefox on Windows"), so a second machine with the same browser and OS does not alert; the first device never does; unverified addresses get nothing.
- **XC-OPS-001**: `crates/sr-core/tests/deploy_files.rs` (prod.yml server env through the real `Config` loader, env file, services, volumes, ports, Caddy, WAL, systemd). Found and fixed: `/readyz` was unreachable through Caddy and the healthcheck was disabled; `sr-server healthcheck` now exists and is the container health check; the image ships `sr-cli` (compose installs had no way to run migrate/bootstrap). Activities wildcard is the opt-in `caddy/activities.snippet`.
- **XC-OPS-002**: `deploy/install.sh` + `scripts/install.test.mjs` (fake docker on PATH). `garage/bootstrap.sh` is parameterised for prod.
- **XC-OPS-003**: WAL archiving in prod.yml, `wal-archive.sh`, `wal-restore.sh`, `basebackup.sh`, `pitr-restore.sh`, nightly timer, `wal-drill.sh` (run for real against Postgres 17 on this machine: 200 of 300 rows recovered to the chosen moment, new timeline) and a `wal-drill` job in `backup-drill.yml`.
- **XC-PERF-003**: new crate `sr-loadtest` (+ `docs/dev/load-testing.md`, manual `load-test.yml`). Verified against a real `sr-server` build: 300 sessions / 100 msg/s passes (REST p95 18 ms, 100% delivery). Full scale (2,000 / 200 / 300 REST/s) connected every session and delivered every dispatch but sent 172 msg/s and had REST p95 2.9 s on this shared laptop with debug builds. Only a release-build run on quiet hardware (the workflow) can say whether the target holds.

## Still open

- FND-STOR-003: AVIF output and animated WebP (pure-Rust encoders only; owner call whether to add `ravif`).
- FND-DB-009: library samples with CC0 media in `sr-cli seed`.
- ENT-AUTH-011: linking Discord to an existing account, unlinking, PKCE.
- XC-TEST-001 (Garage bucket helper; Garage has no Windows build, CI `storage.yml` covers S3), XC-TEST-005 (needs Playwright against a live stack), XC-CI-005 (optional).
- ENT-UI-010/020/022/025/026 are front-end items outside the backend scope given.

## Tests run

`cargo test`: sr-api `--test route_matrix`, `--lib routes::auth` (92), sr-core `--test deploy_files` (11), sr-server `healthcheck` (4) and `search_jobs` pin, sr-cli (9), sr-loadtest (20), sr-db `--lib`, and Postgres suites `auth_repo`, `account_deletion`, `data_export`, `testdb`, `registration` against my own scratch database (dropped). `cargo fmt --check`, `cargo clippy -D warnings` on sr-core, sr-auth, sr-db, sr-api, sr-cli, sr-server, sr-loadtest; `cargo deny check bans licenses`. `node --test scripts/install.test.mjs`, `scripts/check-ops-files.mjs`, `check-doc-links.mjs`. Web: full apps/web vitest (1779 pass; `channelPerms.test.ts` only fails to load without the wasm build), settings/user suite after the Devices change, typecheck, eslint, prettier; packages/ui stories and typecheck/eslint. Vite build and bundle budget: see the end of this file.

## Decisions / owner questions

- AUTHZ vocabulary above is new; `party_*` values describe the watch-party role the service enforces.
- New-device detection uses the UA label, not a device cookie (fewer touch points, no new cookie); say if a cookie-based id is wanted.
- The server image now contains `sr-cli` (about the size of one more binary).
- Playwright and the shared 8080/5173 ports were not used; the load smoke run used port 18080 on a scratch database.
Vite build and bundle budget: pass (all budgets ok; wasm built).
