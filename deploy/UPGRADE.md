# Upgrading

Releases are semantic-versioned. Migrations are forward-only and each release must work with the previous release's binary for one version (expand/contract, doc 12 section 6), so a rollback of one version is safe.

`sr-cli upgrade` does the database half in one command: it compares the database with this release's migrations, refuses a database that is newer than the program or has a failed migration, takes a pre-upgrade backup (`sr-cli backup`, needs `pg_dump`; `--no-backup` skips it) and applies what is pending. `sr-cli upgrade --dry-run` only lists it. Steps 2 and 6 below are that command; the rest stay manual.

This page is rehearsed on every change to the install surface: the `upgrade` job of `.github/workflows/prod-smoke.yml` (`deploy/scripts/prod-smoke.sh upgrade-*`) installs the previous version on a Compose stack, seeds it, runs the commands below against the pull request's image, then rolls back from the backup and checks the data after each move (RDY-OPS-007). The commands in the blocks below are the ones it runs. The shorthand used throughout:

```
cd /opt/sideroom/deploy/compose
dc="docker compose -f prod.yml --env-file .env.prod"
```

## Routine upgrade

1. Read the release notes for migrations, settings and new `.env.prod` keys that need action.
2. **Back up first.** On a Compose install, run `backup.sh` inside the postgres container (the host has neither `pg_dump` nor a published database port; BACKUP.md "On the Compose stack" explains the variables). Note the dump it prints, you need it if you roll back, then confirm that `restore-drill.sh` still passes:

   ```
   $dc exec -T -u postgres postgres sh -c 'SR_DATABASE_URL=postgres://sideroom@/sideroom SR_BACKUP_DIR=/var/backups/sideroom SR_BACKUP_DIR_MODE=750 sh /opt/sideroom/scripts/backup.sh'
   ```

   On a plain-binary install run `deploy/scripts/backup.sh` with `SR_DATABASE_URL` set instead.
3. Update the deploy files to the release: the new `deploy/` folder (at least `compose/prod.yml`, `compose/caddy/`, `compose/garage/` and `scripts/`, which the postgres container mounts) replaces the old one, and your `compose/.env.prod` and `compose/garage/garage.prod.toml` stay as they are. Compare `compose/.env.prod.example` with your `.env.prod` and add any key the release introduced (`docker compose config` fails with "set X in .env.prod" for a required one). Nothing is regenerated: `install.sh` is for a first install and refuses to touch an existing one.
4. Pull the new image (Compose: change `SR_IMAGE` in `.env.prod`, `$dc pull`) or install the new binary.
5. Unpack the new web build into `SR_WEB_DIR`. The server reads the same directory (read-only, `SR_WEB__ROOT`; see "the web app is served by the server" below), so the swap applies to both containers: recreate Caddy and the server (`$dc up -d --force-recreate caddy sr-server`) after swapping the directory itself. Replace it atomically (unpack beside it, then swap the directory) so no visitor sees a half-written build. On Compose the directory is a bind mount: after swapping the directory itself, recreate Caddy (`$dc up -d --force-recreate caddy`) so it sees the new one; unpacking over the existing files needs nothing. Open tabs keep working: the service worker keeps serving the old build until the visitor accepts the "new version" toast.
6. Apply migrations **before** starting the new server: `sr-cli upgrade` (with Compose, from the new image, since step 4 changed `SR_IMAGE`; step 2 already backed up and the image has no `pg_dump`, hence `--no-backup`; the image carries `sr-cli`):

   ```
   $dc run --rm --no-deps --entrypoint /usr/local/bin/sr-cli sr-server upgrade --no-backup
   ```

   The old server keeps running during this; the previous binary tolerates the new schema by the one-version rule.
7. Restart: `$dc up -d sr-server` or `systemctl restart sr-server`. The server drains for up to 25 s, then jobs for another 25 s, so allow a minute.
8. Check readiness, the Grafana overview, and the error ratio for ten minutes. `/readyz` is not served through Caddy (it lists dependency errors), so ask the server itself: `$dc exec sr-server /usr/local/bin/sr-server healthcheck` (exit 0 = ready; `$dc ps` shows the same probe as the container health), or `curl -fsS http://127.0.0.1:8080/readyz` on a plain-binary install. `https://<domain>/healthz` through Caddy must answer too.

## Rolling back

Two cases, in this order of preference.

**The migration ran and the new release is bad.** Roll back the binary and web build to the previous release, and leave the database alone: the previous binary tolerates the new schema by the one-version rule. With Compose, put the old `SR_IMAGE` back in `.env.prod` (and the old deploy files and web build if the release changed them) and run `$dc up -d sr-server`.

