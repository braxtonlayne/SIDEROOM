#!/bin/sh
# Restore a dump made by backup.sh into an EMPTY database (XC-OPS-003).
#
#   restore.sh <dump-file> <target-database-url>
#
# Verifies the checksum from the manifest when present. Refuses to touch a database that already has tables
# unless SR_RESTORE_FORCE=1 (then objects are dropped first). Stop sr-server before restoring in place.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
. "$here/lib.sh"

[ $# -eq 2 ] || die "usage: restore.sh <dump-file> <target-database-url>"
dump=$1
[ -f "$dump" ] || die "no such file: $dump"
need pg_restore
need psql

if [ -f "$dump.json" ]; then
  want=$(sed -n 's/.*"sha256":"\([0-9a-f]*\)".*/\1/p' "$dump.json")
  got=$(sha256_of "$dump")
  [ "$want" = "$got" ] || die "checksum mismatch for $dump (manifest $want, file $got)"
  log "checksum ok"
else
  log "warning: no manifest next to the dump; checksum not verified"
fi

pg_env_from_url "$2"
tables=$(psql -X -A -t -c "select count(*) from information_schema.tables where table_schema='public'" "$PGDATABASE")
if [ "$tables" != "0" ] && [ "${SR_RESTORE_FORCE:-0}" != "1" ]; then
  die "$PGDATABASE already has $tables tables; restore into an empty database or set SR_RESTORE_FORCE=1"
fi

log "restoring into $PGDATABASE@$PGHOST"
if [ "${SR_RESTORE_FORCE:-0}" = "1" ]; then
  pg_restore --clean --if-exists --no-owner --no-privileges --exit-on-error -d "$PGDATABASE" "$dump"
else
  pg_restore --no-owner --no-privileges --exit-on-error -d "$PGDATABASE" "$dump"
fi
log "restore complete; run sr-cli migrate to apply anything newer than the dump"
