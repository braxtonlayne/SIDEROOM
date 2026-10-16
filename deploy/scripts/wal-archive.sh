#!/bin/sh
# Postgres archive_command target (XC-OPS-003, continuous archiving for point-in-time recovery).
#
#   archive_command = '/opt/sideroom/scripts/wal-archive.sh /wal-archive %p %f'
#
# Copies one finished WAL segment into the archive directory. Postgres retries until this exits 0, so the
# script is idempotent (an identical file already there counts as success), never overwrites a different
# file (that would corrupt the archive; it fails loudly instead) and publishes atomically: the segment
# is copied to a .partial name and renamed, so a restore never reads a half-written file.
set -eu

[ $# -eq 3 ] || { echo "usage: wal-archive.sh <archive-dir> <path> <file-name>" >&2; exit 2; }
dir=$1
src=$2
name=$3
case $name in
  ''|*/*|*..*) echo "wal-archive: refusing file name '$name'" >&2; exit 2 ;;
esac
[ -d "$dir" ] || { echo "wal-archive: archive directory $dir does not exist" >&2; exit 1; }
[ -f "$src" ] || { echo "wal-archive: $src is not a file" >&2; exit 1; }

dest=$dir/$name
if [ -f "$dest" ]; then
  if cmp -s "$src" "$dest"; then
    exit 0
  fi
  echo "wal-archive: $name is already archived with different content" >&2
  exit 1
fi

tmp=$dest.partial.$$
trap 'rm -f -- "$tmp"' EXIT INT TERM
cp -- "$src" "$tmp"
if command -v sync >/dev/null 2>&1; then sync; fi
mv -- "$tmp" "$dest"
trap - EXIT INT TERM
