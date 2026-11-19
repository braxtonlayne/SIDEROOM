#!/usr/bin/env bash
# runner.sh (wave 104, D-986/D-987): the job runner. State lives in $SR_RUN_DIR (default cli-agents/run); contract in the wave 104 brief.
#   runner.sh start | stop [--now] | status | run        start = background; stop = no new job, exit when idle; --now kills the running jobs
#   runner.sh prepare <base worktree>                    build it, copy dist to BASE_DIST, seed TEMPLATE_DB (needs a heavy.sh slot)
#   runner.sh exec <id>                                  (internal) the body of one job, run by the loop as its top process
. "$(dirname "$(readlink -f "$0")")/runlib.sh"
load_conf; init_run
SELF=$D/runner.sh; TICK=${TICK:-3}
log() { echo "$(date '+%H:%M:%S') $*" >> "$RUN/runner.log"; }
# wave 106: why a queued job is held. One line when the reason first holds or changes (the key), never one per pass.
hold() { # <id> <key> <text...>
  local id=$1 key=$2; shift 2
  [ "$(cat "$(jd "$id")/hold" 2>/dev/null)" = "$key" ] || { echo "$key" > "$(jd "$id")/hold"; log "hold $id $*"; }
}

# ---------------------------------------------------------------- job bodies (run inside `exec`, output goes through the filter)
run_shoot() { # <id>
  local id=$1 wt phase lane port mport db lf dist x key rec r
  wt=$(jget "$id" wt); phase=$(jreq "$id" phase); lane=$(jget "$id" lane); db=$(jget "$id" db)
  port=$((PORT_BASE + lane)); mport=$((METRICS_BASE + lane)); lf=$RUN/lanes/$lane
  [ -d "$wt/e2e" ] || { echo "no $wt/e2e"; return 2; }
  dist=$(jreq "$id" dist)
  local jbin; jbin=$(jreq "$id" bin)
  if [ -n "$jbin" ]; then   # wave 106: this job's own server and cli (checked again: the folder may have changed since the submit)
    r=$(bin_check "$jbin") || { echo "--bin refused at run time: $r"; return 2; }
    export CELL_BIN=$jbin; echo "[job] binaries from $CELL_BIN"
  fi
  if [ -z "$dist" ]; then
    if [ "$phase" = base ]; then dist=$BASE_DIST; else
      dist=$wt/apps/web/dist; key=$(build_key "$wt"); rec=$(sed -n 1p "$RUN/builds/$key" 2>/dev/null)
      [ -n "$rec" ] || { echo "stale build: no build record for $key (submit a build job first)"; return 3; }
      [ "$rec" = "$(tree_sig "$wt")" ] || { echo "stale build: the tree of $key changed since its last build (was: $rec)"; return 3; }
    fi
  fi
  [ -d "$dist" ] || { echo "no dist at $dist"; return 2; }
  mkdir -p "$lf"; safe_rm "$lf/search"; rm -f "$lf/server.log" "$lf/pid" "$lf/winpid"
  echo "[job] lane $lane port $port db $db phase $phase dist $dist"
  # one clone at a time: CREATE DATABASE ... TEMPLATE needs the template free
  local i got=0 crc=0
  for i in $(seq 1 120); do mkdir "$RUN/clone.lock" 2>/dev/null && { got=1; break; }; sleep 0.5; done
  [ $got = 1 ] || { echo "clone lock busy"; return 1; }
  if [ "$(jreq "$id" fresh)" = 1 ]; then
    { sql "DROP DATABASE IF EXISTS $db;" && sql "CREATE DATABASE $db;" && safe_rm "$wt/e2e/.state" && mkdir -p "$lf/search"; } || crc=1
  else
    bash "$D/world.sh" clone "$TEMPLATE_DB" "$db" "$port" "$wt" "$lf" || crc=1
  fi
  rmdir "$RUN/clone.lock"
  [ $crc = 0 ] || { lane_clean "$lane" "$db"; return 1; }
  local senv; senv=$(jreq_all "$id" senv)
  CELL_SERVER_ENV="$senv" bash "$D/cell.sh" up "$port" "$mport" "$(cygpath -m "$dist")" "$db" "$lf" || {
    echo "[job] cell did not start"; tail -5 "$lf/server.log" 2>/dev/null; lane_clean "$lane" "$db"; return 1; }
  local rc=0; local args=(); while IFS= read -r x; do args+=("$x"); done < <(jreq_all "$id" arg)
  (
    cd "$wt/e2e" || exit 1
    export SR_E2E_WEB=server SR_E2E_SERVER_PORT="$port" SR_E2E_BASE_URL="http://localhost:$port" SR_CLI_BIN="$CELL_BIN/sr-cli.exe"
    export SR_DATABASE__URL; SR_DATABASE__URL=$(dburl "$db"); export DATABASE_URL=$SR_DATABASE__URL
    while IFS= read -r x; do [ -n "$x" ] && export "$x"; done < <(jreq_all "$id" env)
    if [ "$(jreq "$id" fresh)" = 1 ]; then
      echo "[job] seeding the sweep world (fresh)"
      SR_E2E_SWEEP=1 npx playwright test tests/page-sweep.spec.ts -g "seed the sweep world" --project=chromium --workers=1 --retries=0 --reporter=line 2>&1 | tee "$lf/seed.out"
      [ "${PIPESTATUS[0]}" = 0 ] && ! grep -q FAIL "$lf/seed.out" || { echo "[job] seed failed or printed FAIL"; exit 1; }
    fi
    npx playwright test "${args[@]}" --project=chromium --workers=1 --retries=0 --reporter=line
  ); rc=$?
  echo "[job] playwright rc $rc"
  lane_clean "$lane" "$db"
  return $rc
}

