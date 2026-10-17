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

`sr-cli backup [--dir ./backups]` writes the same file and manifest as `backup.sh` (`db/sideroom-<stamp>.dump` + `.dump.json`), using `pg_dump` from `PATH` and the database in `SR_DATABASE__URL`; the password is passed in the environment, never as an argument. `sr-cli restore <dump> [--force]` verifies the manifest, refuses a non-empty database unless `--force`, and runs `pg_restore`. `sr-cli upgrade` takes this backup automatically before it migrates. Retention pruning stays with `backup.sh`.

## Retention

`backup.sh` keeps the newest dump of each of the last 14 days, 8 ISO weeks and 12 months (`SR_BACKUP_KEEP=14,8,12`), and deletes the rest with their manifests. Run it more often than daily and only the newest of each day survives.

## Restoring

Stop `sr-server` first, so nothing writes during the restore.

```
# 1. Pick a dump. The manifest is verified automatically.
ls /var/backups/sideroom/db/

# 2. Restore into a new, empty database...
createdb sideroom_restored
deploy/scripts/restore.sh /var/backups/sideroom/db/sideroom-20261013T031500Z.dump \
  postgres://sideroom:<password>@127.0.0.1:5432/sideroom_restored

# 3. ...or over the existing one (drops objects first):
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
