#!/bin/sh
# The systemd route, checked on a Linux machine (RDY-OPS-003): the unit files pass `systemd-analyze verify`, and the
# static-binary install of deploy/INSTALL.md (sections 3 to 5) works end to end: the binaries go under a prefix,
# /etc/sideroom/config.toml comes from the template in INSTALL.md section 4, secrets go in server.env, sr-cli migrates
# and bootstraps, and sr-server runs as a systemd USER service long enough to answer /healthz.
# .github/workflows/prod-smoke.yml runs all three stages on an Ubuntu runner.
#
#   deploy/scripts/smoke-systemd.sh verify         systemd-analyze verify over deploy/systemd/*
#   deploy/scripts/smoke-systemd.sh user-service   the static-binary install under a prefix, started as a user service
#   deploy/scripts/smoke-systemd.sh system-service the plain-binary section of INSTALL.md for real (the sideroom user,
#                                /opt/sideroom, /etc/sideroom, /var/lib/sideroom) and the unit copied UNCHANGED into
#                                /etc/systemd/system, started as a system service with every hardening line in place
#                                (D-1047, SEC-AUD-578). Needs sudo and a system manager (a hosted GitHub runner has both);
#                                refused unless CI=true or SMOKE_ALLOW_SUDO=1, because it changes the machine.
#
#   SMOKE_SERVER_BIN, SMOKE_CLI_BIN            the static binaries (the musl ones that ship in the server image)
#   SMOKE_WEB_DIR                the built apps/web/dist (user-service). It goes to <prefix>/opt/sideroom/web, which the
#                                unit names as SR_WEB__ROOT; the server refuses to start without its app/csp.txt (D-385)
#   SMOKE_DATABASE_URL           an empty, reachable Postgres database for user-service, e.g.
#                                postgres://sideroom:secret@127.0.0.1:5432/sideroom?sslmode=disable
#   SMOKE_PREFIX                 where the tree is built (default $RUNNER_TEMP/sr-systemd/prefix); /etc/sideroom,
#                                /opt/sideroom and /var/lib/sideroom live under it
#   SMOKE_PORT                   server port (default 18080); the metrics listener uses SMOKE_PORT + 1
#   SMOKE_ALLOW_SUDO=1           verify installs a `sideroom` user, /opt/sideroom/bin/sr-server and a /opt/sideroom/deploy
#                                link with sudo, because the units name them and verify checks that they exist.
#                                Refused unless CI=true or this is set: it changes the machine.
#   SMOKE_SYSTEMD_USER_FALLBACK=1  when no systemd user manager can be started, run the unit's ExecStart directly and say
#                                so loudly. The run then does NOT prove the unit. Default: fail.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
. "$here/lib.sh"
repo=$(cd "$here/../.." && pwd)
work=${SMOKE_WORK:-${RUNNER_TEMP:-/tmp}/sr-systemd}
prefix=${SMOKE_PREFIX:-$work/prefix}
port=${SMOKE_PORT:-18080}
mport=$((port + 1))

sudo_run() {
  if [ "$(id -u)" = "0" ]; then "$@"; else sudo "$@"; fi
}