# wave 106 (D-1019): a cargo job. The real cargo unless SR_CARGO_CMD names another command (the drill's stub). Target folder and debug info as D-999 asks.
run_cargo() { # <id>
  local id=$1 wt sub x name; local -a args=() extra=()
  wt=$(jget "$id" wt); name=$(jreq "$id" name)
  while IFS= read -r x; do args+=("$x"); done < <(jreq_all "$id" arg)
  sub=${args[0]}; args=("${args[@]:1}")
  case $sub in fmt) ;; *) extra=(--offline);; esac        # cargo fmt takes no --offline
  [ -d "$wt" ] || { echo "no $wt"; return 2; }
  export CARGO_TARGET_DIR=$W/$WAVE-$name-target CARGO_PROFILE_DEV_DEBUG=0
  echo "[cargo] $sub ${extra[*]} ${args[*]} in $wt, target $CARGO_TARGET_DIR"
  cd "$wt" && ${SR_CARGO_CMD:-cargo} "$sub" "${extra[@]}" "${args[@]}"
}

run_body() { # <id>: the class's work; its exit code is the job's
  local id=$1 wt cls; cls=$(jclass "$id"); wt=$(jget "$id" wt)
  case $cls in
    test) echo "[test] hold $(jreq "$id" mb) MB, sleep $(jreq "$id" sec) s, exit $(jreq "$id" rc)"; sleep "$(jreq "$id" sec)"; return "$(jreq "$id" rc)";;
    tsc) cd "$wt/apps/web" && npx tsc --noEmit -p .;;
    build) cd "$wt" && VITE_SR_E2E=1 VITE_ACTIVITIES_ENABLED=true node scripts/build-web.mjs || return $?
           { tree_sig "$wt"; now; } > "$RUN/builds/$(build_key "$wt")"; echo "[build] recorded $(build_key "$wt")";;
    vitest) local files=() x; while IFS= read -r x; do files+=("$x"); done < <(jreq_all "$id" arg)
            cd "$wt/apps/web" && npx vitest run --maxWorkers=2 "${files[@]}";;
    shoot) run_shoot "$id";;
    cargo) run_cargo "$id";;
  esac
}

do_exec() { # <id>
  local id=$1 rc
  jset "$id" pid "$(winpid $$)"
  run_body "$id" 2>&1 | filter >> "$(jd "$id")/log"; rc=${PIPESTATUS[0]}
  # write the end myself (end_job would kill this very process); rc and ended first, state last
  if [ "$(jget "$id" state)" = running ]; then
    jset "$id" rc "$rc"; jset "$id" ended "$(now)"; jset "$id" state "$([ "$rc" = 0 ] && echo done || echo failed)"
  fi
}

# ---------------------------------------------------------------- the loop
start_job() { # <id> <lane> <wt>
  local id=$1 lane=$2 wt=$3
  jset "$id" wt "$wt"; jset "$id" started "$(now)"
  if [ -n "$lane" ]; then jset "$id" lane "$lane"; jset "$id" db "$DB_PREFIX$(ticket "$id")"; fi
  : > "$(jd "$id")/log"
  jset "$id" state running                      # before the launch: a job that ends at once must be able to write its end state
  SR_RUN_DIR=$RUN nohup bash "$SELF" exec "$id" > /dev/null 2>&1 &
  local wp; wp=$(winpid $!); if [ -n "$wp" ] && [ ! -s "$(jd "$id")/pid" ]; then jset "$id" pid "$wp"; fi
  log "started $id lane=${lane:-} wt=$wt"
}

end_job_q() { # a queued job that never ran: <id> [rc] [message]
  local id=$1 rc=${2:-143} msg=${3:-}; [ -n "$msg" ] && echo "[runner] $msg" > "$(jd "$id")/log"
  jset "$id" started "$(now)"; jset "$id" rc "$rc"; jset "$id" ended "$(now)"; jset "$id" state "$([ "$rc" = 143 ] && echo cancelled || echo failed)"
}

