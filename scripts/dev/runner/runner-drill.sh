#!/usr/bin/env bash
# runner-drill.sh (wave 104, D-987): the acceptance drill for the job runner (job.sh, runner.sh, lane.sh), one function
# per "must hold" line of the brief's runner contract. NOT the runner's author: it only calls the documented commands.
#
#   bash runner-drill.sh                 # the dummy checks: 1 2 3 4 5 6 7 9 10 (no heavy.sh slot needed)
#   bash runner-drill.sh 2 5             # only those numbers
#   bash runner-drill.sh 8               # the leftovers check (databases, ports, processes) on the state as it is
#   bash runner-drill.sh real            # part 3, REAL cells: two shoot jobs at once, checks 3 (lanes), 8, 7, and the
#                                        # memory figures. Needs a heavy.sh slot in the name "drill" (read from status).
#
# Output: one line per check, `PASS <n> <what>` or `FAIL <n> <what>: <what was seen>`, and a last line
# `drill: <passed>/<run>`. Always works in its own run folder (SR_RUN_DIR below) with its own conf; never the real queue.
# A check leaves nothing behind: its runner is stopped and its jobs are archived to $RUN/evidence/<check>-<time>/ and
# removed, whether it passed or failed (also on ^C / error: trap). The only deletions go through drill_rm, which
# refuses a path that is empty or not under the run folder.
W=/c/Users/braxt/Desktop/SIDEROOM_wt
D=${DRILL_SCRIPTS:-$(cd "$(dirname "$(readlink -f "$0")")" && pwd)}     # wave 106: the folder this drill is in (a clone tests itself)
RUN=$D/run-drill
export SR_RUN_DIR=$(cygpath -m "$RUN")
WTS=$RUN/wt
WTW=C:/Users/braxt/Desktop/SIDEROOM_wt          # the same worktree root in the C:/ spelling, for conf values
PSQL="/c/Program Files/PostgreSQL/17/bin/psql.exe"
JOBSH="bash $D/job.sh"; RUNSH="bash $D/runner.sh"
PASSED=0; RAN=0; F=""

