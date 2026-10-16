#!/bin/sh
# Postgres restore_command target (XC-OPS-003). Written into postgresql.auto.conf by pitr-restore.sh; not
# meant to be run by hand.
#
#   restore_command = 'sh /opt/sideroom/scripts/wal-restore.sh /wal-archive %f %p'
#
# Exit 0 after copying the segment, non-zero when the archive does not have it (which is how Postgres
# learns that recovery has reached the end of the archive).
set -eu

[ $# -eq 3 ] || { echo "usage: wal-restore.sh <archive-dir> <file-name> <destination>" >&2; exit 2; }
dir=$1
name=$2
dest=$3
case $name in
  ''|*/*|*..*) exit 2 ;;
esac
[ -f "$dir/$name" ] || exit 1
cp -- "$dir/$name" "$dest"
