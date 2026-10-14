#!/bin/sh
# Restore drill (XC-OPS-003): proves the newest backup restores and the data is plausible, in a throwaway
# database that is dropped afterwards. Exit 0 = the backup is usable. Run weekly (the systemd timer in
# deploy/systemd or .github/workflows/backup-drill.yml).
#
#   SR_DATABASE_URL   URL of a role that can create databases on the same server. The drill creates and
#                     drops its own database; the one named in the URL is only used to connect.
#   SR_BACKUP_DIR     where backup.sh writes                                   (default ./backups)
#   SR_DRILL_MAX_AGE_HOURS  fail when the newest backup is older than this     (default 36)
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
cleanup() { dropdb --if-exists "$drill" >/dev/null 2>&1 || true; }
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
