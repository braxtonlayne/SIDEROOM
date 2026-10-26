#!/bin/sh
# SIDE ROOM backup (XC-OPS-003): a consistent Postgres dump, an optional object-storage snapshot, a manifest
# with a checksum, and retention pruning (docs/12 section 5: 14 daily, 8 weekly, 12 monthly).
#
#   SR_DATABASE_URL        postgres://user:pass@host:5432/sideroom?sslmode=require   (required)
#   SR_BACKUP_DIR          where dumps are written                                    (default ./backups)
#   SR_BACKUP_KEEP         "daily,weekly,monthly" counts to keep                      (default 14,8,12)
#   SR_BACKUP_OBJECTS_DIR  directory holding Garage data; snapshotted when a repository is set
#   SR_BACKUP_REPOSITORY   rustic/restic repository (for example sftp:u123@u123.your-storagebox.de:/sideroom)
#   RUSTIC_PASSWORD_FILE   password file for the repository (never pass the password on the command line)
#   SR_BACKUP_DIR_MODE     mode of the backup folders (default 700; the Compose stack uses 750 so sr-server can
#                          list the folder read-only, see BACKUP.md)
#
# Secrets come from the environment or files, never from arguments.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
. "$here/lib.sh"

: "${SR_DATABASE_URL:?set SR_DATABASE_URL}"
dir=${SR_BACKUP_DIR:-./backups}
keep=${SR_BACKUP_KEEP:-14,8,12}
need pg_dump
need psql
pg_env_from_url "$SR_DATABASE_URL"

mkdir -p "$dir/db"
chmod "${SR_BACKUP_DIR_MODE:-700}" "$dir" "$dir/db" 2>/dev/null || true
# RDY-OPS-008: a dump that died leaves sideroom-<stamp>.dump.partial behind. Remove one older than an hour (a younger one
# may be a dump still running) before starting. Retention below globs *.dump, so it never counts a .partial.
find "$dir/db" -maxdepth 1 -type f -name 'sideroom-*.dump.partial' -mmin +60 | while read -r stale; do
  log "removing the stale partial dump $(basename "$stale")"
  rm -f -- "$stale"
done
stamp=$(date -u +%Y%m%dT%H%M%SZ)
out="$dir/db/sideroom-$stamp.dump"
tmp="$out.partial"

log "dumping $PGDATABASE@$PGHOST to $out"
# Custom format: compressed, parallel-restorable, selective. --no-owner keeps restores portable.
pg_dump --format=custom --no-owner --no-privileges --file="$tmp" "$PGDATABASE"
mv "$tmp" "$out"

sum=$(sha256_of "$out")
size=$(wc -c <"$out" | tr -d ' ')
migrations=$(psql -X -A -t -c "select count(*) from _sqlx_migrations" "$PGDATABASE" 2>/dev/null || echo unknown)
printf '{"file":"%s","created_at":"%s","sha256":"%s","bytes":%s,"migrations":"%s"}\n' \
  "$(basename "$out")" "$stamp" "$sum" "$size" "$migrations" >"$out.json"
log "wrote $(basename "$out") ($size bytes, $migrations migrations)"

if [ -n "${SR_BACKUP_REPOSITORY:-}" ] && [ -n "${SR_BACKUP_OBJECTS_DIR:-}" ]; then
  if command -v rustic >/dev/null 2>&1; then
    tool=rustic
  elif command -v restic >/dev/null 2>&1; then
    tool=restic
  else
    die "SR_BACKUP_REPOSITORY is set but neither rustic nor restic is installed"
  fi
  log "snapshotting objects with $tool"
  if [ -n "${RUSTIC_PASSWORD_FILE:-}" ]; then
    "$tool" -r "$SR_BACKUP_REPOSITORY" -p "$RUSTIC_PASSWORD_FILE" backup "$SR_BACKUP_OBJECTS_DIR" --tag sideroom-objects
  else
    "$tool" -r "$SR_BACKUP_REPOSITORY" backup "$SR_BACKUP_OBJECTS_DIR" --tag sideroom-objects
  fi
  # Marker the System page reads (K-10): its modification time is the last object snapshot.
  date -u +%s >"$dir/snapshot.ok"
fi

# Retention: keep the newest dump of each of the last N days, ISO weeks and months.
daily=${keep%%,*}
rest=${keep#*,}
weekly=${rest%%,*}
monthly=${rest#*,}
ls -1 "$dir"/db/sideroom-*.dump | sort -r | awk -v D="$daily" -v W="$weekly" -v M="$monthly" '
  function iso(ts,   cmd, r) {
    cmd = "date -u -d \"" substr(ts,1,4) "-" substr(ts,5,2) "-" substr(ts,7,2) "\" +%G-W%V"
    cmd | getline r
    close(cmd)
    return r
  }
  {
    f = $0
    match(f, /[0-9]{8}T[0-9]{6}Z/)
    ts = substr(f, RSTART, RLENGTH)
    day = substr(ts, 1, 8); month = substr(ts, 1, 6); week = iso(ts)
    keepit = 0
    if (!(day in seenD) && nd < D) { seenD[day] = 1; nd++; keepit = 1 }
    if (!(week in seenW) && nw < W) { seenW[week] = 1; nw++; keepit = 1 }
    if (!(month in seenM) && nm < M) { seenM[month] = 1; nm++; keepit = 1 }
    if (!keepit) print f
  }' | while read -r old; do
  log "pruning $(basename "$old")"
  rm -f -- "$old" "$old.json"
done
log "backup complete"
