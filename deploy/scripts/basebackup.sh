#!/bin/sh
# Physical base backup for point-in-time recovery (XC-OPS-003). Run it from cron or the systemd timer; WAL
# segments arrive continuously through wal-archive.sh, this script provides the starting points.
#
#   SR_DATABASE_URL     postgres://user:pass@host:5432/db  (a role with REPLICATION, or a superuser)
#   SR_BACKUP_DIR       base backups go to $SR_BACKUP_DIR/base/base-<stamp>/            (default ./backups)
#   SR_WAL_ARCHIVE_DIR  the archive that wal-archive.sh fills; trimmed after each run   (optional)
#   SR_BASEBACKUP_KEEP  how many base backups to keep                                   (default 7)
#
# Each backup is a tar pair (base.tar.gz + pg_wal.tar.gz, WAL streamed so the backup restores on its own)
# plus backup.json with the checksum and the first WAL segment it needs. After pruning, archived segments
# older than the oldest kept backup are deleted with pg_archivecleanup, so the archive cannot grow forever.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
. "$here/lib.sh"

: "${SR_DATABASE_URL:?set SR_DATABASE_URL}"
dir=${SR_BACKUP_DIR:-./backups}
keep=${SR_BASEBACKUP_KEEP:-7}
case $keep in ''|*[!0-9]*|0) die "SR_BASEBACKUP_KEEP must be a positive number" ;; esac
need pg_basebackup
need tar
need gzip
pg_env_from_url "$SR_DATABASE_URL"

mkdir -p "$dir/base"
chmod 700 "$dir" "$dir/base" 2>/dev/null || true
stamp=$(date -u +%Y%m%dT%H%M%SZ)
dest="$dir/base/base-$stamp"
tmp="$dest.partial"
rm -rf -- "$tmp"

log "base backup of $PGDATABASE@$PGHOST to $dest"
# -c fast: do not wait for the next scheduled checkpoint. -X stream: the WAL needed to make the backup
# consistent travels with it. -w: never prompt (the password comes from the environment).
pg_basebackup -D "$tmp" -Ft -z -X stream -c fast -w -l "sideroom-$stamp"

[ -f "$tmp/base.tar.gz" ] || die "pg_basebackup did not produce base.tar.gz"
startwal=$(gzip -dc "$tmp/base.tar.gz" | tar -xOf - backup_label | sed -n 's/^START WAL LOCATION: .*(file \([0-9A-F]*\)).*$/\1/p')
[ -n "$startwal" ] || die "could not read START WAL LOCATION from backup_label"
sum=$(sha256_of "$tmp/base.tar.gz")
size=$(wc -c <"$tmp/base.tar.gz" | tr -d ' ')
printf '{"created_at":"%s","start_wal":"%s","sha256":"%s","bytes":%s}\n' \
  "$stamp" "$startwal" "$sum" "$size" >"$tmp/backup.json"
mv "$tmp" "$dest"
log "wrote base-$stamp ($size bytes, first WAL segment $startwal)"

# Retention: the newest $keep backups stay.
count=0
for b in $(ls -1d "$dir"/base/base-* 2>/dev/null | sort -r); do
  count=$((count + 1))
  if [ "$count" -gt "$keep" ]; then
    log "pruning $(basename "$b")"
    rm -rf -- "$b"
  fi
done

# Archive trimming: nothing older than the oldest kept backup's first segment is ever needed.
if [ -n "${SR_WAL_ARCHIVE_DIR:-}" ]; then
  oldest=$(ls -1d "$dir"/base/base-* | sort | head -n 1)
  oldest_wal=$(sed -n 's/.*"start_wal":"\([0-9A-F]*\)".*/\1/p' "$oldest/backup.json")
  if [ -n "$oldest_wal" ] && command -v pg_archivecleanup >/dev/null 2>&1; then
    log "trimming $SR_WAL_ARCHIVE_DIR before $oldest_wal"
    pg_archivecleanup "$SR_WAL_ARCHIVE_DIR" "$oldest_wal"
  fi
fi
log "base backup complete"
