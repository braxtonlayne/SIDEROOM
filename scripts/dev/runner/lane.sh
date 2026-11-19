#!/usr/bin/env bash
# lane.sh (wave 104, D-986): the lane worktrees of LANE_WTS (made by the commander; this script never runs `git worktree add`).
#   lane.sh sync <n|all> <commit>   git checkout --detach <commit> in the lane worktree(s) (n = position in LANE_WTS); pnpm install --offline if the lockfile changed
#   lane.sh status                  each lane worktree: commit, dirty or clean, a running job in it, the lane's cell (port, server alive)
. "$(dirname "$(readlink -f "$0")")/runlib.sh"
load_conf; init_run

lane_wt() { local i=0 w; for w in $LANE_WTS; do i=$((i+1)); [ "$i" = "$1" ] && { cygpath -u "$w"; return 0; }; done; return 1; }
busy_in() { local id; for id in $(jids); do [ "$(jget "$id" state)" = running ] && [ "$(jget "$id" wt)" = "$1" ] && { echo "$id"; return 0; }; done; return 1; }

sync_one() { # <n> <commit>
  local wt old new b; wt=$(lane_wt "$1") || { echo "no lane $1 in LANE_WTS" >&2; return 1; }
  [ -d "$wt/.git" ] || [ -f "$wt/.git" ] || { echo "$wt is not a worktree" >&2; return 1; }
  b=$(busy_in "$wt") && { echo "lane $1: job $b is running in $wt: not syncing" >&2; return 1; }
  old=$(git -C "$wt" rev-parse HEAD)
  git -C "$wt" checkout -q --detach "$2" || return 1
  new=$(git -C "$wt" rev-parse HEAD)
  if ! git -C "$wt" diff --quiet "$old" "$new" -- pnpm-lock.yaml; then
    echo "lane $1: lockfile changed, pnpm install --offline"
    (cd "$wt" && npm_config_manage_package_manager_versions=false npx pnpm@10.12.1 install --offline --frozen-lockfile) || return 1
  fi
  echo "lane $1: $wt at $(git -C "$wt" rev-parse --short HEAD)"
}

do_status() {
  local i=0 w n; for w in $LANE_WTS; do
    i=$((i+1)); w=$(cygpath -u "$w")
    echo "lane $i: $w $(git -C "$w" rev-parse --short HEAD 2>/dev/null) $([ -z "$(git -C "$w" status --porcelain 2>/dev/null)" ] && echo clean || echo dirty) job=$(busy_in "$w" || echo none)"
  done
  for n in $LANES; do
    echo "cell $n: port $((PORT_BASE + n)) $(port_open $((PORT_BASE + n)) && echo LISTENING || echo closed), metrics $((METRICS_BASE + n)) $(port_open $((METRICS_BASE + n)) && echo LISTENING || echo closed)"
  done
  [ -n "$LANE_WTS" ] || echo "(LANE_WTS is empty in $RUN/conf)"
}

case ${1:-} in
  sync) [ -n "${3:-}" ] || { sed -n 2,5p "$0"; exit 2; }
        if [ "$2" = all ]; then i=0; rc=0; for w in $LANE_WTS; do i=$((i+1)); sync_one "$i" "$3" || rc=1; done; exit $rc; else sync_one "$2" "$3"; fi;;
  status) do_status;;
  *) sed -n 2,5p "$0"; exit 2;;
esac
