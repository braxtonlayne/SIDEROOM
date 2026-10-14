# Upgrading

Releases are semantic-versioned. Migrations are forward-only and each release must work with the previous release's binary for one version (expand/contract, doc 12 section 6), so a rollback of one version is safe.

`sr-cli upgrade` is planned (FND-CLI-003) but does not exist yet; the steps below are what it will automate.

## Routine upgrade

1. Read the release notes for migrations and settings that need action.
2. **Back up first**: `deploy/scripts/backup.sh`. Confirm it finished and `restore-drill.sh` still passes.
3. Pull the new image (Compose: change `SR_IMAGE` in `.env.prod`, `docker compose pull`) or install the new binary.
4. Unpack the new web build into `SR_WEB_DIR`. Replace it atomically (unpack beside it, then swap the directory) so no visitor sees a half-written build. Open tabs keep working: the service worker keeps serving the old build until the visitor accepts the "new version" toast.
5. Apply migrations **before** starting the new server: `sr-cli migrate`.
6. Restart: `docker compose up -d sr-server` or `systemctl restart sr-server`. The server drains for up to 25 s, then jobs for another 25 s, so allow a minute.
7. Check `curl -fsS https://<host>/readyz`, the Grafana overview, and the error ratio for ten minutes.

## Rolling back

Roll back the binary and web build to the previous release. Do **not** restore the database unless the migration itself failed: the previous binary tolerates the new schema by the one-version rule. If a migration failed midway, restore the pre-upgrade dump (BACKUP.md) and report the migration.

## When the version skips

Upgrading across several releases: step through each release in order, running `sr-cli migrate` at each, unless the release notes say a jump is safe.