pass() { # returns 1 when the runner should exit
  load_conf
  local t id s p mode; t=$(now); mode=$(cat "$RUN/stop" 2>/dev/null)
  # 1. reap: cancels, the limit, orphans; count what runs
  local rwts="" rlanes="" nrun=0 ntsc=0 nbuild=0 ncargo=0 recent=0 st lim
  for id in $(jids); do
    s=$(jget "$id" state)
    if [ "$s" = queued ] && [ -f "$(jd "$id")/cancel" ]; then end_job_q "$id"; continue; fi
    [ "$s" = running ] || continue
    if [ -f "$(jd "$id")/cancel" ]; then log "cancel $id"; end_job "$id" cancelled 143; continue; fi
    if [ "$mode" = now ]; then log "stop --now: $id"; end_job "$id" killed 143; continue; fi
    st=$(jget "$id" started)
    lim=$JOB_LIMIT_S; [ "$(jclass "$id")" = cargo ] && lim=$CARGO_LIMIT_S
    if [ $(( t - st )) -gt "$lim" ]; then log "limit: $id"; echo "[runner] killed: over ${lim}s" >> "$(jd "$id")/log"; end_job "$id" killed 124; continue; fi
    p=$(jget "$id" pid)
    if { [ -n "$p" ] && ! pid_alive "$p"; } || { [ -z "$p" ] && [ $(( t - st )) -gt 30 ]; }; then
      if [ "$(jget "$id" state)" = running ]; then log "orphan: $id (pid ${p:-none})"; echo "[runner] orphaned job" >> "$(jd "$id")/log"; end_job "$id" failed 137; fi
      continue
    fi
    nrun=$((nrun+1)); rwts="$rwts|$(jget "$id" wt)|"; rlanes="$rlanes $(jget "$id" lane)"
    case $(jclass "$id") in tsc) ntsc=$((ntsc+1));; build) nbuild=$((nbuild+1));; cargo) ncargo=$((ncargo+1));; esac
    [ $(( t - st )) -lt "$RECENT_S" ] && recent=$(( recent + $(job_cost "$id") ))
  done
  if [ -n "$mode" ]; then { [ "$mode" = now ] || [ $nrun -eq 0 ]; } && return 1; return 0; fi   # stop asked: no admissions; exit when idle
  # 2. admission, in ticket order
  local avail stopep=""; avail=$(avail_mb); [ -n "$avail" ] || avail=0
  if [ -n "$STOP_AT" ]; then stopep=$(date -d "today $STOP_AT" +%s 2>/dev/null); fi
  local head="" first=1 cls cost wt lane l w ok hol why wkey
  for id in $(jids); do
    [ "$(jget "$id" state)" = queued ] || continue
    lim=$JOB_LIMIT_S; [ "$(jclass "$id")" = cargo ] && lim=$CARGO_LIMIT_S
    if [ -n "$stopep" ] && [ $(( t + lim )) -gt "$stopep" ]; then hold "$id" stop_at stop_at; continue; fi   # no job starts inside its own limit of STOP_AT
    hol=0
    if [ $first = 1 ]; then
      first=0; head=$id
      [ "$(cut -d' ' -f1 "$RUN/head" 2>/dev/null)" = "$id" ] || echo "$id $t" > "$RUN/head"
      [ $(( t - $(cut -d' ' -f2 "$RUN/head") )) -ge "$HOL_S" ] && hol=1
    fi
    cls=$(jclass "$id"); cost=$(job_cost "$id"); wt=$(jreq "$id" wt); lane=""; ok=1; why=""; wkey=""
    # (a) the class has room
    case $cls in
      shoot) for l in $LANES; do case " $rlanes " in *" $l "*) ;; *) lane=$l; break;; esac; done; [ -n "$lane" ] || { ok=0; why="lane"; wkey=lane; };;
      tsc) [ $ntsc -lt "$TSC_MAX" ] || { ok=0; why="cap tsc"; wkey=$why; };;
      build) [ $nbuild -lt "$BUILD_MAX" ] || { ok=0; why="cap build"; wkey=$why; };;
      cargo) [ $nrun -eq 0 ] || { ok=0; why="cargo"; wkey=cargo; };;      # D-1019: cargo starts only when nothing else runs
    esac
    [ $ncargo -eq 0 ] || { ok=0; why="cargo"; wkey=cargo; }               # ... and while it runs nothing else starts
    # (b) no job of the same worktree runs (--wt lanes: any lane worktree with no running job)
    if [ "$wt" = lanes ]; then
      if [ -z "$LANE_WTS" ]; then end_job_q "$id" 2 "no LANE_WTS in conf"; continue; fi
      w=""; for l in $LANE_WTS; do l=$(cygpath -u "$l"); case "$rwts" in *"|$l|"*) ;; *) w=$l; break;; esac; done
      if [ -z "$w" ]; then ok=0; [ -n "$why" ] || { why="wt lanes"; wkey=$why; }; fi
      wt=$w
    else
      case "$rwts" in *"|$wt|"*) ok=0; [ -n "$why" ] || { why="wt $wt"; wkey=$why; };; esac
    fi
    # (c) memory; with nothing running the oldest queued job starts whatever it says
    if [ $ok = 1 ] && ! { [ $nrun -eq 0 ] && [ "$id" = "$head" ]; }; then
      [ $(( avail - recent - RESERVE )) -ge "$cost" ] || { ok=0; why="mem avail=$avail recent=$recent reserve=$RESERVE cost=$cost"; wkey=mem; }
    fi
    if [ $ok = 1 ]; then
      start_job "$id" "$lane" "$wt"
      nrun=$((nrun+1)); rwts="$rwts|$wt|"; rlanes="$rlanes $lane"; recent=$(( recent + cost ))
      case $cls in tsc) ntsc=$((ntsc+1));; build) nbuild=$((nbuild+1));; cargo) ncargo=$((ncargo+1));; esac
    else
      [ -n "$why" ] && hold "$id" "$wkey" "$why"
      if [ $hol = 1 ]; then                       # the head has waited HOL_S: nothing behind it starts
        for id in $(jids); do [ "$(jget "$id" state)" = queued ] && [ "$id" != "$head" ] && hold "$id" "hol $head" "hol $head"; done
        break
      fi
    fi
  done
  return 0
}

