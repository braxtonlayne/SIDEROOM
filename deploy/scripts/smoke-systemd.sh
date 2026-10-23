#!/bin/sh
# The systemd route, checked on a Linux machine (RDY-OPS-003): the unit files pass `systemd-analyze verify`, and the
# static-binary install of deploy/INSTALL.md (sections 3 to 5) works end to end: the binaries go under a prefix,
# /etc/sideroom/config.toml comes from the template in INSTALL.md section 4, secrets go in server.env, sr-cli migrates
# and bootstraps, and sr-server runs as a systemd USER service long enough to answer /healthz.
# .github/workflows/prod-smoke.yml runs both stages on an Ubuntu runner.
#
#   deploy/scripts/smoke-systemd.sh verify         systemd-analyze verify over deploy/systemd/*
#   deploy/scripts/smoke-systemd.sh user-service   the static-binary install under a prefix, started as a user service
#
#   SR_SERVER, SR_CLI            the static binaries (the musl ones that ship in the server image)
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
  : "${SR_SERVER:?set SR_SERVER}"
  getent passwd sideroom >/dev/null 2>&1 || sudo_run useradd --system --no-create-home --shell /usr/sbin/nologin sideroom
  sudo_run install -d -m 755 /opt/sideroom/bin
  sudo_run install -m 755 "$SR_SERVER" /opt/sideroom/bin/sr-server
  # The units run /opt/sideroom/deploy/scripts/*.sh and name /opt/sideroom/deploy/compose/prod.yml.
  sudo_run ln -sfn "$repo/deploy" /opt/sideroom/deploy
  systemd-analyze --version | head -n 1
  log "systemd-analyze verify deploy/systemd/*"
  # No --man=no: a unit with a misspelt key is the point of the exercise. Warnings count as failures.
  out=$(systemd-analyze verify "$repo"/deploy/systemd/*.service "$repo"/deploy/systemd/*.timer 2>&1) && rc=0 || rc=$?
  [ -z "$out" ] || echo "$out"
  [ "$rc" = "0" ] || die "systemd-analyze verify failed (exit $rc)"
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

stage_user_service() {
  : "${SR_SERVER:?set SR_SERVER}"
  : "${SR_CLI:?set SR_CLI}"
  : "${SMOKE_DATABASE_URL:?set SMOKE_DATABASE_URL}"
  need curl
  need openssl
  rm -rf "$prefix"
  umask 077
  mkdir -p "$prefix/opt/sideroom/bin" "$prefix/var/lib/sideroom"
  mkdir -p "$prefix/etc/sideroom"
  chmod 700 "$prefix/etc/sideroom" || true
  install -m 755 "$SR_SERVER" "$prefix/opt/sideroom/bin/sr-server"
  install -m 755 "$SR_CLI" "$prefix/opt/sideroom/bin/sr-cli"

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
  if [ "$proven" = "a systemd user service" ]; then
    [ "$(systemctl --user is-active sr-server.service)" = "active" ] || die "sr-server.service is not active"
    systemctl --user stop sr-server.service
    log "stopped by SIGTERM: $(systemctl --user show -p Result --value sr-server.service)"
  fi
  log "static-binary install ran as $proven"
}

case ${1:-} in
  verify) stage_verify ;;
  user-service) stage_user_service ;;
  *) die "usage: smoke-systemd.sh verify|user-service" ;;
esac