stage_verify() {
  need systemd-analyze
  if [ "${CI:-}" != "true" ] && [ "${SMOKE_ALLOW_SUDO:-0}" != "1" ]; then
    die "verify installs a user and files under /opt/sideroom with sudo; set SMOKE_ALLOW_SUDO=1 to allow it"
  fi
  : "${SMOKE_SERVER_BIN:?set SMOKE_SERVER_BIN}"
  getent passwd sideroom >/dev/null 2>&1 || sudo_run useradd --system --no-create-home --shell /usr/sbin/nologin sideroom
  sudo_run install -d -m 755 /opt/sideroom/bin
  sudo_run install -m 755 "$SMOKE_SERVER_BIN" /opt/sideroom/bin/sr-server
  # The units run /opt/sideroom/deploy/scripts/*.sh and name /opt/sideroom/deploy/compose/prod.yml.
  sudo_run ln -sfn "$repo/deploy" /opt/sideroom/deploy
  systemd-analyze --version | head -n 1
  log "systemd-analyze verify deploy/systemd/*"
  # No --man=no: a unit with a misspelt key is the point of the exercise. Warnings count as failures.
  out=$(systemd-analyze verify "$repo"/deploy/systemd/*.service "$repo"/deploy/systemd/*.timer 2>&1) && rc=0 || rc=$?
  [ -z "$out" ] || echo "$out"
  [ "$rc" = "0" ] || die "systemd-analyze verify failed (exit $rc)"
  # Known: the runner's own units (/usr/lib, /lib, /etc systemd paths) can warn on a newer systemd
  # (259 dropped CPUAccounting=). Those lines are not about our units, so they do not count.
  out=$(printf '%s\n' "$out" | grep -vE '^(/usr/lib|/lib|/etc)/systemd/' || true)
  [ -z "$out" ] || die "systemd-analyze verify printed warnings (above); fix the units or list the warning as known"
  log "all unit files pass systemd-analyze verify"
}

# The first ```toml block of INSTALL.md is the documented config.toml template (section 4).
config_template() {
  awk '/^```toml/ { f = 1; next } /^```/ { if (f) exit } f' "$repo/deploy/INSTALL.md"
}

user_manager_ready() { systemctl --user show-environment >/dev/null 2>&1; }

start_user_manager() {
  user_manager_ready && return 0
  uid=$(id -u)
  XDG_RUNTIME_DIR=/run/user/$uid
  export XDG_RUNTIME_DIR
  DBUS_SESSION_BUS_ADDRESS=unix:path=$XDG_RUNTIME_DIR/bus
  export DBUS_SESSION_BUS_ADDRESS
  user_manager_ready && return 0
  log "no systemd user manager yet: enabling linger and starting user@$uid.service"
  sudo_run loginctl enable-linger "$(id -un)" || true
  sudo_run systemctl start "user@$uid.service" || true
  i=0
  while [ "$i" -lt 30 ]; do
    user_manager_ready && return 0
    i=$((i + 1))
    sleep 1
  done
  return 1
}

# WHAT THIS MODE CANNOT PROVE (D-1044). The user unit is a sed-edited copy of sr-server.service: it deletes User=, Group=,
# StateDirectory=, ReadWritePaths=, ProtectSystem=, ProtectHome=, PrivateTmp=, PrivateDevices=, ProtectKernel*,
# ProtectControlGroups=, RestrictNamespaces= and LimitNOFILE=, moves every path under a prefix in $HOME, and (with
# SMOKE_SYSTEMD_USER_FALLBACK=1) may not use systemd at all. So it cannot see a fault that only the real sandbox shows: a
# read-only root, a folder the service user may not write, a missing StateDirectory (this is how SEC-AUD-566 got through).
# The `system-service` mode below runs the unit as shipped and proves those; this mode proves only the static-binary steps.
stage_user_service() {
  : "${SMOKE_SERVER_BIN:?set SMOKE_SERVER_BIN}"
  : "${SMOKE_CLI_BIN:?set SMOKE_CLI_BIN}"
  : "${SMOKE_DATABASE_URL:?set SMOKE_DATABASE_URL}"
  : "${SMOKE_WEB_DIR:?set SMOKE_WEB_DIR (the built apps/web/dist)}"
  [ -f "$SMOKE_WEB_DIR/app/csp.txt" ] || die "$SMOKE_WEB_DIR has no app/csp.txt: is it the built apps/web/dist?"
  need curl
  need openssl
  rm -rf "$prefix"
  umask 077
  mkdir -p "$prefix/opt/sideroom/bin" "$prefix/var/lib/sideroom"
  mkdir -p "$prefix/etc/sideroom"
  chmod 700 "$prefix/etc/sideroom" || true
  install -m 755 "$SMOKE_SERVER_BIN" "$prefix/opt/sideroom/bin/sr-server"
  install -m 755 "$SMOKE_CLI_BIN" "$prefix/opt/sideroom/bin/sr-cli"
  log "the web build in /opt/sideroom/web (INSTALL.md, plain binary step 1): the unit sets SR_WEB__ROOT to it"
  mkdir -p "$prefix/opt/sideroom/web"
  cp -R "$SMOKE_WEB_DIR"/. "$prefix/opt/sideroom/web/"
  chmod -R u+rwX,go+rX "$prefix/opt/sideroom/web"

  log "config.toml from the template in INSTALL.md section 4"
  config_template >"$prefix/etc/sideroom/config.toml"
  grep -q '^\[server\]' "$prefix/etc/sideroom/config.toml" || die "found no toml template in INSTALL.md"
  # Only the listener ports change, so the smoke does not collide with anything else on the machine.
  sed -i "s|^bind = \"127.0.0.1:8080\"|bind = \"127.0.0.1:$port\"|; s|^metrics_bind = \"127.0.0.1:9100\"|metrics_bind = \"127.0.0.1:$mport\"|" \
    "$prefix/etc/sideroom/config.toml"
  grep -q "127.0.0.1:$port" "$prefix/etc/sideroom/config.toml" || die "the template no longer has a bind line the smoke can move"

  log "server.env: the secrets the unit's EnvironmentFile carries (mode 0600)"
  secret_key=$(openssl rand -hex 32)
  {
    echo "SR_CONFIG=$prefix/etc/sideroom/config.toml"
    echo "SR_SERVER__SECRET_KEY=$secret_key"
    echo "SR_DATABASE__URL=$SMOKE_DATABASE_URL"
    echo "SR_STORAGE__ACCESS_KEY=GK$(openssl rand -hex 12)"
    echo "SR_STORAGE__SECRET_KEY=$(openssl rand -hex 32)"
    echo "SR_SMTP__PASSWORD=$(openssl rand -hex 8)"
  } >"$prefix/etc/sideroom/server.env"
  chmod 600 "$prefix/etc/sideroom/server.env"

  log "sr-cli migrate and bootstrap (INSTALL.md steps 3 and 4)"
  (
    set -a
    # shellcheck disable=SC1091
    . "$prefix/etc/sideroom/server.env"
    set +a
    "$prefix/opt/sideroom/bin/sr-cli" migrate
    SR_OWNER_PASSWORD=$(openssl rand -hex 16) "$prefix/opt/sideroom/bin/sr-cli" bootstrap \
      --owner-username owner --owner-email owner@example.test >/dev/null
  )

  # The user-service copy of deploy/systemd/sr-server.service: same ExecStart, restart policy, timeout and the
  # syscall/address-family/W^X hardening; the lines that need root (User=, ProtectSystem, StateDirectory, ...) are dropped
  # and the paths move under the prefix.
  unit_src=$repo/deploy/systemd/sr-server.service
  unit_dir=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user
  mkdir -p "$unit_dir"
  sed -e '/^User=/d' -e '/^Group=/d' -e '/^StateDirectory=/d' -e '/^ReadWritePaths=/d' \
    -e '/^ProtectSystem=/d' -e '/^ProtectHome=/d' -e '/^PrivateTmp=/d' -e '/^PrivateDevices=/d' \
    -e '/^ProtectKernel/d' -e '/^ProtectControlGroups=/d' -e '/^RestrictNamespaces=/d' -e '/^LimitNOFILE=/d' \
    -e '/^After=/d' -e '/^Wants=/d' \
    -e "s|/etc/sideroom|$prefix/etc/sideroom|g" -e "s|/opt/sideroom|$prefix/opt/sideroom|g" \
    -e "s|/var/lib/sideroom|$prefix/var/lib/sideroom|g" -e 's|multi-user.target|default.target|' \
    "$unit_src" >"$unit_dir/sr-server.service"
  log "user unit differs from the system unit as follows"
  diff -u "$unit_src" "$unit_dir/sr-server.service" || true

  cleanup() {
    systemctl --user stop sr-server.service >/dev/null 2>&1 || true
    rm -f "$unit_dir/sr-server.service"
    systemctl --user daemon-reload >/dev/null 2>&1 || true
    [ -z "${fallback_pid:-}" ] || kill "$fallback_pid" 2>/dev/null || true
  }
  trap cleanup EXIT INT TERM

  if start_user_manager; then
    systemctl --user daemon-reload
    systemd-analyze --user verify "$unit_dir/sr-server.service" || die "the user unit does not verify"
    log "starting sr-server.service as a user service"
    systemctl --user start sr-server.service || { journalctl --user -u sr-server.service --no-pager -n 50 || true; die "systemctl --user start failed"; }
    proven="a systemd user service"
  elif [ "${SMOKE_SYSTEMD_USER_FALLBACK:-0}" = "1" ]; then
    echo "::warning::no systemd user manager here: sr-server runs from the unit's ExecStart without systemd. The unit is NOT proven."
    exec_line=$(sed -n 's/^ExecStart=//p' "$unit_dir/sr-server.service")
    (
      set -a
      # shellcheck disable=SC1091
      . "$prefix/etc/sideroom/server.env"
      set +a
      # shellcheck disable=SC2086
      exec $exec_line
    ) >"$work/fallback.log" 2>&1 &
    fallback_pid=$!
    proven="a plain process (fallback: NOT a systemd service)"
  else
    die "no systemd user manager could be started on this machine (set SMOKE_SYSTEMD_USER_FALLBACK=1 to run without it)"
  fi

  log "waiting for http://127.0.0.1:$port/healthz"
  i=0
  until curl --fail --silent "http://127.0.0.1:$port/healthz" >/dev/null 2>&1; do
    i=$((i + 1))
    if [ "$i" -ge 60 ]; then
      journalctl --user -u sr-server.service --no-pager -n 80 2>/dev/null || cat "$work/fallback.log" 2>/dev/null || true
      die "sr-server did not answer /healthz on port $port"
    fi
    sleep 1
  done
  log "healthz: $(curl --fail --silent "http://127.0.0.1:$port/healthz")"
  metrics=$(curl --silent --output /dev/null --write-out '%{http_code}' "http://127.0.0.1:$mport/internal/metrics" || true)
  echo "metrics listener on 127.0.0.1:$mport answers $metrics"
  # The gate is on (D-385): without a session an app path is a 302 to the sign-in page and an app file an empty 401.
  nav=$(curl --silent --output /dev/null --write-out '%{http_code} %{redirect_url}'     -H 'Accept: text/html' -H 'Sec-Fetch-Mode: navigate' "http://127.0.0.1:$port/channels" || true)
  case $nav in
    "302 "*"/login?next=/channels") log "gate: /channels without a session is $nav" ;;
    *) die "gate: /channels without a session answered '$nav', expected a 302 to /login?next=/channels" ;;
  esac
  file=$(curl --silent --output /dev/null --write-out '%{http_code}' "http://127.0.0.1:$port/app/assets/anything.js" || true)
  [ "$file" = "401" ] || die "gate: /app/assets/anything.js without a session answered $file, expected 401"
  log "gate: an app file without a session is 401"
  if [ "$proven" = "a systemd user service" ]; then
    [ "$(systemctl --user is-active sr-server.service)" = "active" ] || die "sr-server.service is not active"
    systemctl --user stop sr-server.service
    log "stopped by SIGTERM: $(systemctl --user show -p Result --value sr-server.service)"
  fi
  log "static-binary install ran as $proven"
}

# SEC-AUD-578 / D-1047. The plain-binary section of INSTALL.md as an operator runs it, as root on a real system manager, with
# deploy/systemd/sr-server.service copied UNCHANGED (the same `install -m 644` line as step 5 of the page): every hardening
# line stays. It passes only when the service is active, answers /healthz, runs as `sideroom`, and its search folder exists
# under /var/lib/sideroom owned by `sideroom` (created by StateDirectory=, written through ReadWritePaths=).
#
# Where this deviates from INSTALL.md (each is a finding about the page, reported to the commander):
#  - /etc/sideroom is mode 0750 root:sideroom and config.toml is 0640 root:sideroom. The page says 0700 for the folder, which
#    the service user (User=sideroom) cannot enter, so it could not read SR_CONFIG. server.env stays 0600 root: systemd reads
#    it as root before it drops privileges.
#  - Only the listener ports in config.toml move (SMOKE_PORT), as in the user-service mode.
#  - sr-cli bootstrap is allowed to refuse: the user-service step ran earlier against the same database.
# What this mode cannot prove: the runner's systemd version is the one in ubuntu-latest, not a customer's; Postgres is
# reachable on 127.0.0.1; object storage and SMTP are not contacted (/healthz is liveness, not /readyz).
stage_system_service() {
  if [ "${CI:-}" != "true" ] && [ "${SMOKE_ALLOW_SUDO:-0}" != "1" ]; then
    die "system-service installs a user, folders and a unit as root; set SMOKE_ALLOW_SUDO=1 to allow it"
  fi
  : "${SMOKE_SERVER_BIN:?set SMOKE_SERVER_BIN}"
  : "${SMOKE_CLI_BIN:?set SMOKE_CLI_BIN}"
  : "${SMOKE_DATABASE_URL:?set SMOKE_DATABASE_URL}"
  : "${SMOKE_WEB_DIR:?set SMOKE_WEB_DIR (the built apps/web/dist)}"
  [ -f "$SMOKE_WEB_DIR/app/csp.txt" ] || die "$SMOKE_WEB_DIR has no app/csp.txt: is it the built apps/web/dist?"
  need curl
  need openssl
  need systemctl
  need journalctl
  [ -d /run/systemd/system ] || die "this machine is not booted with systemd as its init: system-service cannot run here"
  mkdir -p "$work"
  tmp_env=$work/system-server.env
  tmp_pat=$work/system-secrets.pat
  made_user=0
  unit=/etc/systemd/system/sr-server.service

  # Journal and status lines go through this: it drops any line holding a value from server.env (the database URL, the
  # secret key, the storage and SMTP secrets) or a postgres:// URL, so a failure prints no secret.
  scrub() {
    awk -v pat="$tmp_pat" 'BEGIN { while ((getline p < pat) > 0) if (p != "") pats[++n] = p }
      { for (i = 1; i <= n; i++) if (index($0, pats[i])) next; if (index($0, "postgres://") || index($0, "postgresql://")) next; print }'
  }
  show_failure() {
    echo "---- systemctl status sr-server (filtered) ----"
    sudo_run systemctl status sr-server.service --no-pager -l 2>&1 | scrub || true
    echo "---- journalctl -u sr-server, last 80 lines (filtered) ----"
    sudo_run journalctl -u sr-server.service --no-pager -n 80 2>&1 | scrub || true
  }
  cleanup() {
    sudo_run systemctl stop sr-server.service >/dev/null 2>&1 || true
    sudo_run systemctl disable sr-server.service >/dev/null 2>&1 || true
    sudo_run rm -f /etc/systemd/system/sr-server.service
    sudo_run systemctl daemon-reload >/dev/null 2>&1 || true
    sudo_run systemctl reset-failed sr-server.service >/dev/null 2>&1 || true
    sudo_run rm -rf /etc/sideroom /opt/sideroom/web /var/lib/sideroom
    sudo_run rm -f /opt/sideroom/bin/sr-cli
    [ "$made_user" != "1" ] || sudo_run userdel sideroom >/dev/null 2>&1 || true
    rm -f "$tmp_env" "$tmp_pat"
  }
  trap cleanup EXIT INT TERM

  log "INSTALL.md plain binary, step 1: the sideroom user, /opt/sideroom/bin, the binaries, the web build"
  if ! getent passwd sideroom >/dev/null 2>&1; then
    sudo_run useradd --system --no-create-home --shell /usr/sbin/nologin sideroom
    made_user=1
  fi
  sudo_run install -d -m 755 /opt/sideroom/bin
  sudo_run install -m 755 "$SMOKE_SERVER_BIN" /opt/sideroom/bin/sr-server
  sudo_run install -m 755 "$SMOKE_CLI_BIN" /opt/sideroom/bin/sr-cli
  sudo_run rm -rf /opt/sideroom/web
  sudo_run install -d -m 755 /opt/sideroom/web
  sudo_run cp -R "$SMOKE_WEB_DIR"/. /opt/sideroom/web/
  sudo_run chmod -R u+rwX,go+rX /opt/sideroom/web
  sudo_run test -f /opt/sideroom/web/app/csp.txt || die "/opt/sideroom/web/app/csp.txt is missing after the copy"
  # The state folder must not exist yet: StateDirectory= has to be what creates it.
  sudo_run rm -rf /var/lib/sideroom
  sudo_run install -d -m 750 -o root -g sideroom /etc/sideroom

  log "step 2: /etc/sideroom/config.toml from the template in INSTALL.md section 4"
  config_template >"$work/system-config.toml"
  grep -q '^\[server\]' "$work/system-config.toml" || die "found no toml template in INSTALL.md"
  sed -i "s|^bind = \"127.0.0.1:8080\"|bind = \"127.0.0.1:$port\"|; s|^metrics_bind = \"127.0.0.1:9100\"|metrics_bind = \"127.0.0.1:$mport\"|" \
    "$work/system-config.toml"
  grep -q "127.0.0.1:$port" "$work/system-config.toml" || die "the template no longer has a bind line the smoke can move"
  sudo_run install -m 640 -o root -g sideroom "$work/system-config.toml" /etc/sideroom/config.toml
  rm -f "$work/system-config.toml"

  log "step 3: /etc/sideroom/server.env (mode 0600, root)"
  umask 077
  {
    echo "SR_CONFIG=/etc/sideroom/config.toml"
    echo "SR_SERVER__SECRET_KEY=$(openssl rand -hex 32)"
    echo "SR_DATABASE__URL=$SMOKE_DATABASE_URL"
    echo "SR_STORAGE__ACCESS_KEY=GK$(openssl rand -hex 12)"
    echo "SR_STORAGE__SECRET_KEY=$(openssl rand -hex 32)"
    echo "SR_SMTP__PASSWORD=$(openssl rand -hex 8)"
  } >"$tmp_env"
  # The values that scrub() drops from any output.
  sed -n '/^SR_CONFIG=/d; s/^SR_[A-Z_]*=//p' "$tmp_env" >"$tmp_pat"
  sudo_run install -m 600 -o root -g root "$tmp_env" /etc/sideroom/server.env

  log "step 4: sr-cli migrate and bootstrap, as root, with the same environment"
  out=$(sudo_run sh -c 'set -a; . /etc/sideroom/server.env; set +a; /opt/sideroom/bin/sr-cli migrate' 2>&1) && rc=0 || rc=$?
  printf '%s
' "$out" | scrub
  [ "$rc" = "0" ] || die "sr-cli migrate failed (exit $rc)"
  if sudo_run sh -c 'set -a; . /etc/sideroom/server.env; set +a
      SR_OWNER_PASSWORD=$(openssl rand -hex 16) /opt/sideroom/bin/sr-cli bootstrap --owner-username owner --owner-email owner@example.test' \
    >/dev/null 2>&1; then
    log "bootstrap created the Owner"
  else
    log "bootstrap refused (already bootstrapped by an earlier step against this database): fine"
  fi

  log "step 5: the unit, copied unchanged, then daemon-reload and start"
  sudo_run install -m 644 "$repo/deploy/systemd/sr-server.service" "$unit"
  cmp -s "$repo/deploy/systemd/sr-server.service" "$unit" || die "the installed unit differs from deploy/systemd/sr-server.service"
  sudo_run systemctl daemon-reload
  sudo_run systemctl start sr-server.service || { show_failure; die "systemctl start sr-server failed"; }

  log "waiting for http://127.0.0.1:$port/healthz"
  i=0
  until curl --fail --silent "http://127.0.0.1:$port/healthz" >/dev/null 2>&1; do
    i=$((i + 1))
    if [ "$i" -ge 90 ]; then
      show_failure
      die "sr-server (system service, hardening on) did not answer /healthz on port $port within 90 s"
    fi
    sleep 1
  done
  log "healthz: $(curl --fail --silent "http://127.0.0.1:$port/healthz")"

  log "asserting the sandbox was really on and the service is healthy"
  [ "$(sudo_run systemctl is-active sr-server.service)" = "active" ] || { show_failure; die "sr-server.service is not active"; }
  ps_val=$(sudo_run systemctl show -p ProtectSystem --value sr-server.service)
  [ "$ps_val" = "strict" ] || die "systemd reports ProtectSystem=$ps_val for the running unit, expected strict"
  main_pid=$(sudo_run systemctl show -p MainPID --value sr-server.service)
  run_user=$(ps -o user= -p "$main_pid" 2>/dev/null | tr -d ' ' || true)
  [ "$run_user" = "sideroom" ] || die "sr-server runs as '$run_user', expected sideroom"
  [ -d /var/lib/sideroom/search ] || { show_failure; die "/var/lib/sideroom/search does not exist: the server did not create its search folder"; }
  owner=$(sudo_run stat -c %U /var/lib/sideroom/search)
  [ "$owner" = "sideroom" ] || die "/var/lib/sideroom/search is owned by '$owner', expected sideroom"
  state_owner=$(sudo_run stat -c %U /var/lib/sideroom)
  [ "$state_owner" = "sideroom" ] || die "/var/lib/sideroom is owned by '$state_owner', expected sideroom (StateDirectory=)"
  if sudo_run journalctl -u sr-server.service --no-pager 2>&1 | scrub | grep -q "opening the search index"; then
    show_failure
    die "the journal holds an 'opening the search index' error"
  fi
  log "system service ok: active, runs as sideroom, ProtectSystem=strict, /var/lib/sideroom/search owned by sideroom, no search-index error"

  sudo_run systemctl stop sr-server.service
  log "stopped by SIGTERM: $(sudo_run systemctl show -p Result --value sr-server.service)"
  log "the plain-binary install ran as a systemd SYSTEM service with the unit's hardening on"
}

case ${1:-} in
  verify) stage_verify ;;
  user-service) stage_user_service ;;
  system-service) stage_system_service ;;
  *) die "usage: smoke-systemd.sh verify|user-service|system-service" ;;
esac
