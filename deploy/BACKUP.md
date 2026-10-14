# Backups and restores

Doc 12 section 5 sets the policy. This page is the how-to. Everything here is plain shell, so it runs from cron, systemd or CI.

## What is backed up

| What | How | Where it lives |
|---|---|---|
| Postgres | `pg_dump` custom format, one file per run, with a `.json` manifest (SHA-256, size, migration count) | `deploy/scripts/backup.sh` |
| Object storage (attachments, avatars, library pages) | `rustic` or `restic` snapshot of the Garage data directory | same script, when `SR_BACKUP_REPOSITORY` and `SR_BACKUP_OBJECTS_DIR` are set |
| HLS renditions | Not backed up. They are rebuilt from originals by re-transcoding | n/a |
| `server.secret_key` and `config.toml` | **You** copy these, encrypted, next to the dumps. Without the key the TOTP secrets in the dump are unreadable | n/a |

Write-ahead-log archiving (point-in-time recovery) is not scripted yet. The nightly dump gives a recovery point of up to 24 hours. If you need less, add `archive_command` to Postgres and keep the dumps as the base backup.

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

A backup nobody has restored is a hope. `deploy/scripts/restore-drill.sh` restores the newest dump into a throwaway database, checks that migrations and tables are present and that none failed, and drops the database. It fails when the newest backup is more than 36 hours old (`SR_DRILL_MAX_AGE_HOURS`). The systemd timer runs it every Sunday; `.github/workflows/backup-drill.yml` runs the same scripts weekly against a migrated, seeded CI database so the scripts themselves cannot rot. Wire the service's failure to your alerting (for example `OnFailure=` with an email unit).

## Secrets rotation

Rotating `server.secret_key`: move the old value to `server.secret_key_previous`, set the new one, run `sr-cli rotate-secret-key`, and take a fresh backup of both keys. Older dumps need the key that was current when they were made.