**The migration itself failed, or the previous binary cannot read the new schema** (a release note will say so; migration 0803 below is one). Go back to the pre-upgrade dump from step 2. Restore it into an *empty* database, not over the upgraded one: `SR_RESTORE_FORCE=1` only drops the objects the dump contains, so tables a newer migration added stay behind and the next upgrade then stops on "already exists". With Compose, using the dump path step 2 printed:

```
$dc stop sr-server
$dc exec -T postgres psql -X -U sideroom -d postgres -c 'drop database sideroom with (force)' -c 'create database sideroom'
$dc exec -T -u postgres postgres sh -c "sh /opt/sideroom/scripts/restore.sh /var/backups/sideroom/db/sideroom-<stamp>.dump postgres://sideroom@/sideroom"
# set the previous SR_IMAGE in .env.prod (and the previous deploy files), then
$dc up -d sr-server
```

Everything written after the backup is lost; the restored database is exactly the one the previous release had. Report the failing migration.

## When the version skips

Upgrading across several releases: step through each release in order, running `sr-cli migrate` at each, unless the release notes say a jump is safe.

## Release notes: the plaintext signing seed is gone (migration 0803, G-85)

Migration `0803_drop_plaintext_signing_seed.sql` drops `applications.interactions_signing_seed`. Seeds have lived encrypted in `interactions_signing_seed_sealed` since `0771`, and the old column was emptied by the startup job `apps.seal_seeds` of the release that shipped `0771`. The upgrade order is therefore: first run a release that contains `0771` (any release after it) once, with `server.secret_key` set, then upgrade. If a plaintext seed is left, the migration stops with "run the previous release once to seal seeds" and changes nothing; start the previous release, then retry. The one-version rule does not cover this migration: the release before it still reads the dropped column at startup, so go back to the pre-upgrade dump (the second case of "Rolling back") if you must roll back past it.

## Release notes: the instance bucket is storage mount 1 (migration 1420, BYOS-INS-001)

No operator action. On first start the server records your `[storage]` bucket as mount 1 in `storage_mounts` (the access key's secret is sealed with `server.secret_key`, like every mount secret) and the media tables gain a `mount_id` column that defaults to 1, so existing rows keep pointing at the same bucket and the same keys. The `[storage]` config stays the source of truth for the instance bucket and nothing is copied or moved.

One new refusal: if mount 1 already exists and `storage.bucket` names a different bucket, the server will not start (every existing media row names mount 1, so a silent switch would break them). Restore the old bucket name, or move the objects and then run `sr-cli storage set-instance-mount`. With `storage.endpoint = "memory:"` (test use) no mount is seeded.

## Release notes: the web app is served by the server, behind the session (D-385, SEC-ENT-003)

A visitor without a login now receives only the public files (the landing, sign-in and invite pages, the fonts, the icons, the service worker). The app itself (`/app`, `/app/*` and every in-app path) is served by `sr-server` from the same web build, and only to a request with a valid session; the Compose Caddy no longer serves it. The OpenAPI document (`/api/v1/openapi.json`) and the API reference (`/developers/docs/api`) need a session or a bot token. This is a **one-time hosting change** at the first upgrade that carries it:

1. **Give the server the web build.** Compose: take the new `compose/prod.yml` (step 3); it mounts `SR_WEB_DIR` read-only into `sr-server` as `/srv/web` and sets `SR_WEB__ROOT=/srv/web`. Nothing else to edit: `SR_WEB_DIR` is the variable you already have. Plain-binary install: set `SR_WEB__ROOT=/opt/sideroom/web` (or `[web] root` in `config.toml`) in `/etc/sideroom/server.env`; the unit in `deploy/systemd/sr-server.service` already names that path.
2. **Re-render the Caddyfile.** Replace `compose/caddy/Caddyfile.prod` with the release's file, or, if you have edited it, run `node deploy/scripts/render-caddyfile.mjs --public-url https://<domain>` against your copy (add `--activities` and `--voice-url` as before). The file now has no `try_files` fallback: Caddy answers the listed public paths and proxies everything else to the server. Recreate Caddy: `$dc up -d --force-recreate caddy`.
3. **Order.** Unpack the web build first (step 5), then restart the server (step 7): with `[web] root` set the server **refuses to start** when `app/csp.txt` is missing from the build, so a half-unpacked or old build stops the start instead of serving the app without its policy. `sr-cli preflight` has a row for it ("web root set and app.html present"; a warning when `SR_WEB__ROOT` is unset).
4. **Check.** Signed out, `https://<domain>/channels` answers `302` to `/login?next=/channels` and `https://<domain>/app/anything.js` answers an empty `401`, whether or not the file exists. Signed in, the app opens. `/healthz` is unchanged.

If you serve the web app from some other proxy, reproduce the rule in the Caddyfile: serve only the public paths of `dist/surfaces.json` yourself, and proxy every other path to `sr-server`. Do not add a `try_files` fallback to `index.html`.
