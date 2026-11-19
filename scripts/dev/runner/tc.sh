#!/usr/bin/env bash
# Web typecheck lock (wave 102, parallel-design.md). A web `tsc --noEmit` peaks at 1.8 GB for about 30 s, so only
# one runs at a time. Use: bash tc.sh run <name> bash -c "cd <worktree>/apps/web && npx tsc --noEmit -p ."
# Run it once, after your last edit. Exit 75 = busy for 9 min, retry.
#
#   bash tc.sh run <name> <cmd...>   # wait for the lock (up to 9 min), run cmd, release. Exit 75 = still busy, retry.
#   bash tc.sh acquire <name>        # take it for a multi-step job. Exit 75 = busy.
#   bash tc.sh release <name>        # give it back (always, even after a failure)
#   bash tc.sh status
#
# Hold it as briefly as you can; never hold it while you only read or edit code. A lock older than 150 minutes (a full gate run takes ~85) is
# treated as stale and taken over.
L=$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)/tc.lock   # wave 106: own folder
cmd=${1:-status}; name=${2:-?}
stale() { local m; m=$(stat -c %Y "$L" 2>/dev/null) || return 1; [ $(( $(date +%s) - m )) -gt 9000 ]; }
take() {
  local end=$(( $(date +%s) + 540 ))
  while :; do
    if mkdir "$L" 2>/dev/null; then echo "$name $(date '+%H:%M')" > "$L/owner"; return 0; fi
    if stale; then echo "stale lock ($(cat "$L/owner" 2>/dev/null)) taken over" >&2; rm -rf "$L"; continue; fi
    [ $(date +%s) -ge $end ] && { echo "heavy lock busy: $(cat "$L/owner" 2>/dev/null); try again later" >&2; return 75; }
    sleep 15
  done
}
case $cmd in
  run) shift 2; take || exit 75; "$@"; rc=$?; rm -rf "$L"; exit $rc ;;
  acquire) take ;;
  release) if [ -f "$L/owner" ] && grep -q "^$name " "$L/owner"; then rm -rf "$L"; echo released; else echo "not held by $name" >&2; fi ;;
  status) [ -d "$L" ] && cat "$L/owner" || echo free ;;
esac
