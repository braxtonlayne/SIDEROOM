#!/usr/bin/env bash
# Heavy-work lock, two slots (wave 102). The machine has 16 GB RAM, most of it taken by the owner's own apps.
# Heavy = cargo build/test/clippy, Playwright, `pnpm build`, full vitest runs (more than one file), and a running
# sr-server + Vite test stack.
#   Slot 1 is always available (as before).
#   Slot 2 is granted only while Windows reports at least HEAVY_MIN_MB (default 4500) MB available, measured at the
#   moment of taking it: two stack rounds at once only when memory really allows it.
#
#   bash heavy.sh run <name> <cmd...>   # wait for a slot (up to 9 min), run cmd, release. Exit 75 = still busy, retry.
#   bash heavy.sh acquire <name>        # take a slot for a multi-step job (a test stack + specs). Exit 75 = busy.
#   bash heavy.sh release <name>        # give it back (always, even after a failure)
#   bash heavy.sh status                # "free", or one line per held slot ("<name> HH:MM")
#
# The lock order in a plan is a queue, not an appointment: take a slot as soon as one is free. Hold it as briefly as
# you can; never hold it while you only read or edit code. A slot older than 150 minutes is stale and taken over.
D=$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)   # wave 106: own folder
S1=$D/p50-heavy.lock; S2=$D/p50-heavy.lock2
MIN=${HEAVY_MIN_MB:-4500}
cmd=${1:-status}; name=${2:-?}
stale() { local m; m=$(stat -c %Y "$1" 2>/dev/null) || return 1; [ $(( $(date +%s) - m )) -gt 9000 ]; }
avail() { powershell -NoProfile -Command "[int](Get-CimInstance Win32_PerfFormattedData_PerfOS_Memory).AvailableMBytes" 2>/dev/null | tr -d '\r'; }
grab() { if mkdir "$1" 2>/dev/null; then echo "$name $(date '+%H:%M')" > "$1/owner"; return 0; fi
  if stale "$1"; then echo "stale lock ($(cat "$1/owner" 2>/dev/null)) taken over" >&2; rm -rf "$1"; mkdir "$1" 2>/dev/null && { echo "$name $(date '+%H:%M')" > "$1/owner"; return 0; }; fi
  return 1; }
held() { for s in "$S1" "$S2"; do [ -f "$s/owner" ] && grep -q "^$name " "$s/owner" && { echo "$s"; return 0; }; done; return 1; }
take() {
  if held >/dev/null; then echo "$name already holds a slot" >&2; return 0; fi
  local end=$(( $(date +%s) + ${HEAVY_WAIT_S:-540} ))
  while :; do
    grab "$S1" && return 0
    if [ ! -d "$S2" ] || stale "$S2"; then
      local a; a=$(avail); if [ -n "$a" ] && [ "$a" -ge "$MIN" ] && grab "$S2"; then echo "slot 2 (available ${a} MB)" >&2; return 0; fi
    fi
    [ $(date +%s) -ge $end ] && { echo "heavy lock busy: $(cat "$S1/owner" 2>/dev/null)$( [ -d "$S2" ] && echo "; $(cat "$S2/owner")"); try again later" >&2; return 75; }
    sleep 15
  done
}
case $cmd in
  run) shift 2; take || exit 75; s=$(held); "$@"; rc=$?; [ -n "$s" ] && rm -rf "$s"; exit $rc ;;
  acquire) take ;;
  release) s=$(held) && { rm -rf "$s"; echo released; } || echo "not held by $name" >&2 ;;
  status) out=""; for s in "$S1" "$S2"; do [ -d "$s" ] && out="$out$(cat "$s/owner" 2>/dev/null)
"; done; [ -n "$out" ] && printf "%s" "$out" || echo free ;;
esac
