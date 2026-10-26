# Backups and restores

Doc 12 section 5 sets the policy. This page is the how-to. Everything here is plain shell, so it runs from cron, systemd or CI.

## What is backed up

| What | How | Where it lives |
|---|---|---|
| Postgres | `pg_dump` custom format, one file per run, with a `.json` manifest (SHA-256, size, migration count) | `deploy/scripts/backup.sh` |
| Object storage (attachments, avatars, library pages) | `rustic` or `restic` snapshot of the Garage data directory | same script, when `SR_BACKUP_REPOSITORY` and `SR_BACKUP_OBJECTS_DIR` are set |
| HLS renditions | Not backed up. They are rebuilt from originals by re-transcoding | n/a |
| `server.secret_key` and `config.toml` | **You** copy these, encrypted, next to the dumps. Without the key the TOTP secrets in the dump are unreadable | n/a |

The nightly dump alone gives a recovery point of up to 24 hours. With WAL archiving (below) the recovery point shrinks to about five minutes (`archive_timeout=300`), and you can recover to any moment in between.

## Point-in-time recovery (WAL archiving)

`deploy/compose/prod.yml` starts Postgres with `archive_mode=on`. Every finished WAL segment is copied into the `wal-archive` volume by `deploy/scripts/wal-archive.sh` (atomic, idempotent, never overwrites a different file). A nightly base backup (`deploy/scripts/basebackup.sh`, via `sideroom-basebackup.timer`) is the starting point; recovery replays the archived segments on top of it.

| Piece | What it does |
|---|---|
| `wal-archive.sh` | `archive_command`: publishes one segment into the archive |
| `basebackup.sh` | `pg_basebackup` in tar format with the WAL streamed, a manifest (checksum, first needed segment), keeps the newest `SR_BASEBACKUP_KEEP` (7) and trims archived segments older than the oldest kept one with `pg_archivecleanup` |
| `pitr-restore.sh` | unpacks a base backup into an empty data directory and writes the recovery settings (`restore_command`, `recovery_target_time`, promote at the target) |
| `wal-drill.sh` | proves all of it on a scratch cluster: 300 rows in three batches, recover to between batch two and three, expect exactly 200 rows on a new timeline |

Enable the nightly base backup on the host: copy `deploy/systemd/sideroom-basebackup.*` to `/etc/systemd/system/` and `systemctl enable --now sideroom-basebackup.timer`. The unit runs the script inside the postgres container over the local socket, so no password is involved.

To recover (stop `sr-server` and the old Postgres first):

```
# list base backups; pick the newest one that is older than the moment you want
docker compose -f deploy/compose/prod.yml run --rm --no-deps --user postgres -v sideroom_recovered:/recovered postgres \
  sh /opt/sideroom/scripts/pitr-restore.sh /var/backups/sideroom/base/base-<stamp> /recovered/data \
  /var/lib/postgresql/wal-archive "2026-10-14 09:30:00+00"
```

then start a Postgres on that data directory (`pg_ctl -D <dir> start`, or point a fresh container at the volume), check the data, and switch `sr-server` over. Omit the time to replay the whole archive. The recovered server promotes itself at the target and continues on a new timeline. The base backups and the archive live on the same host as the database: copy both off the machine (rustic or restic over the `pg-backups` and `wal-archive` volumes) or a dead disk takes the backups with it.

`wal-drill.sh` runs weekly in `.github/workflows/backup-drill.yml` (job `wal-drill`) and by hand with the Postgres 17 binaries on `PATH`: `deploy/scripts/wal-drill.sh`.

## One-off setup

1. Create `/etc/sideroom/backup.env` (mode 0600, owner `sideroom`):

   ```
   SR_DATABASE_URL=postgres://sideroom:<password>@127.0.0.1:5432/sideroom
   SR_BACKUP_DIR=/var/backups/sideroom
   SR_BACKUP_KEEP=14,8,12
   # Optional object snapshots to a Storage Box:
   # SR_BACKUP_OBJECTS_DIR=/var/lib/garage/data
   # SR_BACKUP_REPOSITORY=sftp:u123@u123.your-storagebox.de:/sideroom
   # RUSTIC_PASSWORD_FILE=/etc/sideroom/rustic.pass
   ```

