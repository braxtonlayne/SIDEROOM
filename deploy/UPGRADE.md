# Upgrading

Releases are semantic-versioned. Migrations are forward-only and each release must work with the previous release's binary for one version (expand/contract, doc 12 section 6), so a rollback of one version is safe.

`sr-cli upgrade` does the database half in one command: it compares the database with this release's migrations, refuses a database that is newer than the program or has a failed migration, takes a pre-upgrade backup (`sr-cli backup`, needs `pg_dump`; `--no-backup` skips it) and applies what is pending. `sr-cli upgrade --dry-run` only lists it. Steps 2 and 5 below are that command; the rest stay manual.

## Routine upgrade

1. Read the release notes for migrations and settings that need action.
2. **Back up first**: `deploy/scripts/backup.sh`. Confirm it finished and `restore-drill.sh` still passes.
3. Pull the new image (Compose: change `SR_IMAGE` in `.env.prod`, `docker compose pull`) or install the new binary.
4. Unpack the new web build into `SR_WEB_DIR`. Replace it atomically (unpack beside it, then swap the directory) so no visitor sees a half-written build. Open tabs keep working: the service worker keeps serving the old build until the visitor accepts the "new version" toast.
5. Apply migrations **before** starting the new server: `sr-cli upgrade` (with Compose: `docker compose run --rm --no-deps --entrypoint /usr/local/bin/sr-cli sr-server upgrade --no-backup`, since step 2 already backed up; the image carries `sr-cli`).
6. Restart: `docker compose up -d sr-server` or `systemctl restart sr-server`. The server drains for up to 25 s, then jobs for another 25 s, so allow a minute.
7. Check readiness, the Grafana overview, and the error ratio for ten minutes. `/readyz` is not served through Caddy (it lists dependency errors), so ask the server itself: `docker compose exec sr-server /usr/local/bin/sr-server healthcheck` (exit 0 = ready; `docker compose ps` shows the same probe as the container health), or `curl -fsS http://127.0.0.1:8080/readyz` on a plain-binary install.

## Rolling back

Roll back the binary and web build to the previous release. Do **not** restore the database unless the migration itself failed: the previous binary tolerates the new schema by the one-version rule. If a migration failed midway, restore the pre-upgrade dump (BACKUP.md) and report the migration.

## When the version skips

Upgrading across several releases: step through each release in order, running `sr-cli migrate` at each, unless the release notes say a jump is safe.
