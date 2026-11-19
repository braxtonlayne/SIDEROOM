#!/usr/bin/env bash
# Shared functions of job.sh, runner.sh and lane.sh (wave 104, D-986/D-987). Sourced, never run.
W=/c/Users/braxt/Desktop/SIDEROOM_wt
D=$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)   # wave 106: own folder (a clone runs itself)
RUN=${SR_RUN_DIR:-$D/run}; RUN=$(cygpath -u "$RUN")
PSQL="/c/Program Files/PostgreSQL/17/bin/psql.exe"
now() { date +%s; }

# conf: plain KEY=VALUE, sourced on every call (so a cost can change without a restart). Defaults first.
load_conf() {
  WAVE=p104 CELL_BIN=$W/p104-cell-bin BASE_DIST=$W/p104-base-dist TEMPLATE_DB=sr_p104_world DB_PREFIX=sr_p104_j
  LANES="1 2" PORT_BASE=8840 METRICS_BASE=9540 COST_SHOOT=900 COST_BUILD=1500 COST_TSC=1900 COST_VITEST=1200
  RESERVE=700 RECENT_S=45 HOL_S=240 JOB_LIMIT_S=600 TSC_MAX=1 BUILD_MAX=1 LANE_WTS="" STOP_AT=""
  COST_CARGO=3000 CARGO_LIMIT_S=1500   # wave 106 (D-1019): the cargo class runs alone
  [ -f "$RUN/conf" ] && . "$RUN/conf"
  export CELL_BIN
}
init_run() {
  mkdir -p "$RUN/jobs" "$RUN/lanes" "$RUN/builds"
  [ -f "$RUN/conf" ] || cat > "$RUN/conf" <<CONF
WAVE=p104
CELL_BIN=$W/p104-cell-bin
BASE_DIST=$W/p104-base-dist
TEMPLATE_DB=sr_p104_world
DB_PREFIX=sr_p104_j
LANES="1 2"
PORT_BASE=8840
METRICS_BASE=9540
COST_SHOOT=900 COST_BUILD=1500 COST_TSC=1900 COST_VITEST=1200 RESERVE=700 RECENT_S=45 HOL_S=240
JOB_LIMIT_S=600 TSC_MAX=1 BUILD_MAX=1 LANE_WTS="" STOP_AT=""
COST_CARGO=3000 CARGO_LIMIT_S=1500
CONF
}