2. `install -d -o sideroom -g sideroom -m 700 /var/backups/sideroom`
3. Copy `deploy/systemd/sideroom-backup.*` and `sideroom-restore-drill.*` to `/etc/systemd/system/`, then
   `systemctl enable --now sideroom-backup.timer sideroom-restore-drill.timer`.

With Docker Compose, run the same script from the host against the published or exec'd Postgres: `docker compose exec -T postgres pg_dump ...` also works, but the scripts expect a reachable `SR_DATABASE_URL`. The simplest route is to publish Postgres on `127.0.0.1` for the backup user only.

## From the command line

`sr-cli backup [--dir ./backups]` writes the same file and manifest as `backup.sh` (`db/sideroom-<stamp>.dump` + `.dump.json`), using `pg_dump` from `PATH` and the database in `SR_DATABASE__URL`; the password is passed in the environment, never as an argument. `sr-cli restore <dump> [--force]` verifies the manifest, refuses a non-empty database unless `--force`, and runs `pg_restore`. `sr-cli upgrade` takes this backup automatically before it migrates. Retention pruning stays with `backup.sh`. Like the script, it first removes a `*.dump.partial` older than one hour.

## On the Compose stack

The postgres container of `deploy/compose/prod.yml` has the scripts at `/opt/sideroom/scripts` and the backup folder as the `pg-backups` volume at `/var/backups/sideroom`, so the packaged route needs nothing on the host but Docker:

```
cd /opt/sideroom/deploy/compose
dc="docker compose -f prod.yml --env-file .env.prod"
$dc exec -T -u postgres postgres sh -c 'SR_DATABASE_URL=postgres://sideroom@/sideroom SR_BACKUP_DIR=/var/backups/sideroom SR_BACKUP_DIR_MODE=750 sh /opt/sideroom/scripts/backup.sh'
```

