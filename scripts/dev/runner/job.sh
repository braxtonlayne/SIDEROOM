#!/usr/bin/env bash
# job.sh (wave 104, D-986): what a builder uses. Jobs are folders under $SR_RUN_DIR/jobs (default cli-agents/run); the runner (runner.sh) runs them.
#   job.sh submit <name> tsc | build | vitest -- <files> | shoot base|after [--fresh] [--dist <dir>] [--bin <folder>] [--env K=V]... [--server-env K=V]... -- <playwright args>
#   job.sh submit <name> cargo -- build|test|clippy|fmt|run [cargo arguments]   (D-1019: runs alone, in the job's worktree, target folder $W/<WAVE>-<name>-target, debug info off, --offline added where cargo takes it)
#   shoot --bin <folder>: serve this job with the sr-server.exe and sr-cli.exe of <folder> (under SIDEROOM_wt/, never p80-base-bin or p92-base-bin) instead of the wave's CELL_BIN
#   job.sh submit <name> test <mb> <seconds> [rc]      (a dummy: holds <mb> of gate cost, sleeps, exits rc)
#         every submit takes [--wt <worktree>|lanes]; default worktree $W/<WAVE>-<name>; prints the job id
#   job.sh wait <id> [seconds=100]   0 done | 1 failed | 2 killed, cancelled or unknown id | 75 still queued or running (100: a builder's tool call ends at 120 s; always pass it, never wrap wait in timeout)
#   job.sh status | cancel <id> | log <id> (prints the log's path)
. "$(dirname "$(readlink -f "$0")")/runlib.sh"
load_conf; init_run

next_ticket() { # taken under run/seq.lock (a mkdir lock held for milliseconds)
  local i n
  for i in $(seq 1 100); do
    if mkdir "$RUN/seq.lock" 2>/dev/null; then
      n=$(( $(cat "$RUN/seq" 2>/dev/null || echo 0) + 1 )); echo "$n" > "$RUN/seq"; rmdir "$RUN/seq.lock"
      printf '%04d' "$n"; return 0
    fi
    sleep 0.1
  done
  echo "could not take $RUN/seq.lock" >&2; return 1
}

do_submit() {
  local name=${1:-}; local cls=${2:-}; shift 2 2>/dev/null
  [ -n "$name" ] && [ -n "$cls" ] || { echo "usage: job.sh submit <name> <class> ..." >&2; return 2; }
  echo "$name" | grep -qE '^[A-Za-z0-9_-]+$' || { echo "name must be letters, digits, underscore or hyphen" >&2; return 2; }
  local wt="" phase="" fresh=0 dist="" bin="" mb="" sec="" rc=0 r; local -a envs=() senvs=() args=() pos=()
  case $cls in tsc|build|vitest|shoot|test|cargo) ;; *) echo "unknown class $cls" >&2; return 2;; esac
  while [ $# -gt 0 ]; do
    case $1 in
      --wt) wt=$2; shift 2;;
      --fresh) fresh=1; shift;;
      --dist) dist=$2; shift 2;;
      --bin) bin=$2; shift 2;;
      --env) echo "$2" | grep -qE '^(SR_P[0-9]+_[A-Za-z0-9_]*|SR_E2E_SWEEP|SR_SWEEP_[A-Za-z0-9_]*)=' || { echo "--env name refused: $2 (only SR_P<digits>_*, SR_E2E_SWEEP, SR_SWEEP_*)" >&2; return 2; }
             envs+=("$2"); shift 2;;
      --server-env) echo "$2" | grep -qE '^(SR_VOICE__ENABLED|SR_PAYMENTS_RATE_SOURCES|SR_PAYMENTS_ESPLORA_URL)=' || { echo "--server-env name refused: $2" >&2; return 2; }
             senvs+=("$2"); shift 2;;
      --) shift; args=("$@"); break;;
      *) pos+=("$1"); shift;;
    esac
  done
  case $cls in
    shoot) if [ -n "$bin" ]; then bin=$(cygpath -u "$bin" 2>/dev/null); r=$(bin_check "$bin") || { echo "--bin refused: $r" >&2; return 2; }; bin=${bin%/}; fi
           phase=${pos[0]:-}; [ "$phase" = base ] || [ "$phase" = after ] || { echo "shoot needs base or after" >&2; return 2; }
           [ ${#args[@]} -gt 0 ] || { echo "shoot needs -- <playwright args>" >&2; return 2; };;
    test) mb=${pos[0]:-}; sec=${pos[1]:-}; rc=${pos[2]:-0}
          echo "$mb$sec" | grep -qE '^[0-9]+$' && [ -n "$mb" ] && [ -n "$sec" ] || { echo "test needs <mb> <seconds> [rc]" >&2; return 2; };;
    vitest) [ ${#args[@]} -gt 0 ] || { echo "vitest needs -- <files>" >&2; return 2; };;
    cargo) case ${args[0]:-} in build|test|clippy|fmt|run) ;; *) echo "cargo needs -- build|test|clippy|fmt|run [arguments]" >&2; return 2;; esac;;
  esac
  [ -n "$wt" ] || wt=$W/$WAVE-$name
  [ "$wt" = lanes ] || wt=$(cygpath -u "$wt")
  local t id; t=$(next_ticket) || return 1; id=$t-$name-$cls
  mkdir -p "$(jd "$id")"
  { echo "name=$name"; echo "class=$cls"; echo "wt=$wt"; echo "phase=$phase"; echo "fresh=$fresh"; echo "dist=$dist"; echo "bin=$bin"
    echo "mb=$mb"; echo "sec=$sec"; echo "rc=$rc"
    for x in "${envs[@]}"; do echo "env=$x"; done; for x in "${senvs[@]}"; do echo "senv=$x"; done
    for x in "${args[@]}"; do echo "arg=$x"; done; } > "$(jd "$id")/req"
  jset "$id" submitted "$(now)"
  jset "$id" state queued      # last
  echo "$id"
}