do_run() {
  if runner_alive && [ "$(cat "$RUN/runner.pid")" != "$(winpid $$)" ]; then echo "a runner is already alive (pid $(cat "$RUN/runner.pid"))" >&2; return 1; fi
  winpid $$ > "$RUN/runner.pid"; rm -f "$RUN/stop" "$RUN/head"
  trap 'rm -f "$RUN/runner.pid" "$RUN/stop"; log "runner exits"' EXIT
  log "runner starts (pid $(cat "$RUN/runner.pid"), run folder $RUN)"
  # a job still `running` when a runner starts was left by a runner that died: orphan, whatever its process does (D-987 line 5)
  local oid; for oid in $(jids); do if [ "$(jget "$oid" state)" = running ]; then log "orphan at start: $oid"; echo "[runner] orphaned job" >> "$(jd "$oid")/log"; end_job "$oid" failed 137; fi; done
  while pass; do sleep "$TICK"; done
}

do_start() {
  if runner_alive; then echo "runner already alive (pid $(cat "$RUN/runner.pid"))"; return 1; fi
  rm -f "$RUN/stop"
  SR_RUN_DIR=$RUN nohup bash "$SELF" run >> "$RUN/runner.log" 2>&1 &
  local i; for i in $(seq 1 20); do runner_alive && { echo "runner started (pid $(cat "$RUN/runner.pid"))"; return 0; }; sleep 0.5; done
  echo "runner did not start; see $RUN/runner.log" >&2; return 1
}

do_stop() {
  runner_alive || { echo "runner is not running"; rm -f "$RUN/stop"; return 0; }
  local mode=soft; [ "${1:-}" = --now ] && mode=now; echo "$mode" > "$RUN/stop"
  local i; for i in $(seq 1 60); do runner_alive || { echo "runner stopped"; return 0; }; sleep 1; done
  echo "runner still draining (mode $mode); running jobs finish first" >&2; return 1
}

do_prepare() {
  local wt x; wt=$(cygpath -u "${1:?usage: runner.sh prepare <base worktree>}")
  [ -d "$wt/e2e" ] || { echo "no $wt/e2e" >&2; return 1; }
  echo "[prepare] build $wt"; (cd "$wt" && VITE_SR_E2E=1 VITE_ACTIVITIES_ENABLED=true node scripts/build-web.mjs) || return 1
  mkdir -p "$BASE_DIST"; for x in "$BASE_DIST"/* "$BASE_DIST"/.[!.]*; do [ -e "$x" ] && safe_rm "$x"; done
  cp -r "$wt/apps/web/dist/." "$BASE_DIST/" || return 1
  echo "[prepare] seed template $TEMPLATE_DB from $BASE_DIST"
  (cd "$wt" && bash "$D/world.sh" seed "$TEMPLATE_DB" "$(cygpath -m "$BASE_DIST")" $((PORT_BASE + 9))) || return 1
  echo "[prepare] done"
}

case ${1:-} in
  run) do_run;;
  exec) do_exec "$2";;
  start) do_start;;
  stop) do_stop "${2:-}";;
  status) bash "$D/job.sh" status;;
  prepare) do_prepare "$2";;
  *) sed -n 2,6p "$0"; exit 2;;
esac
