#!/usr/bin/env bash
# World template (wave 103 pilot, D-985): seed the sweep world ONCE, then clone it per job.
#
#   bash world.sh seed  <template-db> <dist> <port>          # run from the worktree whose e2e/ seeds (its root is the cwd)
#   bash world.sh clone <template-db> <new-db> <port> <worktree>
#   bash world.sh drop  <db>
#
# seed:  create the empty database, start a cell (cell.sh), global setup + "seed the sweep world", stop the cell, copy
#        <worktree>/e2e/.state and the cell's search folder to $W/<template-db>-state/ (with a `port` file).
# clone: CREATE DATABASE <new-db> TEMPLATE <template-db> (the template must have NO connections: the cell is stopped);
#        copy the state into <worktree>/e2e/.state with the old port rewritten to <port> in every json, copy the search folder.
# The password is read into PGPASSWORD / the db URL inside this script and never echoed.
W=/c/Users/braxt/Desktop/SIDEROOM_wt
D=$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)   # wave 106: own folder (a clone runs itself)
PSQL="/c/Program Files/PostgreSQL/17/bin/psql.exe"
BIN=${CELL_BIN:-$W/p103-pilot-bin}
export PGPASSWORD; PGPASSWORD=$(tr -d '\r\n' < "$(cygpath -u "$LOCALAPPDATA")/sideroom/sr_pg_pw.txt")
safe_rm() { local p rest n x min=${2:-2}; p=$(cygpath -u "$1" 2>/dev/null); case "$p" in ""|*..*) echo "safe_rm: refuses [$1]" >&2; return 1;; "$W"/*) ;; *) echo "safe_rm: refuses $p" >&2; return 1;; esac
  rest=${p#"$W"/}; n=0; for x in ${rest//// }; do n=$((n+1)); done; [ $n -ge $min ] || { echo "safe_rm: too shallow $p" >&2; return 1; }; rm -rf "$p"; }   # wave 104: the one rm -rf
sql() { "$PSQL" -h 127.0.0.1 -U postgres -v ON_ERROR_STOP=1 -q -c "$1"; }
url() { echo "postgres://postgres:$PGPASSWORD@127.0.0.1:5432/$1"; }
cmd=${1:-}
case $cmd in
  seed)
    tpl=$2; dist=$3; port=$4; wt=$(pwd); cell=$W/$tpl-cell
    [ -n "$port" ] || { echo "usage: world.sh seed <template-db> <dist> <port>" >&2; exit 2; }
    sql "DROP DATABASE IF EXISTS $tpl;" && sql "CREATE DATABASE $tpl;" || exit 1
    safe_rm "$cell" 1; safe_rm "$wt/e2e/.state"; safe_rm "$W/$tpl-state" 1
    bash "$D/cell.sh" up "$port" $((port + 700)) "$dist" "$tpl" "$cell" || exit 1
    cd "$wt/e2e" || exit 1
    export SR_E2E_WEB=server SR_E2E_SERVER_PORT="$port" SR_E2E_BASE_URL="http://localhost:$port" SR_CLI_BIN="$BIN/sr-cli.exe" \
           SR_DATABASE__URL="$(url "$tpl")" DATABASE_URL="$(url "$tpl")" SR_E2E_SWEEP=1
    # global setup (migrate + bootstrap Owner) runs by itself before the first test of the run
    npx playwright test tests/page-sweep.spec.ts -g "seed the sweep world" --project=chromium --workers=1 --retries=0 --reporter=line \
      2>&1 | tee "$W/$tpl-seed.log"; rc=${PIPESTATUS[0]}
    bash "$D/cell.sh" down "$cell"
    grep -q "FAIL" "$W/$tpl-seed.log" && { echo "seed printed a FAIL line" >&2; rc=1; }
    [ $rc -eq 0 ] || { echo "seed failed (rc $rc), see $W/$tpl-seed.log" >&2; exit $rc; }
    mkdir -p "$W/$tpl-state" && cp -r "$wt/e2e/.state/." "$W/$tpl-state/" && cp -r "$cell/search" "$W/$tpl-state/search" && echo "$port" > "$W/$tpl-state/port"
    echo "template $tpl seeded; state in $W/$tpl-state" ;;
  clone)
    tpl=$2; new=$3; port=$4; wt=$5
    [ -n "$wt" ] || { echo "usage: world.sh clone <template-db> <new-db> <port> <worktree>" >&2; exit 2; }
    st=$W/$tpl-state; old=$(cat "$st/port")
    sql "DROP DATABASE IF EXISTS $new;" && sql "CREATE DATABASE $new TEMPLATE $tpl;" || exit 1
    safe_rm "$wt/e2e/.state"; mkdir -p "$wt/e2e/.state"
    cp -r "$st/." "$wt/e2e/.state/"; safe_rm "$wt/e2e/.state/search"; safe_rm "$wt/e2e/.state/port"
    if [ "$old" != "$port" ]; then
      find "$wt/e2e/.state" -name '*.json' -exec sed -i "s/localhost:$old/localhost:$port/g; s/127.0.0.1:$old/127.0.0.1:$port/g" {} +
    fi
    cell=${6:-$W/$new-cell}; safe_rm "$cell" 1; mkdir -p "$cell"; cp -r "$st/search" "$cell/search"   # wave 104: optional 6th argument = the cell folder
    echo "clone $new ready (state in $wt/e2e/.state, port $old -> $port, cell folder $cell)" ;;
  drop) sql "DROP DATABASE IF EXISTS $2;" && echo "dropped $2" ;;
  *) echo "usage: world.sh seed|clone|drop ..." >&2; exit 2 ;;
esac
