#!/bin/sh
# Restore drill (XC-OPS-003): proves the newest backup restores and the data is plausible, in a throwaway
# database that is dropped afterwards. Exit 0 = the backup is usable. Run weekly (the systemd timer in
# deploy/systemd or .github/workflows/backup-drill.yml).
#
#   SR_DATABASE_URL   URL of a role that can create databases on the same server. The drill creates and
#                     drops its own database; the one named in the URL is only used to connect.
#   SR_BACKUP_DIR     where backup.sh writes                                   (default ./backups)
#   SR_DRILL_MAX_AGE_HOURS  fail when the newest backup is older than this     (default 36)
#   SR_BACKUP_REPOSITORY, SR_BACKUP_OBJECTS_DIR, SR_BACKUP_OBJECTS_META_DIR, RUSTIC_PASSWORD_FILE
#                     when the first three are all set (the object backup is configured), a last step
#                     restores the latest object snapshot into a scratch folder and checks it holds both
#                     Garage's metadata and at least one block (D-1045). It starts nothing.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
. "$here/lib.sh"

: "${SR_DATABASE_URL:?set SR_DATABASE_URL}"
dir=${SR_BACKUP_DIR:-./backups}
need psql
need createdb
need dropdb

dump=$(ls -1 "$dir"/db/sideroom-*.dump 2>/dev/null | sort | tail -n 1 || true)
[ -n "$dump" ] || die "no backups found in $dir/db"
age_h=$(( ( $(date +%s) - $(date -r "$dump" +%s) ) / 3600 ))
max=${SR_DRILL_MAX_AGE_HOURS:-36}
log "newest backup: $(basename "$dump") (${age_h}h old)"
[ "$age_h" -le "$max" ] || die "newest backup is older than ${max}h"

pg_env_from_url "$SR_DATABASE_URL"
drill="sr_restore_drill_$$"
scratch=
cleanup() {
  dropdb --if-exists "$drill" >/dev/null 2>&1 || true
  if [ -n "$scratch" ]; then rm -rf -- "$scratch"; fi
}
trap cleanup EXIT INT TERM

createdb "$drill"
drill_url="postgres://$PGUSER:$PGPASSWORD@$PGHOST:$PGPORT/$drill"
"$here/restore.sh" "$dump" "$drill_url"

q() { psql -X -A -t -d "$drill" -c "$1"; }
migrations=$(q "select count(*) from _sqlx_migrations")
tables=$(q "select count(*) from information_schema.tables where table_schema='public'")
[ "$migrations" -gt 0 ] || die "restored database has no applied migrations"
[ "$tables" -gt 10 ] || die "restored database has only $tables tables"
failed=$(q "select count(*) from _sqlx_migrations where success = false")
[ "$failed" = "0" ] || die "$failed failed migrations in the restored database"
log "drill ok: $migrations migrations, $tables tables"
# D-1045: the object backup restores, with its metadata. Only when it is configured.
if [ -n "${SR_BACKUP_REPOSITORY:-}" ] && [ -n "${SR_BACKUP_OBJECTS_DIR:-}" ] && [ -n "${SR_BACKUP_OBJECTS_META_DIR:-}" ]; then
  if command -v rustic >/dev/null 2>&1; then
    tool=rustic
  elif command -v restic >/dev/null 2>&1; then
    tool=restic
  else
    die "the object backup is configured but neither rustic nor restic is installed"
  fi
  scratch=$(mktemp -d)
  log "restoring the latest object snapshot into a scratch folder with $tool"
  pw=
  if [ -n "${RUSTIC_PASSWORD_FILE:-}" ]; then pw="-p $RUSTIC_PASSWORD_FILE"; fi
  # Flag shapes differ (to be confirmed against the installed version): restic takes --target, rustic a destination.
  if [ "$tool" = rustic ]; then
    # shellcheck disable=SC2086
    rustic -r "$SR_BACKUP_REPOSITORY" $pw restore latest "$scratch"
  else
    # shellcheck disable=SC2086
    restic -r "$SR_BACKUP_REPOSITORY" $pw restore latest --target "$scratch"
  fi
  # Snapshots keep absolute paths, so each folder lands under the scratch folder at its own path.
  meta="$scratch$SR_BACKUP_OBJECTS_META_DIR"
  data="$scratch$SR_BACKUP_OBJECTS_DIR"
  [ -d "$meta" ] && [ -n "$(find "$meta" -mindepth 1 -print -quit)" ] || die "the object snapshot has no metadata content under $SR_BACKUP_OBJECTS_META_DIR: a data-only snapshot cannot be restored"
  [ -d "$data" ] && [ -n "$(find "$data" -type f -print -quit)" ] || die "the object snapshot holds no block under $SR_BACKUP_OBJECTS_DIR"
  log "object drill ok: metadata and at least one block restored"
fi
# Marker the System page reads (K-10): its modification time is the last successful restore check.
date -u +%s >"$(dirname "$dump")/../restore-drill.ok"