(The URL has no host and no password: the container's own socket is trusted. The same shape is in `deploy/systemd/sideroom-basebackup.service`.) Copy `/var/lib/docker/volumes/<project>_pg-backups/_data` off the host, or `docker cp` from the container.

**Admin > System shows the last backup.** sr-server mounts the `pg-backups` volume read-only at the same path and is in the postgres group (`group_add: ["999"]`), and the folder is mode 750 (`SR_BACKUP_DIR_MODE=750`, which `backup.sh` and `basebackup.sh` honour; the default stays 700). Point the platform setting at it once, in Admin > Limits (group `alerts`) or with `PATCH /admin/settings`:

```
ops.backup_dir = /var/backups/sideroom
```

Then Admin > System reports the newest `*.dump` or manifest time, the `restore-drill.ok` time (run `restore-drill.sh` the same way) and, when objects are snapshotted, `snapshot.ok`. The folder is readable by the server because the server can already read the whole database; do not widen it further.

### Script path and `sr-cli` path

Both write the same dump format and manifest, and both verify the checksum before restoring. They differ in where they run and what else they do:

| | `backup.sh` / `restore.sh` | `sr-cli backup` / `restore` |
|---|---|---|
| Runs | where `pg_dump` / `pg_restore` / `psql` and a shell are: in the postgres container (Compose), or on the host with the client tools | where `sr-cli` runs and `pg_dump` / `pg_restore` are on `PATH`. The server image is distroless, so there is **no** `sr-cli backup` inside it: use the static binary on the host and reach Postgres over its container address, or publish it on loopback |
| Database named by | `SR_DATABASE_URL` (backup) and the target URL argument (restore) | `SR_DATABASE__URL` / `SR_CONFIG`, the same configuration as the server; restore always targets the configured database |
| Folder | `SR_BACKUP_DIR`, mode `SR_BACKUP_DIR_MODE` (default 700) | `--dir`, always 700 |
| Stale `*.dump.partial` (older than one hour) removed first | yes | yes |
| Retention (14 daily, 8 weekly, 12 monthly) | yes | no |
| Object snapshot (`rustic` / `restic`), `snapshot.ok` | yes | no |
| Non-empty target | refused unless `SR_RESTORE_FORCE=1` | refused unless `--force` |
| `--clean --if-exists` before restoring | with `SR_RESTORE_FORCE=1` | with `--force` |
| Tampered dump | refused (`checksum mismatch`) | refused |
| After restoring | prints a reminder to run `sr-cli migrate` | prints the same reminder |
| Also does | nothing else | `sr-cli upgrade` takes this backup before it migrates; `upgrade --dry-run` only lists the pending migrations |

`.github/workflows/prod-smoke.yml` runs both paths against a freshly installed Compose stack (`deploy/scripts/prod-smoke.sh backups` and `cli-backups`): `backup.sh`, the base backup of the systemd unit, `restore.sh` into a second database (and a refused second restore), `restore-drill.sh`, `wal-drill.sh`, a live WAL segment reaching the archive, then `sr-cli upgrade --dry-run`, `backup`, `restore`, `restore --force` and a refused tampered dump. `wal-drill.sh` itself builds a scratch cluster inside the postgres image; it proves the archive scripts, not the live archive, which the last check covers.

## Retention

`backup.sh` keeps the newest dump of each of the last 14 days, 8 ISO weeks and 12 months (`SR_BACKUP_KEEP=14,8,12`), and deletes the rest with their manifests. Run it more often than daily and only the newest of each day survives.

**Stale `.partial` files.** A dump is written to `sideroom-<stamp>.dump.partial` and renamed only when `pg_dump` succeeds, so a failed run leaves no fresh `.dump` and never refreshes the backup age shown in Admin > System. Both `backup.sh` and `sr-cli backup` remove a `*.dump.partial` older than one hour before they start (a younger one may be a dump still running, so it stays), and retention only ever looks at finished `*.dump` files, so a `.partial` is never counted as a backup (RDY-OPS-008).

## Restoring

Stop `sr-server` first, so nothing writes during the restore.

```
# 1. Pick a dump. The manifest is verified automatically.
ls /var/backups/sideroom/db/

# 2. Restore into a new, empty database...
createdb sideroom_restored
deploy/scripts/restore.sh /var/backups/sideroom/db/sideroom-20261013T031500Z.dump \
  postgres://sideroom:<password>@127.0.0.1:5432/sideroom_restored

# 3. ...or over the existing one (drops the objects the dump contains first; tables a newer migration added stay, so
#    for a rollback after an upgrade drop and recreate the database instead, deploy/UPGRADE.md "Rolling back"):
SR_RESTORE_FORCE=1 deploy/scripts/restore.sh <dump> <url-of-the-live-database>

# 4. Apply migrations newer than the dump, then start the server.
sr-cli migrate
systemctl start sr-server
```

`restore.sh` refuses a non-empty database unless `SR_RESTORE_FORCE=1`, and refuses a dump whose checksum does not match its manifest.

Object storage: `rustic -r <repo> restore latest --target /var/lib/garage/data` (with Garage stopped), then start Garage. Orphaned objects are harmless; missing ones show as broken attachments, so restore objects from the same night as the dump.

## The restore drill

A backup nobody has restored is a hope. `deploy/scripts/restore-drill.sh` restores the newest dump into a throwaway database, checks that migrations and tables are present and that none failed, and drops the database. It fails when the newest backup is more than 36 hours old (`SR_DRILL_MAX_AGE_HOURS`). The systemd timer runs it every Sunday; `.github/workflows/backup-drill.yml` runs the same scripts weekly against a migrated, seeded CI database so the scripts themselves cannot rot. Wire the service's failure to your alerting (for example `OnFailure=` with an email unit). A successful drill touches `restore-drill.ok` in the backup folder, and a backup that snapshotted the objects touches `snapshot.ok`; Admin > System shows their times next to the newest dump (`GET /admin/system`, `backups`).

## Secrets rotation

Rotating `server.secret_key`: move the old value to `server.secret_key_previous`, set the new one, run `sr-cli rotate-secret-key`, and take a fresh backup of both keys. Older dumps need the key that was current when they were made.
