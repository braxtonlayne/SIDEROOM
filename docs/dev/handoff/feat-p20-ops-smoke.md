# Handoff: feat/p20-ops-smoke (RDY-OPS-001, RDY-OPS-002, RDY-OPS-003)

Branch `feat/p20-ops-smoke`, base `integration/phase20`. No migrations used (1180 to 1189 unused). Nothing pushed.

## Status: written, not yet run

**None of the three items is ticked** in `docs/inventory/20-launch-readiness.md`. The proof is the first green dispatch of
`.github/workflows/prod-smoke.yml`, which needs the owner's push (Actions > Production smoke > Run workflow, branch
`feat/p20-ops-smoke` or the merged tree). This machine has no Docker, no systemd and no Linux, so the Docker, Caddy, systemd
and Playwright parts have never executed. Expect the first run to need a few fix-ups; the scripts print enough to find them.

## What was written

| File | Purpose |
|---|---|
| `.github/workflows/prod-smoke.yml` | `workflow_dispatch` (input `systemd_user_fallback`) plus weekly (Tue 05:41 UTC). Jobs: `image` (server image as a tarball, same build and `type=gha` cache as `release-dry-run.yml`), `web` (web bundle as `release-dry-run.yml` builds it), `stack` (needs both) and `systemd` (needs `image`, with a `postgres:17` service). |
| `deploy/scripts/prod-smoke.sh` | The `stack` stages: `install`, `wait`, `migrate`, `signin`, `backups`, `cli-backups`, `logs`. |
| `deploy/scripts/smoke-admin.mjs` | Owner sign-in through Caddy (public API, CSRF, Origin) and Admin > System; `backup-time` sets `ops.backup_dir` and checks the reported backup time. |
| `deploy/scripts/smoke-systemd.sh` | `verify` (`systemd-analyze verify`) and `user-service` (static-binary install under a prefix, user service). |
| `deploy/BACKUP.md` | New "On the Compose stack" section: how Admin > System sees backups, and the script path vs `sr-cli` path table (RDY-OPS-002 acceptance). |
| `crates/sr-core/tests/deploy_files.rs` | Two new guards: the server sees the backup folder read-only; the workflow only calls stages and files that exist (and the unit shape `prod-smoke.sh` parses). |
| `scripts/install.test.mjs` | Test for `SR_INSTALL_SKIP_PULL`. |

### What the `stack` job does (RDY-OPS-001/002)

1. Loads the image, unpacks the web bundle, takes the static musl `sr-cli` out of the image (`docker cp`), installs the PostgreSQL 17 client (as `backup-drill.yml`), installs the e2e workspace package.
2. `install`: adds `smoke.localhost` to `/etc/hosts`, runs `deploy/install.sh --yes` against `prod.yml` with generated secrets (installer already writes `.env.prod` from the example, migrates, sets CORS and bootstraps the Owner).
   Domain is `smoke.localhost`, not `localhost`: `install.sh` requires a dot in `SR_DOMAIN`, and Caddy issues `.localhost` names from its internal CA.