do_wait() {
  local id=$1 secs=${2:-100} end s; end=$(( $(now) + secs ))
  [ -d "$(jd "$id")" ] || { echo "unknown job $id" >&2; return 2; }
  while :; do
    s=$(jget "$id" state)
    case $s in done) return 0;; failed) return 1;; killed|cancelled) return 2;; esac
    [ "$(now)" -ge "$end" ] && { echo "$id still $s" >&2; return 75; }
    sleep 1
  done
}

do_status() {
  local a; a=$(avail_mb)
  if runner_alive; then echo "runner: alive (pid $(cat "$RUN/runner.pid"))"; else echo "runner: NOT running"; fi
  echo "available: ${a:-?} MB$([ -f "$RUN/fake-avail" ] && echo ' (fake-avail)')   run folder: $RUN"
  local id s t; t=$(now)
  echo "running:"; for id in $(jids); do [ "$(jget "$id" state)" = running ] && echo "  $id lane=$(jget "$id" lane) age=$(( t - $(jget "$id" started) ))s"; done
  echo "queued:"; for id in $(jids); do [ "$(jget "$id" state)" = queued ] && echo "  $id waiting=$(( t - $(jget "$id" submitted) ))s"; done
  echo "last holds (why a job waited):"; grep ' hold ' "$RUN/runner.log" 2>/dev/null | tail -5 | sed 's/^/  /'
  return 0
}

do_cancel() {
  local id=$1 s; [ -d "$(jd "$id")" ] || { echo "unknown job $id" >&2; return 2; }
  s=$(jget "$id" state)
  case $s in queued|running) ;; *) echo "$id is $s: nothing to cancel"; return 0;; esac
  if runner_alive; then
    touch "$(jd "$id")/cancel"
    local i; for i in $(seq 1 25); do [ "$(jget "$id" state)" = cancelled ] && { echo "$id cancelled"; return 0; }; sleep 1; done
    echo "runner has not acted on the cancel yet" >&2; return 1
  fi
  end_job "$id" cancelled 143; echo "$id cancelled"
}

case ${1:-} in
  submit) shift; do_submit "$@";;
  wait) shift; do_wait "$@";;
  status) do_status;;
  cancel) do_cancel "$2";;
  log) echo "$(jd "$2")/log";;
  *) sed -n 2,9p "$0"; exit 2;;
esac
