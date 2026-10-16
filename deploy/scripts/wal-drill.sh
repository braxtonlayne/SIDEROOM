#!/bin/sh
# Point-in-time recovery drill (XC-OPS-003): proves, on a scratch Postgres cluster, that the WAL archiving
# scripts really give back a past state. Needs the Postgres 17 server binaries on PATH (initdb, pg_ctl, psql,
# pg_basebackup, pg_archivecleanup); it never touches an existing cluster or database.
#
#   1. initdb a throwaway cluster with archive_command = wal-archive.sh and start it
#   2. load 100 rows, take a base backup with basebackup.sh
#   3. load 100 more rows, note the time, load 100 more, switch WAL and wait until it is archived
#   4. stop the cluster, pitr-restore.sh the base backup to the noted time, start the copy
#   5. the copy must hold exactly the first 200 rows, on a new timeline, and the archive must be trimmed
#
#   SR_DRILL_PORT  port of the scratch cluster; the recovered copy uses the next one   (default 55441)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
. "$here/lib.sh"

for b in initdb pg_ctl psql pg_basebackup pg_archivecleanup; do need "$b"; done
port=${SR_DRILL_PORT:-55441}
port2=$((port + 1))

work=$(mktemp -d)
# Native Windows binaries need C:/ style paths; everywhere else the path is already fine.
native() { if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else printf '%s' "$1"; fi; }
cleanup() {
  pg_ctl -D "$work/pg" stop -m immediate >/dev/null 2>&1 || true
  pg_ctl -D "$work/restored" stop -m immediate >/dev/null 2>&1 || true
  rm -rf -- "$work"
}
trap cleanup EXIT INT TERM

mkdir -p "$work/archive" "$work/backups"
scripts=$(native "$here")
archive=$(native "$work/archive")
q() { psql -X -A -t -h 127.0.0.1 -p "$1" -U postgres -d postgres -c "$2"; }

log "initialising a scratch cluster on port $port"
initdb -D "$(native "$work/pg")" -U postgres -A trust -E UTF8 --no-sync >/dev/null
cat >>"$work/pg/postgresql.conf" <<CONF
listen_addresses = '127.0.0.1'
port = $port
unix_socket_directories = ''
fsync = off
wal_level = replica
max_wal_senders = 4
archive_mode = on
archive_command = 'sh "$scripts/wal-archive.sh" "$archive" "%p" "%f"'
CONF
pg_ctl -D "$(native "$work/pg")" -l "$(native "$work/pg.log")" -w start >/dev/null

q "$port" "create table drill (id int primary key, note text)" >/dev/null
q "$port" "insert into drill select g, 'before backup' from generate_series(1,100) g" >/dev/null

log "base backup"
SR_DATABASE_URL="postgres://postgres@127.0.0.1:$port/postgres" \
  SR_BACKUP_DIR="$(native "$work/backups")" SR_WAL_ARCHIVE_DIR="$archive" \
  sh "$here/basebackup.sh" >/dev/null
base=$(ls -1d "$work"/backups/base/base-* | head -n 1)
[ -f "$base/backup.json" ] || die "the base backup left no manifest"

q "$port" "insert into drill select g, 'before target' from generate_series(101,200) g" >/dev/null
q "$port" "select pg_sleep(1)" >/dev/null
target=$(q "$port" "select to_char(clock_timestamp() at time zone 'utc', 'YYYY-MM-DD HH24:MI:SS.US') || '+00'")
q "$port" "select pg_sleep(1)" >/dev/null
q "$port" "insert into drill select g, 'after target' from generate_series(201,300) g" >/dev/null

log "switching WAL and waiting for the archive"
last=$(q "$port" "select pg_walfile_name(pg_switch_wal())")
tries=0
until [ -f "$work/archive/$last" ]; do
  tries=$((tries + 1))
  [ "$tries" -le 60 ] || die "segment $last was not archived within 60 s (see $work/pg.log)"
  sleep 1
done
# WAL segment names in the archive: 24 upper-case hex digits (a glob, not ls | grep: SC2010).
segments() {
  for f in "$work/archive"/*; do
    b=${f##*/}
    case $b in *[!0-9A-F]*) continue ;; esac
    [ ${#b} -eq 24 ] && printf '%s\n' "$b"
  done
  return 0
}
log "archive holds $(segments | wc -l | tr -d ' ') segment(s); newest $last"

pg_ctl -D "$(native "$work/pg")" -m fast -w stop >/dev/null

log "recovering to $target"
sh "$here/pitr-restore.sh" "$base" "$work/restored" "$work/archive" "$target" >/dev/null
pg_ctl -D "$(native "$work/restored")" -l "$(native "$work/restored.log")" -w \
  -o "-p $port2 -c archive_mode=off -c listen_addresses=127.0.0.1" start >/dev/null

tries=0
while [ "$(q "$port2" "select pg_is_in_recovery()")" != "f" ]; do
  tries=$((tries + 1))
  [ "$tries" -le 60 ] || die "the recovered cluster did not finish recovery (see $work/restored.log)"
  sleep 1
done

rows=$(q "$port2" "select count(*) from drill")
late=$(q "$port2" "select count(*) from drill where note = 'after target'")
tl=$(q "$port2" "select timeline_id from pg_control_checkpoint()")
[ "$rows" = "200" ] || die "expected 200 rows after recovery, found $rows"
[ "$late" = "0" ] || die "rows written after the target time survived recovery"
[ "$tl" -ge 2 ] || die "the recovered cluster did not switch timeline (timeline $tl)"
log "recovered 200 rows, none after the target, timeline $tl"

# The base backup recorded its first segment; nothing older than it may remain after trimming.
# Archiving is asynchronous, so a segment older than the backup can reach the archive after
# basebackup.sh trimmed it; the next backup's trim removes it. Trim once more here, as that run would.
first=$(sed -n 's/.*"start_wal":"\([0-9A-F]*\)".*/\1/p' "$base/backup.json")
[ -n "$first" ] || die "the base backup manifest has no start_wal"
pg_archivecleanup "$archive" "$first"
oldest=$(segments | sort | head -n 1)
smallest=$(printf '%s\n%s\n' "$first" "$oldest" | sort | head -n 1)
[ "$smallest" = "$first" ] || die "archive still holds $oldest, older than $first"
log "drill ok"