# ---------- small helpers ----------
now() { date +%s; }
drill_rm() { # the drill's own guarded delete: a non-empty path strictly under the run folder
  local p=$1
  case $p in "$RUN"/?*) ;; *) echo "drill_rm refuses '$p'" >&2; return 1 ;; esac
  rm -rf "$p"
}
bad() { F="${F:+$F; }$*"; }
finish() { # finish <n> <what>
  RAN=$((RAN + 1))
  if [ -z "$F" ]; then PASSED=$((PASSED + 1)); echo "PASS $1 $2"; else echo "FAIL $1 $2: $F"; fi
  F=""
}
pid_alive() { [ -n "$1" ] && tasklist //FI "PID eq $1" //NH 2>/dev/null | grep -qw "$1"; }
winpid() { cat "/proc/$1/winpid" 2>/dev/null || echo "$1"; }
kill_tree() { [ -n "$1" ] && taskkill //PID "$1" //T //F >/dev/null 2>&1; }
jf() { tr -d '\r\n ' < "$RUN/jobs/$1/$2" 2>/dev/null; }                 # a job's file, trimmed
ep() { # the job's time file as epoch seconds ("" if missing); accepts epoch, epoch ms or a date text
  [ -f "$RUN/jobs/$1/$2" ] || return
  local c; c=$(tr -d '\r\n' < "$RUN/jobs/$1/$2" 2>/dev/null) || return
  c=${c#"${c%%[![:space:]]*}"}
  [ -n "$c" ] || return
  if [[ $c =~ ^[0-9]+$ ]]; then [ ${#c} -ge 12 ] && c=$((c / 1000)); echo "$c"; else date -d "$c" +%s 2>/dev/null; fi
}
state() { jf "$1" state; }
is_end() { case $1 in done|failed|killed|cancelled) return 0 ;; esac; return 1; }
wait_state() { # wait_state <id> <regex> <timeout s>: 0 when state matches in time
  local id=$1 re=$2 t=$(( $(now) + $3 ))
  while [ "$(now)" -lt $t ]; do state "$id" | grep -Eq "^($re)$" && return 0; sleep 1; done
  state "$id" | grep -Eq "^($re)$"
}
wait_end() { local id=$1 t=$(( $(now) + $2 )); while [ "$(now)" -lt $t ]; do is_end "$(state "$id")" && return 0; sleep 1; done; is_end "$(state "$id")"; }
fake() { echo "$1" > "$RUN/fake-avail"; }

write_conf() { # write_conf [KEY=VALUE ...]: the drill's conf; the extras are appended (the later assignment wins)
  mkdir -p "$RUN"
  {
    echo "WAVE=p104"
    echo "CELL_BIN=$WTW/p104-cell-bin"
    echo "BASE_DIST=$WTW/p104-base-dist"
    echo "TEMPLATE_DB=sr_p104_world"
    echo "DB_PREFIX=sr_p104_d"
    echo 'LANES="1 2"'
    echo "PORT_BASE=8850"
    echo "METRICS_BASE=9550"
    echo "COST_SHOOT=900 COST_BUILD=1500 COST_TSC=1900 COST_VITEST=1200 RESERVE=700"
    echo "RECENT_S=5 HOL_S=20 JOB_LIMIT_S=20 TSC_MAX=1 BUILD_MAX=1"
    echo "LANE_WTS=\"$WTW/p104-lane1 $WTW/p104-lane2\""
    echo 'STOP_AT=""'
    for kv in "$@"; do echo "$kv"; done
  } > "$RUN/conf"
}

start_runner() { # start the runner (background) and wait for its pid file
  $RUNSH start >> "$RUN/drill-runner-start.log" 2>&1
  local t=$(( $(now) + 15 ))
  while [ "$(now)" -lt $t ]; do [ -s "$RUN/runner.pid" ] && pid_alive "$(tr -d '\r\n ' < "$RUN/runner.pid")" && return 0; sleep 1; done
  return 1
}

teardown() { # stop the runner and every job process, archive the jobs, leave the run folder empty of state
  local p d id
  [ -s "$RUN/runner.pid" ] && kill_tree "$(tr -d '\r\n ' < "$RUN/runner.pid")"
  for d in "$RUN"/jobs/*/; do
    [ -d "$d" ] || continue
    if [ -s "$d/pid" ]; then kill_tree "$(tr -d '\r\n ' < "$d/pid")"; fi
  done
  sleep 1
  if [ -d "$RUN/jobs" ] && [ -n "$(ls -A "$RUN/jobs" 2>/dev/null)" ]; then
    local ev="$RUN/evidence/${CUR:-x}-$(date +%H%M%S)"; mkdir -p "$ev"
    cp -r "$RUN/jobs/." "$ev/" 2>/dev/null; cp "$RUN/runner.log" "$ev/runner.log" 2>/dev/null; cp "$RUN/conf" "$ev/conf" 2>/dev/null
    for d in "$RUN"/jobs/*/; do [ -d "$d" ] && drill_rm "${d%/}"; done
  fi
  rm -f "$RUN/runner.pid" "$RUN/stop" "$RUN/fake-avail" "$RUN/seq"
  [ -d "$RUN/seq.lock" ] && drill_rm "$RUN/seq.lock"
  for d in "$RUN"/lanes/*/; do [ -d "$d" ] && [ -s "$d/pid" ] && kill_tree "$(winpid "$(tr -d '\r\n ' < "$d/pid")")"; done
}
setup() { CUR=$1; shift; teardown; write_conf "$@"; mkdir -p "$WTS"; }

submit_test() { # submit_test <name> <mb> <seconds> <rc> <wt-name>: prints the job id (the runner decides the spelling)
  local name=$1 mb=$2 sec=$3 rc=$4 wt=$(cygpath -m "$WTS")/$5 id
  mkdir -p "$WTS/$5"
  id=$($JOBSH submit "$name" test "$mb" "$sec" "$rc" --wt "$wt" 2>>"$RUN/drill-submit.err" | tail -1 | tr -d '\r')
  if ! [[ $id =~ ^[0-9]{4}- ]]; then   # the other order of the option, in case the runner takes it first
    id=$($JOBSH submit "$name" --wt "$wt" test "$mb" "$sec" "$rc" 2>>"$RUN/drill-submit.err" | tail -1 | tr -d '\r')
  fi
  [[ $id =~ ^[0-9]{4}- ]] || { bad "submit of $name printed '$id' instead of a job id (see run-drill/drill-submit.err)"; echo "NOID"; return 1; }
  echo "$id"
}
need_runner() { start_runner || { bad "runner did not start (runner.pid missing or dead)"; return 1; }; }

# ---------- 0 a clone runs itself (wave 106, item 0) ----------
c0() {
  local live="$W/cli-agents/run/runner.log" id procs lib
  lib=$(bash -c ". '$D/runlib.sh'; echo \$D" 2>&1)
  [ "$lib" = "$D" ] || bad "runlib.sh sets D='$lib', wanted its own folder $D"
  grep -q 'cli-agents' <(grep -hE '^(D|L)=' "$D/runlib.sh" "$D/world.sh" "$D/heavy.sh" "$D/tc.sh") && bad "a script still names cli-agents in its D= or L= line"
  setup 0 'JOB_LIMIT_S=60'; fake 20000
  need_runner || { teardown; finish 0 "own folder"; return; }
  id=$(submit_test own 50 8 0 c0-a)
  wait_state "$id" running 15 || bad "the test job did not start (state $(state "$id"))"
  procs=$(powershell -NoProfile -Command 'Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -match "runner\.sh (run|exec)" -and $_.CommandLine -notmatch "Get-CimInstance" } | ForEach-Object { $_.CommandLine }' | tr -d '\r')
  echo "$procs" | grep -q "$D/runner.sh exec $id" || bad "no 'exec $id' process runs from $D; seen: $(echo "$procs" | grep "exec $id" | head -2 | tr '\n' ' ')"
  [ "$D" = "$W/cli-agents" ] || { echo "$procs" | grep "exec $id" | grep -q "$W/cli-agents/runner.sh" && bad "the job ran the LIVE runner.sh"; }
  wait_end "$id" 30
  # the live runner logs jobs of the other builders meanwhile: look only for this drill's own job and run folder
  [ "$D" = "$W/cli-agents" ] || ! grep -qE "own-test|run-drill" "$live" 2>/dev/null || bad "$live names this drill's job or run folder"
  teardown; finish 0 "the scripts run from their own folder ($(basename "$D")): runlib's D, the exec process of a test job, the live runner.log has no line of it"
}

# ---------- 1 order ----------
c1() {
  setup 1 'LANES="1 2 3 4 5"' 'JOB_LIMIT_S=60'
  fake 20000; need_runner || { teardown; finish 1 "jobs start in ticket order"; return; }
  local ids=() id i
  for i in 1 2 3 4 5; do ids+=("$(submit_test "o$i" 50 3 0 "c1-$i")"); done
  for id in "${ids[@]}"; do wait_end "$id" 60 || bad "$id did not end in 60 s (state $(state "$id"))"; done
  [ "$(printf '%s\n' "${ids[@]}" | sort | tr '\n' ' ')" = "$(printf '%s\n' "${ids[@]}" | tr '\n' ' ')" ] || bad "tickets not ascending: ${ids[*]}"
  local prev=0 s
  for id in "${ids[@]}"; do
    s=$(ep "$id" started); [ -n "$s" ] || { bad "$id has no started time"; continue; }
    [ "$s" -ge "$prev" ] || bad "$id started at $s, before its predecessor ($prev)"; prev=$s
    [ "$(state "$id")" = done ] || bad "$id ended $(state "$id") rc $(jf "$id" rc), wanted done"
  done
  # strict order: memory for one job at a time (900 each, fake 1700: fits once; RECENT_S 5 holds the next back)
  teardown; setup 1 'JOB_LIMIT_S=60'; fake 1700; need_runner || { teardown; finish 1 "jobs start in ticket order"; return; }
  ids=(); for i in 1 2 3 4; do ids+=("$(submit_test "s$i" 900 2 0 "c1s-$i")"); done
  for id in "${ids[@]}"; do wait_end "$id" 60 || bad "$id did not end in 60 s (state $(state "$id"))"; done
  prev=0
  for id in "${ids[@]}"; do
    s=$(ep "$id" started); [ -n "$s" ] || continue
    [ "$s" -gt "$prev" ] || bad "serialised: $id started at $s, not after its predecessor ($prev)"; prev=$s
  done
  teardown; finish 1 "jobs start in ticket order (5 at once, then 4 held to one at a time)"
}

# ---------- 2 the gate ----------
c2() {
  local a b r h s1 s2 s3 t0
  # 2a: fake 2000, cost 900 (RESERVE 700): one fits; the next waits while the first is "recent", then starts
  setup 2 'JOB_LIMIT_S=60' 'RECENT_S=5' 'HOL_S=20'; fake 2000
  need_runner || { teardown; finish 2 "the gate"; return; }
  a=$(submit_test g_a 900 20 0 c2a-1); b=$(submit_test g_b 900 4 0 c2a-2)
  wait_state "$a" running 15 || bad "2a: first job did not start (state $(state "$a"))"
  sleep 2
  [ "$(state "$b")" = queued ] || bad "2a: second job is '$(state "$b")' 2 s after the first started (wanted queued while the first is recent)"
  wait_state "$b" 'running|done' 20 || bad "2a: second job never started after the first was no longer recent (state $(state "$b"))"
  local sa sb ea; sa=$(ep "$a" started); sb=$(ep "$b" started); ea=$(ep "$a" ended)
  [ -n "$sa" ] && [ -n "$sb" ] && [ $((sb - sa)) -ge 4 ] || bad "2a: second started $((${sb:-0} - ${sa:-0})) s after the first, wanted at least RECENT_S-1 = 4"
  [ -z "$ea" ] || [ "$sb" -lt "$ea" ] || bad "2a: second started after the first ended: it should have started with the first still running (fake 2000 holds both)"
  # 2b: head 1900 at fake 1500 is passed by a 300 job; after HOL_S a second 300 job does not pass
  teardown; setup 2 'JOB_LIMIT_S=80' 'RECENT_S=5' 'HOL_S=20'; fake 1500
  need_runner || { teardown; finish 2 "the gate"; return; }
  r=$(submit_test g_run 100 60 0 c2b-run)
  wait_state "$r" running 15 || bad "2b: the keeper job did not start (state $(state "$r"))"
  h=$(submit_test g_head 1900 3 0 c2b-head); s1=$(submit_test g_s1 300 3 0 c2b-s1); t0=$(now)
  wait_state "$s1" 'running|done' 20 || bad "2b: a 300 job behind a 1900 head at fake 1500 did not pass it (s1 '$(state "$s1")', head '$(state "$h")')"
  [ "$(state "$h")" = queued ] || bad "2b: the 1900 head is '$(state "$h")' at fake 1500 with a job running (wanted queued)"
  while [ $(( $(now) - t0 )) -lt $((20 + 4)) ]; do sleep 1; done          # the head has now been oldest for HOL_S + a pass
  s2=$(submit_test g_s2 300 3 0 c2b-s2); sleep 8
  [ "$(state "$s2")" = queued ] || bad "2b: after HOL_S a second 300 job was '$(state "$s2")' (wanted queued: nothing passes the head)"
  [ "$(state "$h")" = queued ] || bad "2b: the head is '$(state "$h")' while fake is still 1500 and a job runs (wanted queued)"
  fake 3000                                                          # now the head fits; then the 300 job behind it
  wait_state "$h" 'running|done' 20 || bad "2b: the head did not start when memory was raised to 3000 (state $(state "$h"))"
  wait_state "$s2" 'running|done' 30 || bad "2b: the 300 job behind the head did not start after it (state $(state "$s2"))"
  local sh ss; sh=$(ep "$h" started); ss=$(ep "$s2" started)
  [ -n "$sh" ] && [ -n "$ss" ] && [ "$ss" -ge "$sh" ] || bad "2b: after the head, s2 started at ${ss:-?} before the head at ${sh:-?}"
  # 2c: fake 100, nothing running: the head starts anyway
  teardown; setup 2 'JOB_LIMIT_S=60' 'RECENT_S=5' 'HOL_S=20'; fake 100
  need_runner || { teardown; finish 2 "the gate"; return; }
  s3=$(submit_test g_alone 900 3 0 c2c-1)
  wait_state "$s3" 'running|done' 20 || bad "2c: with fake 100 and nothing running the head (cost 900) did not start (state $(state "$s3"))"
  teardown; finish 2 "the gate (fits/waits, small passes big, HOL_S blocks, empty machine starts the head)"
}

# ---------- 3 one per worktree (dummy half; the lane half is in part 3) ----------
c3() {
  setup 3 'JOB_LIMIT_S=60'; fake 20000
  need_runner || { teardown; finish 3 "one job per worktree"; return; }
  local x y z
  x=$(submit_test same_1 100 6 0 c3-same); y=$(submit_test same_2 100 6 0 c3-same); z=$(submit_test other 100 6 0 c3-other)
  for id in "$x" "$y" "$z"; do wait_end "$id" 60 || bad "$id did not end in 60 s (state $(state "$id"))"; done
  local sy ex sz; sy=$(ep "$y" started); ex=$(ep "$x" ended); sz=$(ep "$z" started)
  [ -n "$sy" ] && [ -n "$ex" ] && [ "$sy" -ge "$ex" ] || bad "two jobs of one worktree overlapped: second started ${sy:-?}, first ended ${ex:-?}"
  [ -n "$sz" ] && [ -n "$ex" ] && [ "$sz" -lt "$ex" ] || bad "a job of ANOTHER worktree waited for the first (started ${sz:-?}, first ended ${ex:-?}): the rule is per worktree"
  teardown; finish 3 "one running job per worktree (dummy half; 'never more shoot jobs than lanes' is checked in part 3)"
}

# ---------- 4 the limit ----------
c4() {
  setup 4 'JOB_LIMIT_S=20'; fake 20000
  need_runner || { teardown; finish 4 "the limit"; return; }
  local id p=""; id=$(submit_test slow 50 117 0 c4-1)
  wait_state "$id" running 15 || bad "job did not start (state $(state "$id"))"
  p=$(jf "$id" pid)
  wait_state "$id" killed 50 || bad "after JOB_LIMIT_S=20 the job is '$(state "$id")' (rc $(jf "$id" rc)), wanted killed"
  [ -n "$p" ] || p=$(jf "$id" pid)
  [ -n "$p" ] || bad "no pid recorded in the job folder"
  sleep 2
  pid_alive "$p" && bad "the job's recorded PID $p is still alive after the kill"
  local left; left=$(powershell -NoProfile -Command 'Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -match "sleep 117" -and $_.CommandLine -notmatch "Get-CimInstance" } | ForEach-Object { "$($_.ProcessId) $($_.Name)" }' | tr -d '\r')
  [ -z "$left" ] || { bad "a process of the dummy ('sleep 117') is left: $left"; echo "$left" | while read -r q _; do kill_tree "$q"; done; }
  teardown; finish 4 "a job past JOB_LIMIT_S ends killed and no process of it is left"
}

# ---------- 5 restart ----------
c5() {
  setup 5 'JOB_LIMIT_S=120'; fake 20000
  need_runner || { teardown; finish 5 "restart"; return; }
  local j1 j2 j3 rp jp
  j1=$(submit_test r1 100 40 0 c5-same)
  wait_state "$j1" running 15 || bad "the first job did not start (state $(state "$j1"))"
  j2=$(submit_test r2 100 3 0 c5-same); j3=$(submit_test r3 100 3 0 c5-same); sleep 4
  [ "$(state "$j2")" = queued ] && [ "$(state "$j3")" = queued ] || bad "the two later jobs are '$(state "$j2")' / '$(state "$j3")' (wanted queued behind a running job of the same worktree)"
  rp=$(tr -d '\r\n ' < "$RUN/runner.pid"); jp=$(jf "$j1" pid)
  taskkill //PID "$rp" //T //F >/dev/null 2>&1; sleep 3
  pid_alive "$rp" && bad "the runner (PID $rp) is still alive after taskkill"
  $RUNSH start >> "$RUN/drill-runner-start.log" 2>&1; sleep 3
  wait_state "$j1" failed 40 || bad "the orphan is '$(state "$j1")' after the restart (wanted failed); job PID $jp alive: $(pid_alive "$jp" && echo yes || echo no)"
  [ "$(jf "$j1" rc)" = 137 ] || bad "the orphan's rc is '$(jf "$j1" rc)', wanted 137"
  wait_end "$j2" 40 || bad "second job did not end after the restart (state $(state "$j2"))"
  wait_end "$j3" 40 || bad "third job did not end after the restart (state $(state "$j3"))"
  [ "$(state "$j2")" = done ] && [ "$(state "$j3")" = done ] || bad "after the restart r2 is '$(state "$j2")', r3 '$(state "$j3")' (wanted done, done)"
  local s2 s3 e2; s2=$(ep "$j2" started); s3=$(ep "$j3" started); e2=$(ep "$j2" ended)
  [ -n "$s2" ] && [ -n "$s3" ] && [ "$s3" -ge "$s2" ] || bad "the queue did not go on in order: r2 started ${s2:-?}, r3 ${s3:-?}"
  [ -z "$e2" ] || [ "$s3" -ge "$e2" ] || bad "r3 (same worktree) started $s3 before r2 ended $e2"
  pid_alive "$jp" && { bad "the orphan's process $jp is still alive"; kill_tree "$jp"; }
  teardown; finish 5 "restart after the runner is killed mid-job: orphan failed rc 137, the queue goes on in order"
}

# ---------- 6 wait ----------
timed_wait() { # timed_wait <id> <seconds>: prints "<exit> <elapsed>"
  local t0 rc; t0=$(now); $JOBSH wait "$1" "$2" >/dev/null 2>&1; rc=$?; echo "$rc $(( $(now) - t0 ))"
}
c6() {
  setup 6 'JOB_LIMIT_S=40'; fake 20000
  need_runner || { teardown; finish 6 "wait"; return; }
  local ok bad7 blk q slow r e
  ok=$(submit_test w_ok 50 2 0 c6-1); bad7=$(submit_test w_bad 50 2 7 c6-2)
  blk=$(submit_test w_blk 50 25 0 c6-3); q=$(submit_test w_q 50 2 0 c6-3)       # q waits behind blk (same worktree)
  slow=$(submit_test w_slow 50 25 0 c6-4)
  set -- $(timed_wait "$ok" 30);   r=$1; e=$2; [ "$r" = 0 ] || bad "wait on an rc 0 job exited $r (wanted 0)"; [ "$e" -le 35 ] || bad "wait (rc 0) took $e s inside 30+5"
  set -- $(timed_wait "$bad7" 30); r=$1; e=$2; [ "$r" = 1 ] || bad "wait on an rc 7 job exited $r (wanted 1; job state $(state "$bad7") rc $(jf "$bad7" rc))"; [ "$e" -le 35 ] || bad "wait (rc 7) took $e s inside 30+5"
  [ "$(jf "$bad7" rc)" = 7 ] || bad "the rc 7 job's rc file says '$(jf "$bad7" rc)'"
  [ "$(state "$q")" = queued ] || bad "cannot cancel: the queued job is '$(state "$q")' (wanted queued behind w-blk)"
  $JOBSH cancel "$q" >/dev/null 2>&1; sleep 1
  [ "$(state "$q")" = cancelled ] || bad "after cancel the job is '$(state "$q")' (wanted cancelled)"
  set -- $(timed_wait "$q" 10);    r=$1; e=$2; [ "$r" = 2 ] || bad "wait on a cancelled job exited $r (wanted 2)"; [ "$e" -le 15 ] || bad "wait (cancelled) took $e s inside 10+5"
  set -- $(timed_wait "9999-nosuch-test" 10); r=$1; e=$2; [ "$r" = 2 ] || bad "wait on an id that does not exist exited $r (wanted 2)"; [ "$e" -le 15 ] || bad "wait (unknown id) took $e s inside 10+5"
  wait_state "$slow" running 15 || bad "the 25 s job did not start (state $(state "$slow"))"
  set -- $(timed_wait "$slow" 5);  r=$1; e=$2; [ "$r" = 75 ] || bad "wait <id> 5 on a job still running exited $r (wanted 75; state $(state "$slow"))"; [ "$e" -le 10 ] || bad "wait 5 took $e s (wanted at most 5+5)"
  teardown; finish 6 "wait: 0 done, 1 failed, 2 cancelled and unknown id, 75 still running; each inside its seconds + 5"
}

# ---------- 7 password ----------
c7() {
  local pwf pw hits
  pwf="$(cygpath -u "$LOCALAPPDATA")/sideroom/sr_pg_pw.txt"
  [ -s "$pwf" ] || { bad "cannot read the password file"; finish 7 "the password is in no file of the run folder"; return; }
  pw=$(tr -d '\r\n' < "$pwf"); [ -n "$pw" ] || { bad "empty password"; finish 7 "the password is in no file of the run folder"; return; }
  # the pattern goes in through a file descriptor (process substitution), never on a command line
  hits=$(grep -rlF -f <(printf '%s' "$pw") "$RUN" 2>/dev/null | sed "s|^$RUN/||" | tr '\n' ' ')
  [ -z "$hits" ] || bad "the password is in: $hits"
  # also in the folders the runner makes outside run-drill: lane folders / cell logs of the drill's databases
  pw=""
  local filt; filt=$(grep -l '<pw>' "$D/job.sh" "$D/runner.sh" "$D/runlib.sh" 2>/dev/null | tr '\n' ' ')
  [ -n "$filt" ] || bad "no '<pw>' log filter found in job.sh, runner.sh or runlib.sh"
  finish 7 "the password is in no file under run-drill (logs filtered with <pw>; filter text in: ${filt:-none})"
}

# ---------- 9 STOP_AT ----------
c9() {
  local at id
  at=$(date -d '+1 minute' +%H:%M)
  setup 9 'JOB_LIMIT_S=120' "STOP_AT=$at"; fake 20000
  need_runner || { teardown; finish 9 "STOP_AT"; return; }
  id=$(submit_test late 50 3 0 c9-1); sleep 12
  [ "$(state "$id")" = queued ] || bad "with STOP_AT=$at, JOB_LIMIT_S=120 a job started (state $(state "$id")): nothing may start later than JOB_LIMIT_S before STOP_AT"
  teardown
  # control: the same job with no STOP_AT starts
  setup 9 'JOB_LIMIT_S=120' 'STOP_AT=""'; fake 20000
  if need_runner; then
    id=$(submit_test early 50 3 0 c9-2)
    wait_state "$id" 'running|done' 15 || bad "control: without STOP_AT the job did not start (state $(state "$id"))"
  fi
  teardown; finish 9 "STOP_AT is honoured (a queued job does not start inside JOB_LIMIT_S of it; control without STOP_AT starts)"
}

# ---------- 10 rm ----------
c10() {
  local files=("$D/job.sh" "$D/runner.sh" "$D/lane.sh" "$D/world.sh" "$D/cell.sh" "$D/runlib.sh") f extra src
  for f in "${files[@]}"; do   # a sourced helper file counts too
    [ -f "$f" ] || continue
    for src in $(grep -E '^[[:space:]]*(source|\.)[[:space:]]+' "$f" | sed -E 's/^[[:space:]]*(source|\.)[[:space:]]+//; s/["'"'"']//g'); do
      src=${src//\$D/$D}; src=${src//\$\{D\}/$D}; src=${src//\$W/$W}
      [ -f "$src" ] && files+=("$src")
    done
  done
  local fn="" fnfile="" hit n=0
  for f in "${files[@]}"; do
    [ -f "$f" ] || { bad "$(basename "$f") does not exist"; continue; }
    while IFS= read -r hit; do
      n=$((n + 1))
      # find which function (if any) the rm sits in: the nearest 'name() {' line above it
      local ln=${hit%%:*} owner
      owner=$(awk -v L="$ln" -v q="'" '
        function depth(line,  t,o,c) { t=line; gsub(/"[^"]*"/,"",t); gsub(q "[^" q "]*" q,"",t); gsub(/[$][{][^}]*[}]/,"",t); sub(/[[:space:]]+#.*$/,"",t); o=gsub(/[{]/,"{",t); c=gsub(/[}]/,"}",t); return o-c }
        !inf && /^[[:space:]]*(function[[:space:]]+)?[A-Za-z_][A-Za-z0-9_]*[[:space:]]*[(][)][[:space:]]*[{]/ { name=$0; sub(/^[[:space:]]*(function[[:space:]]+)?/,"",name); sub(/[[:space:]]*[(].*/,"",name); s=NR; d=depth($0); if (d<=0) { if (L>=s && L<=NR) { print name; exit } } else inf=1; next }
        inf { d+=depth($0); if (d<=0) { inf=0; if (L>=s && L<=NR) { print name; exit } } }' "$f")
      if [ -z "$owner" ]; then bad "$(basename "$f"):$ln has an rm -r outside any function: ${hit#*:}"; continue; fi
      if [ -z "$fn" ]; then fn=$owner; fnfile=$f; elif [ "$fn" != "$owner" ]; then bad "$(basename "$f"):$ln: rm -r in function '$owner', but the checking function is '$fn' ($(basename "$fnfile"))"; fi
    done < <(grep -nE '(^|[;&|(]|[[:space:]])rm[[:space:]]+(-[A-Za-z]*[rR][A-Za-z]*|--recursive)' "$f" | grep -vE '^[0-9]+:[[:space:]]*#')
  done
  [ $n -gt 0 ] || bad "found no 'rm -rf' at all in the runner's scripts: cannot check (are the scripts there?)"
  if [ -n "$fn" ]; then
    # the function's own text, from its line to the closing brace at the same indent
    local body; body=$(awk -v fn="$fn" -v q="'" '
      function depth(line,  t,o,c) { t=line; gsub(/"[^"]*"/,"",t); gsub(q "[^" q "]*" q,"",t); gsub(/[$][{][^}]*[}]/,"",t); sub(/[[:space:]]+#.*$/,"",t); o=gsub(/[{]/,"{",t); c=gsub(/[}]/,"}",t); return o-c }
      !p && $0 ~ ("^[[:space:]]*(function[[:space:]]+)?" fn "[[:space:]]*[(][)][[:space:]]*[{]") { p=1; print; d=depth($0); if (d<=0) exit; next }
      p { print; d+=depth($0); if (d<=0) exit }' "$fnfile")
    [ -n "$body" ] || bad "could not read the body of $fn"
    echo "$body" | grep -qE '/c/Users/braxt/Desktop/SIDEROOM_wt|[$]W|[$][{]W[}]' || bad "$fn does not mention the prefix (the literal or $W)"
    # behaviour: the function is called with bad and with good paths in a subshell where `rm` is a STUB that only logs.
    # Nothing is ever deleted by the probe (a faulty function must not be able to delete a real path). A body that
    # reaches rm another way (command rm, a backslash before rm, /bin/rm, env) cannot be probed safely: that is a FAIL to be read.
    if echo "$body" | grep -qE '(command[[:space:]]+rm|/rm[[:space:]]|env[[:space:]]+rm|xargs[[:space:]]+rm)' || echo "$body" | grep -qF "$(printf '\134')rm"; then
      bad "$fn reaches rm in a way the drill cannot stub (command rm, /bin/rm, a backslash before rm): not probed"
    else
      local res
      res=$(bash -c "
        rm() { echo \"RMCALL \$*\"; }
        W=$W; D=$D   # the globals the sourced library sets (the function may read them)
        $body
        for a in '' '/c/Users' '/c/Users/braxt' '/c/Users/braxt/Desktop' '/c/Users/braxt/Desktop/SIDEROOM_wt' '/c/Users/braxt/Desktop/SIDEROOM_wt/' '/c/Users/braxt/Desktop/SIDEROOM_wt/p104-x/../..' '/c/Windows/x/y'; do
          o=\$($fn \"\$a\" 2>&1); rc=\$?
          if echo \"\$o\" | grep -q RMCALL; then echo \"ACCEPTED '\$a'\"; else echo \"refused '\$a'\"; fi
        done
        o=\$($fn 2>&1); if echo \"\$o\" | grep -q RMCALL; then echo 'ACCEPTED (no argument)'; else echo 'refused (no argument)'; fi
        o=\$($fn '$RUN/scratch/probe' 2>&1); if echo \"\$o\" | grep -q RMCALL; then echo 'good path reached rm'; else echo 'GOOD-PATH-REFUSED'; fi
      " 2>&1)
      echo "$res" | grep "^ACCEPTED" | grep -vq '\.\.' && bad "$fn accepted a bad path: $(echo "$res" | grep '^ACCEPTED' | grep -v '\.\.' | tr '\n' ' ')"
      echo "$res" | grep -q "^ACCEPTED .*\.\." && echo "INFO 10 $fn accepts a path with '..' components (the contract does not ask for that; worth a look)"
      echo "$res" | grep -q "refused ''" || bad "$fn did not refuse an empty argument"
      echo "$res" | grep -q "refused '/c/Users'" || bad "$fn did not refuse /c/Users"
      echo "$res" | grep -q "good path reached rm" || bad "$fn refused a legitimate path under SIDEROOM_wt (probe output: $(echo "$res" | grep -i good | tr '\n' ' '))"
      mkdir -p "$RUN/scratch"; echo "$res" > "$RUN/scratch/rm-probe.out"
    fi
  fi
  finish 10 "every rm -r in the runner's scripts goes through one checking function (${fn:-none} in $(basename "${fnfile:-none}"), $n call site(s)); it refuses '' and /c/Users"
}

# ---------- 8 leftovers (after shoot jobs) ----------
c8() {
  local pw dbs lis st
  pw=$(tr -d '\r\n' < "$(cygpath -u "$LOCALAPPDATA")/sideroom/sr_pg_pw.txt")
  dbs=$(PGPASSWORD="$pw" "$PSQL" -h 127.0.0.1 -U postgres -At -c "select datname from pg_database where datname like 'sr\_p104\_d%'" 2>&1 | tr '\n' ' '); pw=""
  [ -z "${dbs// /}" ] || bad "databases left: $dbs"
  lis=$(netstat -ano | tr -d '\r' | grep LISTENING | grep -E ":(8851|8852|9551|9552) " | tr '\n' ';')
  [ -z "$lis" ] || bad "listeners left: $lis"
  st=$(bash "$D/strays.sh" "$RUN/lanes" "run-drill/lanes" --ports 8851 8852 9551 9552 2>&1); [ $? -eq 0 ] || bad "strays.sh: $(echo "$st" | tr '\n' ' ')"
  finish 8 "after the shoot jobs: no sr_p104_d* database, no listener on 8851 8852 9551 9552, no process naming run-drill/lanes"
}

# ---------- part 3: real cells ----------
MEMPS=$RUN/part3/sample.ps1
write_sampler() {
  mkdir -p "$RUN/part3"
  cat > "$MEMPS" <<'PSEOF'
param([string]$Run, [string]$Out, [int]$Every = 2, [int]$PortBase = 8850)
# one row per running job per sample: time,availMB,jobid,ws_mb,private_mb,nproc
# a job's own processes = every process whose command line names its worktree folder (the playwright node processes)
# plus all their descendants (the Playwright workers and Chromium), plus the server listening on its lane's port.
"time,availMB,jobid,ws_mb,private_mb,nproc" | Out-File -Encoding ascii $Out
while ($true) {
  $avail = [int](Get-CimInstance Win32_PerfFormattedData_PerfOS_Memory).AvailableMBytes
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, CommandLine
  $t = Get-Date -Format HH:mm:ss
  $rows = 0
  foreach ($d in Get-ChildItem (Join-Path $Run 'jobs') -Directory -ErrorAction SilentlyContinue) {
    $st = (Get-Content (Join-Path $d.FullName 'state') -ErrorAction SilentlyContinue | Select-Object -First 1)
    if ($st -ne 'running') { continue }
    $wt = ((Get-Content (Join-Path $d.FullName 'req') | Where-Object { $_ -like 'wt=*' } | Select-Object -First 1) -replace '^wt=', '')
    $leaf = Split-Path $wt -Leaf
    $lane = [int]((Get-Content (Join-Path $d.FullName 'lane') -ErrorAction SilentlyContinue | Select-Object -First 1))
    $set = @{}
    foreach ($p in $all) { if ($p.CommandLine -and $p.CommandLine -like "*$leaf*" -and $p.CommandLine -notlike '*Get-CimInstance*') { $set[[int]$p.ProcessId] = 1 } }
    if ($lane -gt 0) { $c = Get-NetTCPConnection -LocalPort ($PortBase + $lane) -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1; if ($c) { $set[[int]$c.OwningProcess] = 1 } }
    $grew = $true
    while ($grew) { $grew = $false; foreach ($p in $all) { if ($set.ContainsKey([int]$p.ParentProcessId) -and -not $set.ContainsKey([int]$p.ProcessId)) { $set[[int]$p.ProcessId] = 1; $grew = $true } } }
    $ws = 0; $pm = 0; $n = 0
    foreach ($id in $set.Keys) { $pr = Get-Process -Id $id -ErrorAction SilentlyContinue; if ($pr) { $ws += $pr.WorkingSet64; $pm += $pr.PrivateMemorySize64; $n++ } }
    "{0},{1},{2},{3},{4},{5}" -f $t, $avail, $d.Name, [int]($ws / 1MB), [int]($pm / 1MB), $n | Out-File -Encoding ascii -Append $Out
    $rows++
  }
  if ($rows -eq 0) { "{0},{1},-,0,0,0" -f $t, $avail | Out-File -Encoding ascii -Append $Out }
  Start-Sleep -Seconds $Every
}
PSEOF
}

part3() {
  local out="$RUN/part3" smp="" swp ida idb t0 sa sb st l1 l2 maxrun=0 n gate=""
  CUR=real
  heavy=$(bash "$D/heavy.sh" status 2>&1)
  echo "$heavy" | grep -q '^drill ' || { echo "part 3 not started: heavy.sh status shows no slot in the name 'drill':"; echo "$heavy"; return 2; }
  teardown; write_conf 'JOB_LIMIT_S=600' 'RECENT_S=45' 'HOL_S=240'   # the real costs and times: what the commander will run
  mkdir -p "$out"; write_sampler
  local base_avail; base_avail=$(powershell -NoProfile -Command '[int](Get-CimInstance Win32_PerfFormattedData_PerfOS_Memory).AvailableMBytes' | tr -d '\r')
  echo "idle available memory: $base_avail MB"
  powershell -NoProfile -ExecutionPolicy Bypass -File "$(cygpath -m "$MEMPS")" -Run "$(cygpath -m "$RUN")" -Out "$(cygpath -m "$out/samples.csv")" -Every 2 &
  smp=$!; swp=$(winpid $smp); echo "$swp" > "$out/sampler.pid"
  need_runner || { kill_tree "$swp"; teardown; return 1; }
  # one cell alone first (the memory figure for a single cell), then the two at once
  local solo; solo=$($JOBSH submit drill_solo shoot base --wt $WTW/p104-lane2 --env SR_P102_ARCADM_ON=1 --env SR_P102_ARCADM_PHASE=after --env SR_P102_ARCADM_OUT=$WTW/visual-sweep-shots/p104-drill/arcadm-solo -- tests/p102-arcadm-shots.spec.ts | tail -1 | tr -d '')
  $JOBSH wait "$solo" 150 >/dev/null 2>&1; echo "solo job $solo: $(state "$solo") rc $(jf "$solo" rc) lane $(jf "$solo" lane)"
  [ "$(state "$solo")" = done ] || bad "the solo shoot job ended '$(state "$solo")' rc $(jf "$solo" rc)"
  local envA="--env SR_P102_LOGS_ON=1 --env SR_P102_LOGS_PHASE=after --env SR_P102_LOGS_OUT=$WTW/visual-sweep-shots/p104-drill/logs"
  local envB="--env SR_P102_ARCADM_ON=1 --env SR_P102_ARCADM_PHASE=after --env SR_P102_ARCADM_OUT=$WTW/visual-sweep-shots/p104-drill/arcadm"
  ida=$($JOBSH submit drill_a shoot base --wt $WTW/p104-lane1 $envA -- tests/p102-logs-shots.spec.ts | tail -1 | tr -d '\r')
  idb=$($JOBSH submit drill_b shoot base --wt $WTW/p104-lane2 $envB -- tests/p102-arcadm-shots.spec.ts | tail -1 | tr -d '\r')
  [[ $ida =~ ^[0-9]{4}- ]] && [[ $idb =~ ^[0-9]{4}- ]] || { bad "submit printed '$ida' / '$idb'"; kill_tree "$swp"; teardown; finish 3 "two shoot jobs at once (lanes)"; return 1; }
  echo "submitted $ida and $idb at $(date +%H:%M:%S)"; t0=$(now)
  while :; do
    n=0; for id in "$ida" "$idb"; do [ "$(state "$id")" = running ] && n=$((n + 1)); done
    [ $n -gt $maxrun ] && maxrun=$n
    if [ $(( $(now) - t0 )) -ge 45 ] && [ -z "$gate" ] && [ "$(state "$idb")" = queued -o "$(state "$ida")" = queued ]; then
      gate="after 45 s one job is still queued: a '$(state "$ida")', b '$(state "$idb")'; real available now $(tail -1 "$out/samples.csv" | cut -d, -f2) MB"
      echo "GATE: $gate"; echo 8000 > "$RUN/fake-avail"; echo "raised fake-avail to 8000 once at $(date +%H:%M:%S)"
    fi
    is_end "$(state "$ida")" && is_end "$(state "$idb")" && break
    [ $(( $(now) - t0 )) -ge 540 ] && { bad "jobs not finished in 9 minutes: a '$(state "$ida")', b '$(state "$idb")'"; break; }
    sleep 2
  done
  kill_tree "$swp"; rm -f "$RUN/fake-avail"
  sa=$($JOBSH wait "$ida" 10 >/dev/null 2>&1; echo $?); sb=$($JOBSH wait "$idb" 10 >/dev/null 2>&1; echo $?)
  [ "$sa" = 0 ] || bad "drill_a: wait exit $sa, state $(state "$ida"), rc $(jf "$ida" rc) (log $($JOBSH log "$ida" 2>/dev/null))"
  [ "$sb" = 0 ] || bad "drill_b: wait exit $sb, state $(state "$idb"), rc $(jf "$idb" rc) (log $($JOBSH log "$idb" 2>/dev/null))"
  l1=$(jf "$ida" lane); l2=$(jf "$idb" lane)
  [ -n "$l1" ] && [ -n "$l2" ] && [ "$l1" != "$l2" ] || bad "lanes: a '$l1', b '$l2' (wanted two different lanes)"
  local a1 a2 b1 b2; a1=$(ep "$ida" started); a2=$(ep "$ida" ended); b1=$(ep "$idb" started); b2=$(ep "$idb" ended)
  [ -n "$a1" ] && [ -n "$b1" ] && [ -n "$a2" ] && [ -n "$b2" ] && [ "$a1" -lt "$b2" ] && [ "$b1" -lt "$a2" ] || bad "the two jobs' times do not overlap: a $a1..$a2, b $b1..$b2"
  [ $maxrun -le 2 ] || bad "more than 2 shoot jobs running at once ($maxrun)"
  local ta tb; ta=${ida%%-*}; tb=${idb%%-*}
  for pair in "$ida:$ta:$tb" "$idb:$tb:$ta"; do
    local i=${pair%%:*} rest=${pair#*:} own other; own=${rest%%:*}; other=${rest#*:}
    if grep -q "sr_p104_d" "$RUN/jobs/$i/log" 2>/dev/null; then
      grep -q "sr_p104_d$own" "$RUN/jobs/$i/log" || bad "$i: its log never names its own database sr_p104_d$own"
      grep -q "sr_p104_d$other" "$RUN/jobs/$i/log" && bad "$i: its log names the other job's database sr_p104_d$other"
    else echo "INFO 3 $i: the log names no database (own-database check read from the lane/ports instead)"; fi
  done
  finish 3 "two shoot jobs at once: both passed, overlapped (a ${a1:-?}-${a2:-?}, b ${b1:-?}-${b2:-?}), lanes $l1 and $l2, at most $maxrun running ${gate:+; gate held one back (see report)}"
  c8
  c7
  analyse "$out/samples.csv" "$ida" "$idb" "$base_avail"
  teardown
}

analyse() { # analyse <csv> <ida> <idb> <idle avail>: the memory table, from the sampler's rows
  local csv=$1
  echo "--- memory (per job: its own PID tree, working set / private MB; from $(basename "$csv")) ---"
  awk -F, -v A="$2" -v B="$3" -v idle="$4" '
    NR==1 {next}
    { t=$1; av=$2; id=$3; ws=$4; pm=$5; np=$6
      if (!(t in seen)) { seen[t]=1; order[++nt]=t; minav_t[t]=av }
      if (id != "-") { cnt[t]++; sum[t]+=ws; sump[t]+=pm; if (ws>peak[id]) peak[id]=ws; if (pm>peakp[id]) peakp[id]=pm; if (np>pn[id]) pn[id]=np }
      if (min=="" || av<min) { min=av; mint=t } }
    END {
      for (i=1;i<=nt;i++) { t=order[i]
        if (cnt[t]==1 && sum[t]>alone) alone=sum[t]
        if (cnt[t]==2) { if (sum[t]>two) two=sum[t]; if (sump[t]>twop) twop=sump[t] } }
      printf "idle available before: %s MB\n", idle
      printf "job a peak working set: %s MB (private %s, up to %s processes)\n", peak[A], peakp[A], pn[A]
      printf "job b peak working set: %s MB (private %s, up to %s processes)\n", peak[B], peakp[B], pn[B]
      printf "one cell alone (a sample with one job running), highest: %s MB\n", alone+0
      printf "two cells together, highest sum: %s MB (private %s)\n", two+0, twop+0
      printf "low point of available memory: %s MB at %s\n", min, mint
      m=(peak[A]>peak[B])?peak[A]:peak[B]; c=int((m+99)/100)*100+100
      printf "proposed COST_SHOOT: %d (peak of one cell %s MB rounded up to the next 100, plus 100)\n", c, m
    }' "$csv" | tee "$RUN/part3/memory.txt"
}

# ---------- 11 why a job is held (wave 106, item 1) ----------
nhold() { grep -c "^[0-9:]* hold $1 $2" "$RUN/runner.log" 2>/dev/null; }   # lines of one job and reason
c11() {
  local k h d id n
  setup 11 'JOB_LIMIT_S=80' 'RECENT_S=5' 'HOL_S=15'; : > "$RUN/runner.log"; fake 1500
  need_runner || { teardown; finish 11 "hold lines"; return; }
  k=$(submit_test hk 100 40 0 c11-a)
  wait_state "$k" running 15 || bad "the keeper did not start (state $(state "$k"))"
  h=$(submit_test hh 1900 3 0 c11-b); d=$(submit_test hd 100 3 0 c11-a)
  sleep 9                                                          # several passes (TICK 3), before HOL_S
  n=$(nhold "$h" mem); [ "$n" = 1 ] || bad "head: $n 'hold $h mem' lines after 9 s (wanted exactly 1, not one per pass)"
  grep -q "hold $h mem avail=1500 recent=[0-9]* reserve=700 cost=1900" "$RUN/runner.log" || bad "the mem line lacks avail=1500 ... reserve=700 cost=1900: $(grep "hold $h" "$RUN/runner.log" | head -1)"
  n=$(nhold "$d" 'wt '); [ "$n" = 1 ] || bad "same-worktree job: $n 'hold $d wt' lines (wanted 1)"
  sleep 14                                                         # the head is now past HOL_S: the job behind it is held by the head
  n=$(nhold "$d" "hol $h"); [ "$n" = 1 ] || bad "behind a head past HOL_S: $n 'hold $d hol $h' lines (wanted 1)"
  n=$(nhold "$h" mem); [ "$n" = 1 ] || bad "head: $n mem lines after 23 s (wanted still 1)"
  fake 20000
  for id in "$k" "$h" "$d"; do wait_end "$id" 70 || bad "$id did not end (state $(state "$id"))"; done
  [ "$(nhold "$h" mem)" = 1 ] || bad "after the release the head has $(nhold "$h" mem) mem lines (wanted 1)"
  teardown
  # stop_at: one line per job
  local at; at=$(date -d '+1 minute' +%H:%M)
  setup 11 'JOB_LIMIT_S=120' "STOP_AT=$at"; : > "$RUN/runner.log"; fake 20000
  if need_runner; then
    id=$(submit_test late 50 3 0 c11-s); sleep 10
    n=$(nhold "$id" stop_at); [ "$n" = 1 ] || bad "stop_at: $n 'hold $id stop_at' lines after 10 s (wanted 1)"
  fi
  CUR=11; cp "$RUN/runner.log" "$RUN/evidence-c11.log" 2>/dev/null
  teardown; finish 11 "hold lines: one per job and reason (mem with its figures, wt, hol, stop_at), not one per pass"
}

# ---------- 12 the cargo class (wave 106, item 4; the real cargo is never run: SR_CARGO_CMD names a stub) ----------
c12() {
  local stub="$RUN/cargo-stub.sh" log="$RUN/cargo-stub.log" c f t1 c2 t3 ok rc
  setup 12 'JOB_LIMIT_S=10' 'CARGO_LIMIT_S=40' 'COST_CARGO=3000' 'HOL_S=60'; fake 20000; : > "$RUN/runner.log"
  cat > "$stub" <<STUB
#!/usr/bin/env bash
echo "ARGS=\$* TARGET=\$CARGO_TARGET_DIR DEBUG=\$CARGO_PROFILE_DEV_DEBUG PWD=\$PWD" >> "$log"
sleep \$(cat "$RUN/stub-sleep" 2>/dev/null || echo 2)
STUB
  : > "$log"; echo 2 > "$RUN/stub-sleep"
  export SR_CARGO_CMD="$stub"
  # submit-time refusals
  $JOBSH submit cg cargo --wt "$(cygpath -m "$WTS")/c12-r" -- publish >/dev/null 2>&1 && bad "submit accepted 'cargo -- publish' (wanted refused)"
  $JOBSH submit cg cargo --wt "$(cygpath -m "$WTS")/c12-r" >/dev/null 2>&1 && bad "submit accepted a cargo job with no arguments"
  need_runner || { unset SR_CARGO_CMD; teardown; finish 12 "cargo class"; return; }
  mkdir -p "$WTS/c12-a" "$WTS/c12-b" "$WTS/c12-c" "$WTS/c12-d"
  # arguments, target folder, debug off, --offline where cargo takes it
  c=$($JOBSH submit cgb cargo --wt "$(cygpath -m "$WTS")/c12-a" -- build -p sr-media | tail -1 | tr -d '\r')
  wait_end "$c" 30 || bad "cargo build job did not end (state $(state "$c"))"
  [ "$(state "$c")" = done ] || bad "cargo build job ended $(state "$c") rc $(jf "$c" rc)"
  grep -q "ARGS=build --offline -p sr-media TARGET=$W/p104-cgb-target DEBUG=0 PWD=.*c12-a" "$log" || bad "stub line for build: $(sed -n 1p "$log")"
  f=$($JOBSH submit cgf cargo --wt "$(cygpath -m "$WTS")/c12-b" -- fmt --check | tail -1 | tr -d '\r')
  wait_end "$f" 30 || bad "cargo fmt job did not end"
  grep -q "ARGS=fmt --check TARGET" "$log" || bad "fmt got --offline or other args: $(sed -n 2p "$log")"
  # alone: a running test job holds the cargo job back; while cargo runs nothing else starts
  echo 8 > "$RUN/stub-sleep"
  t1=$(submit_test ct1 50 12 0 c12-a)
  wait_state "$t1" running 15 || bad "the test job did not start"
  c2=$($JOBSH submit cgc cargo --wt "$(cygpath -m "$WTS")/c12-c" -- test | tail -1 | tr -d '\r')
  sleep 5
  [ "$(state "$c2")" = queued ] || bad "cargo started while another job was running (state $(state "$c2"))"
  grep -q "hold $c2 cargo" "$RUN/runner.log" || bad "no 'hold $c2 cargo' line in runner.log"
  wait_state "$c2" running 30 || bad "cargo never started after the test job ended (state $(state "$c2"))"
  t3=$(submit_test ct3 50 2 0 c12-d); sleep 5
  [ "$(state "$t3")" = queued ] || bad "a job started while cargo was running (state $(state "$t3"))"
  grep -q "hold $t3 cargo" "$RUN/runner.log" || bad "no 'hold $t3 cargo' line"
  wait_end "$c2" 30; wait_end "$t3" 30
  local s3 e2; s3=$(ep "$t3" started); e2=$(ep "$c2" ended)
  [ -n "$s3" ] && [ -n "$e2" ] && [ "$s3" -ge "$e2" ] || bad "the job behind cargo started $s3 before cargo ended $e2"
  # its own limit: JOB_LIMIT_S=10 would kill a 15 s run; CARGO_LIMIT_S=40 does not
  echo 15 > "$RUN/stub-sleep"
  ok=$($JOBSH submit cgl cargo --wt "$(cygpath -m "$WTS")/c12-a" -- clippy | tail -1 | tr -d '\r')
  wait_end "$ok" 45 || bad "the 15 s cargo job did not end"
  [ "$(state "$ok")" = done ] || bad "a 15 s cargo job ended $(state "$ok") rc $(jf "$ok" rc) under JOB_LIMIT_S=10 (wanted done: CARGO_LIMIT_S=40)"
  unset SR_CARGO_CMD; teardown; finish 12 "cargo class: bad arguments refused; target folder, DEBUG=0, --offline (not for fmt); alone and holds others; own limit"
}

# ---------- 13 --bin on a shoot (wave 106, item 5; no cell is started) ----------
c13() {
  local good="$RUN/binok" bn="$RUN/binmiss" b92="$RUN/p92-base-bin" id out
  setup 13; mkdir -p "$good" "$bn" "$b92"; : > "$good/sr-server.exe"; : > "$good/sr-cli.exe"; : > "$bn/sr-server.exe"; : > "$b92/sr-server.exe"; : > "$b92/sr-cli.exe"
  sub() { $JOBSH submit bn shoot base --wt "$(cygpath -m "$WTS")/c13" --bin "$1" -- tests/x.spec.ts 2>&1; }
  out=$(sub "$(cygpath -m "$bn")") && bad "a folder without sr-cli.exe was accepted"
  out=$(sub "$(cygpath -m "$b92")") && bad "p92-base-bin was accepted"
  out=$(sub "C:/Windows") && bad "a folder outside SIDEROOM_wt was accepted"
  out=$(sub "$(cygpath -m "$RUN")/nosuch") && bad "a missing folder was accepted"
  out=$(sub "$(cygpath -m "$good")/../../../../../../../Windows") && bad "a path climbing out with .. was accepted"; out=$(sub "$(cygpath -m "$b92")/../binok") || bad "a .. path that resolves to a good folder was refused: $out"
  out=$(sub "$(cygpath -m "$good")") || bad "a good folder was refused: $out"
  id=$(echo "$out" | tail -1 | tr -d '\r')
  grep -qx "bin=$(cygpath -u "$good")" "$RUN/jobs/$id/req" || bad "the request of $id does not hold the bin: $(grep '^bin' "$RUN/jobs/$id/req")"
  $JOBSH submit bn shoot base --wt "$(cygpath -m "$WTS")/c13" -- tests/x.spec.ts 2>&1 | tail -1 | grep -q '^[0-9]' || bad "a shoot without --bin was refused"
  teardown; finish 13 "--bin: refused (no sr-cli.exe, p92-base-bin, outside the wave folder, missing, .. out of the tree); a good folder is in the job's request"
}

# ---------- main ----------
cleanup_all() { teardown; [ -f "$RUN/part3/sampler.pid" ] && kill_tree "$(tr -d '\r\n ' < "$RUN/part3/sampler.pid")"; }
trap 'cleanup_all' EXIT
trap 'echo "interrupted"; exit 130' INT TERM

bash -n "$0" || { echo "drill: bash -n failed"; exit 2; }
mkdir -p "$RUN" "$WTS"
checks=("$@"); [ ${#checks[@]} -gt 0 ] || checks=(0 1 2 3 4 5 6 7 9 10 11 12 13)
for c in "${checks[@]}"; do
  case $c in
    0) c0 ;; 1) c1 ;; 2) c2 ;; 3) c3 ;; 4) c4 ;; 5) c5 ;; 6) c6 ;; 7) c7 ;; 8) c8 ;; 9) c9 ;; 10) c10 ;; 11) c11 ;; 12) c12 ;; 13) c13 ;;
    real) part3 ;;
    *) echo "unknown check '$c'" >&2 ;;
  esac
done
echo "drill: $PASSED/$RAN"
[ $PASSED -eq $RAN ]