# The one rm -rf of every script (D-987 line 10): a non-empty path under $W with at least $2 (default 2) parts after it, no "..".
safe_rm() {
  local p; p=$(cygpath -u "$1" 2>/dev/null) || return 1; local min=${2:-2}
  [ -n "$p" ] || { echo "safe_rm: empty path" >&2; return 1; }
  case "$p" in *..*) echo "safe_rm: refuses $p" >&2; return 1;; "$W"/*) ;; *) echo "safe_rm: refuses $p" >&2; return 1;; esac
  local rest=${p#"$W"/} n; n=$(echo "$rest" | awk -F/ '{c=0; for(i=1;i<=NF;i++) if($i!="") c++; print c}')
  [ "$n" -ge "$min" ] || { echo "safe_rm: refuses $p (too shallow)" >&2; return 1; }
  rm -rf "$p"
}

# Windows PIDs
winpid() { cat "/proc/$1/winpid" 2>/dev/null || echo ""; }
pid_alive() { [ -n "$1" ] && tasklist //NH //FI "PID eq $1" 2>/dev/null | grep -q "[[:space:]]$1[[:space:]]"; }
kill_tree() { [ -n "$1" ] && taskkill //PID "$1" //T //F >/dev/null 2>&1; return 0; }
port_open() { netstat -ano 2>/dev/null | grep LISTENING | grep -q ":$1[[:space:]]"; }

# Postgres: password only inside this process (never on a command line, never echoed)
pgpw() { [ -n "${PGPASSWORD:-}" ] || { export PGPASSWORD; PGPASSWORD=$(tr -d '\r\n' < "$(cygpath -u "$LOCALAPPDATA")/sideroom/sr_pg_pw.txt"); }; }
sql() { pgpw; "$PSQL" -h 127.0.0.1 -U postgres -v ON_ERROR_STOP=1 -q -t -A -c "$1"; }
dburl() { pgpw; echo "postgres://postgres:$PGPASSWORD@127.0.0.1:5432/$1"; }
# Log filter: replaces the password by <pw> using shell builtins only (nothing on a command line).
filter() { pgpw; local line; while IFS= read -r line || [ -n "$line" ]; do printf '%s\n' "${line//"$PGPASSWORD"/<pw>}"; done; }
scrub_file() { [ -f "$1" ] || return 0; pgpw; local t="$1.scrub" line
  while IFS= read -r line || [ -n "$line" ]; do printf '%s\n' "${line//"$PGPASSWORD"/<pw>}"; done < "$1" > "$t" && mv "$t" "$1"; }

# wave 106: is <folder> a binary folder a shoot job may use? Prints the reason and returns 1 when not (job.sh at submit, runner.sh again at run time).
bin_check() { # <folder>
  local p; p=$(cygpath -u "$1" 2>/dev/null) || { echo "bad path"; return 1; }
  case "$p" in *..*) echo "a path with .. is refused"; return 1;; "$W"/?*) ;; *) echo "the folder must be under $W/"; return 1;; esac
  case "/${p%/}/" in */p80-base-bin/*|*/p92-base-bin/*) echo "p80-base-bin and p92-base-bin are never used"; return 1;; esac
  [ -d "$p" ] || { echo "no folder $p"; return 1; }
  [ -f "$p/sr-server.exe" ] || { echo "no sr-server.exe in $p"; return 1; }
  [ -f "$p/sr-cli.exe" ] || { echo "no sr-cli.exe in $p"; return 1; }
  return 0
}

# job files
jd() { echo "$RUN/jobs/$1"; }
jget() { cat "$(jd "$1")/$2" 2>/dev/null; }
jset() { echo "$3" > "$(jd "$1")/$2"; }
jreq() { grep -m1 "^$2=" "$(jd "$1")/req" 2>/dev/null | cut -d= -f2-; }     # one value of req
jreq_all() { grep "^$2=" "$(jd "$1")/req" 2>/dev/null | cut -d= -f2-; }       # repeated key, one per line
jids() { ls "$RUN/jobs" 2>/dev/null | grep -E '^[0-9]{4}-' | sort; }
jclass() { local x=${1#*-}; echo "${x##*-}"; }
ticket() { echo "${1%%-*}"; }
job_cost() { case $(jclass "$1") in shoot) echo "$COST_SHOOT";; build) echo "$COST_BUILD";; tsc) echo "$COST_TSC";; vitest) echo "$COST_VITEST";; cargo) echo "$COST_CARGO";; test) jreq "$1" mb;; *) echo 0;; esac; }
runner_alive() { local p; p=$(cat "$RUN/runner.pid" 2>/dev/null); pid_alive "$p"; }
# the build key of a worktree: its folder name without the wave prefix
build_key() { local b; b=$(basename "$1"); echo "${b#"$WAVE"-}"; }
tree_sig() { local wt=$1; { git -C "$wt" rev-parse HEAD; { git -C "$wt" status --porcelain -- apps/web packages; git -C "$wt" diff HEAD -- apps/web packages; } | md5sum | cut -d' ' -f1; } | paste -sd' '; }
avail_mb() { if [ -f "$RUN/fake-avail" ]; then tr -d '\r\n ' < "$RUN/fake-avail"; else
  powershell -NoProfile -Command "[int](Get-CimInstance Win32_PerfFormattedData_PerfOS_Memory).AvailableMBytes" 2>/dev/null | tr -d '\r'; fi; }

# Stop what a shoot job left in a lane: server by its recorded Windows PID, database dropped, ports checked, log scrubbed.
lane_clean() { # <lane> <db>
  local n=$1 db=$2 lf="$RUN/lanes/$1" p
  p=$(cat "$lf/winpid" 2>/dev/null); kill_tree "$p"; rm -f "$lf/winpid" "$lf/pid"
  kill_listener $((PORT_BASE + n)); kill_listener $((METRICS_BASE + n))
  [ -n "$db" ] && sql "DROP DATABASE IF EXISTS $db WITH (FORCE);" >/dev/null 2>&1
  sleep 1; local bad=""
  port_open $((PORT_BASE + n)) && bad="$bad $((PORT_BASE + n))"; port_open $((METRICS_BASE + n)) && bad="$bad $((METRICS_BASE + n))"
  scrub_file "$lf/server.log"
  [ -z "$bad" ] || echo "lane $n: ports still open:$bad" >&2
}

# End a job: stop what it runs (job PID tree, then its lane), then rc, ended, and state LAST. Never overwrites an end state.
end_job() { # <id> <state> <rc>
  local id=$1 st=$2 rc=$3 cur p lane db
  cur=$(jget "$id" state); case $cur in done|failed|killed|cancelled) return 0;; esac
  if [ "$cur" = running ]; then
    p=$(jget "$id" pid); pid_alive "$p" && kill_tree "$p"
    lane=$(jget "$id" lane); db=$(jget "$id" db)
    [ -n "$lane" ] && [ "$(jclass "$id")" = shoot ] && lane_clean "$lane" "$db"
  fi
  jset "$id" rc "$rc"; jset "$id" ended "$(now)"; jset "$id" state "$st"
}
# lane_clean's safety net: whatever still listens on a lane port is the lane's server (the ports are the runner's): stop it by that Windows PID
kill_listener() { local pt=$1 lp; for lp in $(netstat -ano 2>/dev/null | grep LISTENING | grep ":$pt[[:space:]]" | awk '{print $NF}' | sort -u); do kill_tree "$lp"; done; }
