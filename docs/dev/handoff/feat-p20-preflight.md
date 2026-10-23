# feat/p20-preflight: RDY-OPS-004, 005, 006

## Resume here
Branch `feat/p20-preflight` is complete locally. The integrator must (1) add `sr-cli preflight` as the last step of `deploy/install.sh` (print the table; use `--insecure-tls` for the smoke stack's Caddy internal CA) and run it in `prod-smoke.yml`, (2) regenerate OpenAPI/client/mocks/route matrix on the merged tree, (3) run it once against the e2e stack with coturn for the TURN allocation.

## Done
- `crates/sr-preflight` (new): checks over probe traits, real probes, `Report` table. Rows: database, migrations, clock skew, object storage (put/get/presign/multipart create+abort), mail relay (connect, no send), public URL (HSTS, CSP, frame-ancestors), TURN (credentials derive + UDP allocation via a small RFC 5766 client, MD5 implemented locally, no new dependency; no TURN configured is a warn), Web Push key, disk, secret file modes, Esplora (when `SR_PAYMENTS_ESPLORA_URL`), voice node (when on).
- `sr-cli preflight [--json] [--send-to ADDR] [--insecure-tls] [--alert-test]`: table, exit 1 on any fail. An unreachable database is a failed row, not an abort.
- `--alert-test`: `sr_server::staff_feed::alert_test` builds a minimal `StaffFeed` and runs the real alert path (stored alert, `#alerts` card naming host, instance and UTC time, Owner mail when the setting is on). New `StaffAlertKind::AlertTest`. `on_alert` now delegates to `deliver_alert`, which returns what happened.
- `POST /admin/system/preflight` (`OPERATOR`, access-log action `system.preflight` with counts and failing ids), `LivePreflight` in sr-server, route matrix row, OpenAPI/client/mocks regenerated, KNOWN_CLASHES still empty.
- Admin: `PreflightCard` on Admin > System and as the Setup checklist's final step; fragment `p20-preflight-1.ts`.
- Docs: RUNBOOK.md section, docs/dev/server.md section, inventory ticks (004 ticked for its local proof; the prod-smoke.yml and install.sh parts open).

## Tests run
- `cargo test -p sr-preflight` (20 unit tests) and `--test pg -- --include-ignored --test-threads=1` on scratch DB `sr_p20_preflight` (both env vars): pass.
- `cargo test -p sr-server --lib staff_feed -- --include-ignored --test-threads=1`: 14 pass incl. the alert-test Postgres test.
- `cargo test -p sr-cli`, `-p sr-api --lib routes::admin` (28), `--test route_matrix --test openapi_names`: pass.
- fmt --all --check, clippy -D warnings on sr-preflight, sr-cli, sr-server, sr-events, sr-api: clean.
- Web: typecheck, lint, prettier, build, bundle budget pass. Full vitest once: 2492 pass, 1 failed (`p11ShellDirectory` under load); passes alone.
- Manual run, `sr-cli preflight` against the scratch DB (memory storage, unreachable public URL): 7 passed, 4 warnings, 1 failed, exit 1 as expected.

## Not done
- Browser check of the card on 8080/5173: skipped (Vitest covers the card; no server was started).
- Real S3, a real coturn and a real SMTP relay were not available locally; those rows are proven with fakes (TURN against a fake UDP coturn).

## Decisions / owner questions
- The roadmap names `MANAGE_PLATFORM`; no such permission exists. The System page uses `OPERATOR`, so the route does too.
- The alert-test card is stored but not pushed live to connected clients (separate process, no gateway); documented in RUNBOOK.
- Row titles and details come from the server in English (not localized).
- `SR_PREFLIGHT_INSECURE_TLS=1` lets the server-side card accept a private CA.
