#!/usr/bin/env bash
# Cell (wave 103 pilot, D-985): ONE sr-server serving a BUILT web bundle on its own port and database.
# No Vite. The page origin is the API port (SR_E2E_WEB=server). A pilot: plain, to be turned into the wave 104 runner.
#
#   bash cell.sh up <port> <metrics-port> <dist> <db> <cell-folder>   # start, wait for /readyz (up to 90 s), write <cell-folder>/pid
#   bash cell.sh down <cell-folder>                                    # stop by PID tree, check the port is free
#
# The binaries come from $CELL_BIN (default .../p104-cell-bin, a COPY of p92-base-bin; never run p92-base-bin itself).
# The Postgres password is read from %LOCALAPPDATA%\sideroom\sr_pg_pw.txt into the server's environment and never printed.
# The working directory of the server is the cell folder (never inside apps/web: the search index goes to its data/).
W=/c/Users/braxt/Desktop/SIDEROOM_wt
BIN=${CELL_BIN:-$W/p104-cell-bin}
cmd=${1:-}
pw() { tr -d '\r\n' < "$(cygpath -u "$LOCALAPPDATA")/sideroom/sr_pg_pw.txt"; }
winpid() { cat "/proc/$1/winpid" 2>/dev/null || echo "$1"; }
case $cmd in
  up)
    port=$2; mport=$3; dist=$4; db=$5; cell=$6
    [ -n "$cell" ] || { echo "usage: cell.sh up <port> <metrics-port> <dist> <db> <cell-folder>" >&2; exit 2; }
    mkdir -p "$cell/search"; cd "$cell" || exit 1
    curl -s -m 2 "http://127.0.0.1:$port/readyz" >/dev/null && { echo "port $port already answers: not starting" >&2; exit 1; }
# wave 104: CELL_SERVER_ENV = newline-separated K=V pairs added to the server environment (the runner's --server-env)
    while IFS= read -r kv; do [ -n "$kv" ] && export "$kv"; done <<< "${CELL_SERVER_ENV:-}"
    SR_DATABASE__URL="postgres://postgres:$(pw)@127.0.0.1:5432/$db" \
    SR_DATABASE__AUTO_MIGRATE=true \
    SR_SERVER__BIND="127.0.0.1:$port" \
    SR_SERVER__PUBLIC_URL="http://localhost:$port" \
    SR_SERVER__METRICS_BIND="127.0.0.1:$mport" \
    SR_STORAGE__ENDPOINT=memory: \
    SR_ACTIVITIES__ENABLED=true \
    SR_ADMIN_REQUIRE_REAUTH=0 \
    SR_WEB__ROOT="$dist" \
    SR_WEB__SERVE_PUBLIC=true \
    SR_SEARCH__DIR="$(cygpath -m "$cell")/search" \
      nohup "$BIN/sr-server.exe" > "$cell/server.log" 2>&1 &
    echo $! > "$cell/pid"; echo "$port $mport" > "$cell/ports"   # wave 104: so down can find the listeners
    for i in $(seq 1 45); do
      curl -s -m 2 "http://127.0.0.1:$port/readyz" >/dev/null && { cat "/proc/$(cat "$cell/pid")/winpid" > "$cell/winpid" 2>/dev/null; echo "cell up on $port (pid $(cat "$cell/pid"))"; exit 0; }
      sleep 2
    done
    echo "cell did not become ready on $port; see $cell/server.log" >&2; exit 1 ;;
  down)
    cell=$2; [ -f "$cell/pid" ] || { echo "no pid file in $cell" >&2; exit 1; }
    p=$(cat "$cell/pid"); wp=$(cat "/proc/$p/winpid" 2>/dev/null); [ -n "$wp" ] || wp=$(cat "$cell/winpid" 2>/dev/null)   # wave 104: fresh winpid (after exec), else the recorded one; never a cygwin pid taken as a Windows pid
    [ -n "$wp" ] && taskkill //PID "$wp" //T //F >/dev/null 2>&1; rm -f "$cell/pid" "$cell/winpid"
    # wave 104 (twin's FAULT): the listener of each port IS the server, whatever PID was recorded: stop it by its own PID, then check both ports
    for pt in $(cat "$cell/ports" 2>/dev/null); do
      for lp in $(netstat -ano | grep LISTENING | grep ":$pt[[:space:]]" | awk '{print $NF}' | sort -u); do taskkill //PID "$lp" //T //F >/dev/null 2>&1; done
    done
    sleep 1
    for pt in $(cat "$cell/ports" 2>/dev/null); do netstat -ano | grep LISTENING | grep -q ":$pt[[:space:]]" && echo "WARNING: port $pt still listening" >&2; done
    sleep 1
    echo "stopped pid $p (win $wp)" ;;
  *) echo "usage: cell.sh up|down ..." >&2; exit 2 ;;
esac
