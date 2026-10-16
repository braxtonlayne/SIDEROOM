#!/bin/sh
# Point-in-time recovery (XC-OPS-003): unpack a base backup into an EMPTY data directory and configure it to
# replay archived WAL up to a moment. It does not start Postgres; it prints the command that does.
#
#   pitr-restore.sh <base-backup-dir> <data-dir> <wal-archive-dir> [target-time]
#
#   target-time  "2026-10-14 09:30:00+00" (any timestamp Postgres accepts). Omit it to replay everything
#                that was archived (the latest recoverable state).
#
# Stop sr-server and the old Postgres first. The recovered server promotes itself when it reaches the
# target, so it comes up writable on a new timeline; point sr-server at it only after checking the data.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
. "$here/lib.sh"

if [ $# -lt 3 ] || [ $# -gt 4 ]; then
  die "usage: pitr-restore.sh <base-backup-dir> <data-dir> <wal-archive-dir> [target-time]"
fi
base=$1
data=$2
archive=$3
target=${4:-}
need tar
need gzip

[ -f "$base/base.tar.gz" ] || die "$base/base.tar.gz not found (is that a base backup directory?)"
[ -d "$archive" ] || die "WAL archive directory $archive does not exist"
if [ -f "$base/backup.json" ]; then
  want=$(sed -n 's/.*"sha256":"\([0-9a-f]*\)".*/\1/p' "$base/backup.json")
  got=$(sha256_of "$base/base.tar.gz")
  [ "$want" = "$got" ] || die "checksum mismatch for $base/base.tar.gz (manifest $want, file $got)"
  log "checksum ok"
fi
if [ -d "$data" ] && [ -n "$(ls -A "$data" 2>/dev/null)" ]; then
  die "$data is not empty; recover into a new directory"
fi
case $target in
  *"'"*) die "target-time must not contain a quote" ;;
esac

mkdir -p "$data"
chmod 700 "$data"
log "unpacking $(basename "$base")"
gzip -dc "$base/base.tar.gz" | tar -xf - -C "$data"
if [ -f "$base/pg_wal.tar.gz" ]; then
  mkdir -p "$data/pg_wal"
  gzip -dc "$base/pg_wal.tar.gz" | tar -xf - -C "$data/pg_wal"
fi

abs_archive=$(cd "$archive" && pwd)
{
  echo "# written by pitr-restore.sh"
  echo "restore_command = 'sh \"$here/wal-restore.sh\" \"$abs_archive\" %f %p'"
  if [ -n "$target" ]; then
    echo "recovery_target_time = '$target'"
  fi
  echo "recovery_target_action = 'promote'"
} >>"$data/postgresql.auto.conf"
: >"$data/recovery.signal"

if [ -n "$target" ]; then
  log "ready: recovery will stop at $target"
else
  log "ready: recovery will replay the whole archive"
fi
echo "start it with:  pg_ctl -D '$data' -o '-p <free port>' start"