3. `wait`: waits for `sr-server` healthy, copies Caddy's root CA out of the `caddy-data` volume, `/healthz` through Caddy with that CA, HSTS, CSP with `frame-ancestors`, `/internal/metrics` is 404, SPA shell served, container `healthcheck` (readyz: database and storage).
4. `migrate`: second `sr-cli migrate` (no-op), `verify-access-log`, a second `bootstrap` (must refuse or be idempotent).
5. `signin`: Owner login through Caddy, `/sr/csrf`, `/admin/system`, readiness probes ok. Then Playwright `entrance-smoke` (Chromium only) through Caddy, with a config generated at run time (`e2e/playwright.prod-smoke.config.ts`, not committed: `e2e/` is outside this item's file list). Note the existing `entrance-smoke` spec does not itself sign in; the sign-in proof is `smoke-admin.mjs`.
6. `backups`: `backup.sh` in the postgres container; the base backup with the exact command read out of `sideroom-basebackup.service`; `restore.sh` into a second database and comparison of table, migration and account counts; a second restore refused; `restore-drill.sh`; `wal-drill.sh` (scratch cluster inside the postgres image); a live `pg_switch_wal()` and a new segment in the real WAL archive; then Admin > System: `ops.backup_dir` set through `PATCH /admin/settings`, `backups.last_run_ms` within 45 minutes and `last_verify_ms` present.
7. `cli-backups` (on the runner, `sr-cli` and `pg_dump` 17 reaching Postgres by its container address): `upgrade --dry-run`, `backup`, `restore` into an empty database, a refused second `restore`, `restore --force`, count comparison, `upgrade --dry-run` on the copy, and a tampered dump refused.

### What the `systemd` job does (RDY-OPS-003)

- `verify`: creates a `sideroom` system user, `/opt/sideroom/bin/sr-server` and an `/opt/sideroom/deploy` link with sudo (the units name them and `verify` checks that they exist; refused unless `CI=true` or `SMOKE_ALLOW_SUDO=1`), then `systemd-analyze verify deploy/systemd/*.service *.timer`. Any output counts as a failure.
- `user-service`: copies the static `sr-server`/`sr-cli` under `$SMOKE_PREFIX/opt/sideroom/bin`; writes `config.toml` by extracting the first `toml` block of `deploy/INSTALL.md` section 4 (so the doc and the script cannot drift; only the two listener ports change); writes `server.env` (mode 0600) with the secrets the unit's `EnvironmentFile` carries; runs `sr-cli migrate` and `bootstrap` from it; derives a user unit from `deploy/systemd/sr-server.service` (prints the diff) and starts it with `systemctl --user`, starting the user manager via `loginctl enable-linger` and `user@UID.service` when the runner has none; waits for `/healthz`; stops it by SIGTERM and prints the unit `Result`.
  The user unit drops the lines that need root (`User=`, `Group=`, `StateDirectory=`, `ReadWritePaths=`, `ProtectSystem/Home`, `PrivateTmp/Devices`, `ProtectKernel*`, `ProtectControlGroups`, `RestrictNamespaces`, `LimitNOFILE`, `After=`, `Wants=`) and keeps `NoNewPrivileges`, `RestrictAddressFamilies`, `LockPersonality`, `MemoryDenyWriteExecute` and `SystemCallArchitectures`, so the hardening that matters for the binary is exercised.
  If no user manager can be started the job fails, unless `systemd_user_fallback` is set: then it runs the unit's `ExecStart` directly, prints `::warning::` and the log says "NOT a systemd service". A fallback run does not prove the unit.

## Changes to `prod.yml`, `install.sh`, the Caddyfile and the Dockerfiles, with reasons

None of these has been run against Docker. Each was made because the smoke needs it; the smoke may force more.

| File | Change | Reason |
|---|---|---|
| `deploy/install.sh` | New env var `SR_INSTALL_SKIP_PULL=1` skips `docker compose pull`; header and `--help` text updated. Default unchanged. | The smoke uses a locally loaded image (`sideroom-server:smoke`); `compose pull` fails for an image that is not in a registry. Compose still pulls postgres, garage and caddy when `up` starts them. Tested in `scripts/install.test.mjs`. |
| `deploy/compose/prod.yml` | `sr-server` gets `pg-backups:/var/backups/sideroom:ro` and `group_add: ["999"]`. | RDY-OPS-002 acceptance: Admin > System shows the last backup only if the server can read the backup folder (`ops.backup_dir`). Before this the folder existed only in the postgres container, so the tile could never be filled on a Compose install. 999 is the `postgres` group of `postgres:17`. |
| `deploy/compose/prod.yml` | `wal-archive-init` now sets the backup folder to 750 (was 700); the WAL archive stays 700. | Group 999 needs to list the folder. The volume lives under Docker's root-only directory, so host exposure does not change. |
| `deploy/scripts/backup.sh`, `deploy/scripts/basebackup.sh` | New `SR_BACKUP_DIR_MODE` (default 700, unchanged) for the backup folder; `basebackup.sh` keeps `base/` at 700. | Both scripts used to `chmod 700` the folder on every run, undoing the 750 above. |
| `deploy/systemd/sideroom-basebackup.service` | The in-container command gets `SR_BACKUP_DIR_MODE=750`. | Same reason; this is the Compose-side unit. `deploy_files.rs` still passes. |
| `deploy/compose/caddy/Caddyfile.prod` | **No change.** | `{$SR_DOMAIN}` = `smoke.localhost` takes Caddy's internal CA by itself. |
| `deploy/docker/*.Dockerfile` | **No change.** | The smoke builds `server.Dockerfile` as is. |

## Validated here

- `node --test scripts/install.test.mjs`: 10 pass (including the new skip-pull test). `node --test scripts/ci-scripts.test.mjs`: 6 pass.
- `cargo test -j 2 -p sr-core --test deploy_files`: 15 pass (13 existing, 2 new). `cargo fmt -p sr-core --check` and `cargo clippy -j 2 -p sr-core --tests -D warnings`: clean.
- `node scripts/check-ops-files.mjs` (runs `sh -n` over every `deploy/scripts/*.sh` and `install.sh`) and `node scripts/check-doc-links.mjs`: ok.
- YAML: `prod-smoke.yml` and `prod.yml` parse (js-yaml); the run blocks and the generated Playwright config were read back from the parsed workflow. `node --check` on `smoke-admin.mjs`.
- shellcheck and actionlint are not installed here and were not run. The scripts are POSIX `sh` (`sh -n` clean).
- Dry walks with fakes (Git Bash): `prod-smoke.sh install` against a fake `docker` and a copy of `deploy/` (env file filled, Owner password file, image and web dir passed through, pull skipped); `smoke-systemd.sh user-service` against a fake `sr-server` (Node HTTP listener), fake `sr-cli`, and once with a fake `systemctl` and once on the fallback path: config extracted from INSTALL.md with moved ports, `server.env` written, user unit diff printed, `/healthz` answered, stop and result printed. The `wait`, `backups`, `cli-backups`, `verify`, `smoke-admin.mjs` and Playwright steps could not be walked without Docker, systemd and a server.

## Needs the first CI run (what is most likely to need a fix-up)

1. **Image build time.** A cold `cargo zigbuild --release` can exceed the 20 minute budget; the `gha` cache shared with `release-dry-run.yml` is what keeps it down. If it is still too slow, the integrator may want a warmed cache or a prebuilt image from `release.yml`. `stack` itself should take a few minutes.
2. **`systemctl --user` on a runner.** If `loginctl enable-linger` plus `user@UID.service` does not give a user bus, rerun with `systemd_user_fallback` for a weaker check, or move the stage to a self-hosted/VM runner.
3. **`systemd-analyze verify` output.** Any message counts as failure; a known harmless one (for example about `postgresql.service` not being installed) should be fixed in the unit or allow-listed in `smoke-systemd.sh`.
4. **Whether `sr-server`, `sr-cli` and `sr-cli backup` start with only the environment the scripts set** (SMTP and storage endpoints are placeholders; `/healthz` is liveness so the systemd job does not need Garage).
5. Chromium resolving `smoke.localhost` (it maps `*.localhost` to loopback; `/etc/hosts` is also written) and trusting nothing (`ignoreHTTPSErrors`).
6. `docker compose exec -u postgres` for `wal-drill.sh` (needs `initdb` on `PATH` inside the image; the official image provides it).
7. `PATCH /admin/settings` for `ops.backup_dir` without a recent second-factor check (it is not a `require_reauth` route in the code I read).

## Not done, and decisions

- Ticks: none, per the roadmap rule ("written but never run does not tick"). After the first green dispatch tick RDY-OPS-001/002/003, and note RDY-DOC-003 depends on RDY-OPS-001.
- `sr-cli preflight` is **not** called from `install.sh` (another agent owns the command; the integrator adds the call, and RDY-OPS-004 wants it in `prod-smoke.yml` too).
- `deploy/INSTALL.md` is outside this item's file list and was not edited. Its section 5 has no explicit systemd subsection (it mentions `deploy/systemd/` in one sentence, and `server.env`/`SR_CONFIG` appear only in the unit's comments). `smoke-systemd.sh` documents the path in code: the integrator should add a "plain-binary route" subsection to INSTALL.md that matches the script (prefix layout, `config.toml` from section 4, `server.env` with `SR_CONFIG` and the five secrets, `sr-cli migrate`/`bootstrap`, `systemctl enable --now sr-server`).
- Admin > System shows no backup tile until `ops.backup_dir` is set; the installer does not set it (it is a database setting). The smoke sets it through the API; BACKUP.md documents it. Setting it from the Setup Wizard or `sr-cli` would be a follow-up.
- `entrance-smoke` does not sign in; the sign-in proof is `smoke-admin.mjs`. A UI sign-in spec would live in `e2e/`, outside this file list.
- Backup scripts run in the postgres container (the packaged path) and `sr-cli backup`/`restore` run on the runner against the container's IP, because the server image is distroless and has no `pg_dump`. Documented in BACKUP.md.
- Open owner questions: (a) approve a push so the workflow can be dispatched; (b) is a weekly 05:41 UTC Tuesday schedule fine, or should it also run on pull requests touching `deploy/**`?
