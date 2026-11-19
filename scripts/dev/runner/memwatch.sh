#!/usr/bin/env bash
# memwatch.sh <out.csv> [seconds=5]  (parallel-design.md): samples available memory and the working set of the
# stack and tooling process groups until killed. Columns: time, availableMB, node, sr-server, headless Chromium,
# postgres, claude, and node processes running tsc/vitest/eslint (MB).
out=${1:?out.csv}; every=${2:-5}
echo "time,availMB,node,sr-server,chrome-headless,postgres,claude,tsc_like" > "$out"
while :; do powershell -NoProfile -ExecutionPolicy Bypass -File "$(cygpath -m "$(dirname "$(readlink -f "$0")")")/memwatch.ps1" 2>/dev/null | tr -d '\r' >> "$out"; sleep "$every"; done
