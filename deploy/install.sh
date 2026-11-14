#!/bin/sh
# SIDE ROOM installer for one host running Docker Compose (XC-OPS-002).
#
#   deploy/install.sh [--yes] [--force] [--no-start]
#
# It asks for the handful of things only you know (domain, mail server, the Owner account), generates every
# secret fresh, writes deploy/compose/.env.prod and garage.prod.toml (mode 0600), then brings the stack up in
# the right order: Postgres and Garage, the bucket and its key, migrations, the CORS rule, the Owner account,
# then the server and Caddy, and last `sr-cli preflight`, whose table says whether the box is ready for its first
# invite (RDY-OPS-004).
#
# Run it again after a run that did not finish (a failed pull, a wrong Owner password, a missing web build) and it
# RESUMES (D-747): the two files are read, not rewritten, no secret is generated or changed, and every step runs again
# (each is safe to repeat; only the Owner account is detected as done). With --force it starts over instead.
#
#   --yes        take every answer from the environment or the default; never prompt
#   --force      write new files with NEW secrets, for a machine whose data volumes are gone. The files it replaces
#                are kept as <file>.bak.<UTC date and time>, never overwritten. Do not use it to finish a run
#                that stopped part way: the database volume already holds the old password
#   --no-start   write the files and print the commands instead of running them
#
# Answers can come from the environment (they win over prompts):
#   SR_DOMAIN SR_IMAGE SR_WEB_DIR SMTP_HOST SMTP_PORT SMTP_TLS SMTP_FROM SMTP_USERNAME SMTP_PASSWORD
#   SR_OWNER_USERNAME SR_OWNER_EMAIL SR_OWNER_PASSWORD SR_GARAGE_CAPACITY SR_INGEST_DIR
#   SR_INSTALL_EMU_CORES (yes fetches the emulator cores into $SR_WEB_DIR/emu/cores by pinned URL and SHA-256, deploy/EMULATOR.md;
#   default no)
#   SR_PAYMENTS_ELECTRS (yes turns the electrs `payments` profile on) SR_PAYMENTS_ESPLORA_URL (empty = donations off)
#   SR_INSTALL_SKIP_PULL=1 (skip `docker compose pull` when the image was built or loaded locally; compose still
#   pulls whatever is missing when it starts the services. Used by .github/workflows/prod-smoke.yml)
#   SR_INSTALL_PREFLIGHT=warn (default: print the preflight table; a FAIL row is reported, the install still
#   succeeds), strict (a FAIL row makes the installer exit 3) or skip. SR_INSTALL_PREFLIGHT_ARGS: extra arguments for
#   `sr-cli preflight`, for example --insecure-tls when Caddy serves a private CA, or --send-to you@example.org
#
# The built web app must already be in $SR_WEB_DIR (apps/web/dist: it holds app/csp.txt): the server refuses to start
# without it, so the installer stops before it writes or starts anything when the file is missing.
#
# Needs Docker with the compose plugin and a source of randomness (openssl, or /dev/urandom with od).
set -eu

here=$(cd "$(dirname "$0")" && pwd)
compose_dir=$here/compose
env_file=$compose_dir/.env.prod
garage_file=$compose_dir/garage/garage.prod.toml

# The shortest Owner password `sr-cli bootstrap` takes (MIN_PASSWORD_LEN in crates/sr-cli/src/commands.rs; a test
# keeps the two equal). The installer checks it BEFORE it writes a file; the longer strength rule stays in sr-cli.
MIN_OWNER_PASSWORD=10

die() { echo "error: $*" >&2; exit 1; }
# A run that stops (a refused answer, a failed step) leaves no half-written temporary file behind.
trap 'rm -f "$env_file.new" "$env_file.new.part" "$garage_file.new"' EXIT
say() { echo "==> $*"; }

yes=0
force=0
start=1
for arg in "$@"; do
  case $arg in
    --yes|--non-interactive) yes=1 ;;
    --force) force=1 ;;
    --no-start) start=0 ;;
    -h|--help) sed -n '2,37p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option $arg (try --help)" ;;
  esac
done

case ${SR_INSTALL_PREFLIGHT:-warn} in
  warn|strict|skip) ;;
  *) die "SR_INSTALL_PREFLIGHT must be warn, strict or skip (got '${SR_INSTALL_PREFLIGHT:-}')" ;;
esac

# --- host checks ----------------------------------------------------------------------------------------
if [ "${SR_INSTALL_SKIP_CHECKS:-0}" != "1" ]; then
  command -v docker >/dev/null 2>&1 || die "docker is not installed (https://docs.docker.com/engine/install/)"
  docker compose version >/dev/null 2>&1 || die "the docker compose plugin is missing (docker compose version failed)"
fi
if command -v openssl >/dev/null 2>&1; then
  rand_hex() { openssl rand -hex "$1"; }
elif [ -r /dev/urandom ] && command -v od >/dev/null 2>&1; then
  rand_hex() { od -An -N "$1" -tx1 /dev/urandom | tr -d ' \n'; }
else
  die "no source of randomness: install openssl"
fi
[ -f "$compose_dir/.env.prod.example" ] || die "$compose_dir/.env.prod.example not found; run this from a complete checkout or release bundle"

# --- an earlier run (D-747) -----------------------------------------------------------------------------
# Files from an earlier run and no --force: resume with what is in them. Nothing below writes them again.
resume=0
if [ -f "$env_file" ] || [ -f "$garage_file" ]; then
  if [ "$force" = "1" ]; then
    echo "warning: --force is only for a machine whose data volumes are gone. It makes a NEW database password and new keys; an existing database or bucket volume will not accept them. Every file it replaces is kept as <file>.bak.<UTC date and time>. To finish a run that stopped part way, run this script again without --force." >&2
  elif [ -f "$env_file" ]; then
    resume=1
  else
    die "$garage_file exists but $env_file does not. Put the .env.prod of that install back, or pass --force on a machine whose data volumes are gone."
  fi
fi

# The value of KEY in the env file, as written by `envq` below (single quotes taken off; a file an earlier installer
# wrote leaves most values bare). The file is read, never sourced: a value is data, not shell.
read_env() {
  _line=$(sed -n "s/^$1=//p" "$env_file" | head -n 1)
  case $_line in
    \'*\') _line=${_line#\'}; _line=${_line%\'} ;;
  esac
  printf '%s' "$_line"
}

# Take KEY from the env file of the earlier run; an answer given now that differs is ignored, said once.
adopt() {
  grep -q "^$1=" "$env_file" || die "$env_file has no $1 line: it was not written by this installer. Fix it by hand, or pass --force on a machine whose data volumes are gone."
  _have=$(read_env "$1")
  _given=
  eval "_given=\${$1:-}"
  if [ -n "$_given" ] && [ "$_given" != "$_have" ]; then
    echo "warning: $1 is already $_have in $env_file; the value given now is ignored (edit the file to change it)." >&2
  fi
  eval "$1=\$_have"
}

# --- answers --------------------------------------------------------------------------------------------
# ask VAR "question" default [secret]: environment first, then a prompt (unless --yes), then the default.
ask() {
  var=$1
  question=$2
  default=${3:-}
  secret=${4:-}
  current=
  eval "current=\${$var:-}"
  if [ -n "$current" ]; then return 0; fi
  if [ "$yes" = "1" ] || [ ! -t 0 ]; then
    eval "$var=\$default"
    return 0
  fi
  if [ -n "$default" ]; then prompt="$question [$default]: "; else prompt="$question: "; fi
  printf '%s' "$prompt" >&2
  if [ -n "$secret" ]; then stty -echo 2>/dev/null || true; fi
  read -r answer || answer=
  if [ -n "$secret" ]; then stty echo 2>/dev/null || true; echo >&2; fi
  if [ -z "$answer" ]; then answer=$default; fi
  eval "$var=\$answer"
}

payments_profile=0
if [ "$resume" = "1" ]; then
  say "resuming: $env_file is already written; it is read, not rewritten, and no secret is generated or changed"
  adopt SR_DOMAIN
  adopt SR_WEB_DIR
  adopt SR_INGEST_DIR
  adopt SR_INGEST_ROOTS
  ingest_roots=$SR_INGEST_ROOTS
else
  ask SR_DOMAIN "Public hostname (DNS must already point at this host)" ""
  ask SR_IMAGE "Server image" "$(sed -n 's/^SR_IMAGE=//p' "$compose_dir/.env.prod.example")"
  ask SR_WEB_DIR "Directory holding the built web app" "/opt/sideroom/web"
  ask SMTP_HOST "SMTP host" ""
  ask SMTP_PORT "SMTP port" "587"
  ask SMTP_TLS "SMTP security (starttls, tls or none)" "starttls"
  ask SMTP_USERNAME "SMTP username (empty for none)" ""
  ask SMTP_PASSWORD "SMTP password" "" secret
  ask SR_INGEST_DIR "Folder for local imports, mounted read-only (type none to turn local folders off)" "/srv/sideroom/ingest"
  ask SR_PAYMENTS_ELECTRS "Run the optional electrs container for donations? It needs your own synced bitcoind and a pinned image, see docs/12 (yes/no)" "no"
  SMTP_USERNAME=${SMTP_USERNAME:-}
  SMTP_PASSWORD=${SMTP_PASSWORD:-}
  if [ -z "${SMTP_FROM:-}" ]; then SMTP_FROM="SIDE ROOM <no-reply@${SR_DOMAIN:-example.org}>"; fi
  # Donations (D-288): the electrs profile gives the Esplora URL its container address; otherwise an operator-given URL
  # (for example the interim https://mempool.space/api) is kept and an empty one leaves donations off.
  case ${SR_PAYMENTS_ELECTRS:-no} in
    yes|y|true|1) payments_profile=1; SR_PAYMENTS_ESPLORA_URL=${SR_PAYMENTS_ESPLORA_URL:-http://electrs:3000}; payments_private=1 ;;
    *) payments_profile=0; SR_PAYMENTS_ESPLORA_URL=${SR_PAYMENTS_ESPLORA_URL:-}; payments_private= ;;
  esac
fi
ask SR_OWNER_USERNAME "Owner username" "owner"
ask SR_OWNER_EMAIL "Owner email" ""
ask SR_GARAGE_CAPACITY "Object storage capacity to reserve (for example 100G)" "100G"
ask SR_INSTALL_EMU_CORES "Download the emulator cores now (about 8 MB from cdn.emulatorjs.org, checked against pinned SHA-256 digests, see deploy/EMULATOR.md)? (yes/no)" "no"

# --- validation -----------------------------------------------------------------------------------------
case ${SR_DOMAIN:-} in
  ''|*[!A-Za-z0-9.-]*|.*|*.|*..*) die "SR_DOMAIN must be a hostname such as chat.example.org (got '${SR_DOMAIN:-}')" ;;
  *.*) ;;
  *) die "SR_DOMAIN must contain a dot (got '$SR_DOMAIN')" ;;
esac
case ${SR_OWNER_EMAIL:-} in *?@?*.*) ;; *) die "SR_OWNER_EMAIL must be an email address" ;; esac
case ${SR_OWNER_USERNAME:-} in ''|*[!a-z0-9_.]*) die "SR_OWNER_USERNAME may use a-z, 0-9, '_' and '.'" ;; esac
case $SR_GARAGE_CAPACITY in [0-9]*[KMGT]|[0-9]*[KMGT]B) ;; *) die "SR_GARAGE_CAPACITY must look like 100G" ;; esac
if [ "$resume" != "1" ]; then
  [ -n "${SMTP_HOST:-}" ] || die "SMTP_HOST is required: the server mails sign-up and password links"
  case $SMTP_PORT in ''|*[!0-9]*) die "SMTP_PORT must be a number (got '$SMTP_PORT')" ;; esac
  case $SMTP_TLS in
    starttls|tls) ;;
    none) echo "warning: SMTP_TLS=none sends mail and the SMTP password unencrypted; use it only with a local relay." >&2 ;;
    *) die "SMTP_TLS must be starttls, tls or none (got '$SMTP_TLS')" ;;
  esac
  case ${SR_IMAGE:-} in ''|*' '*) die "SR_IMAGE must be an image reference" ;; esac
  # Local ingest folder: mounted read-only at /srv/sideroom/ingest; "none" turns local folders off.
  if [ "$SR_INGEST_DIR" = "none" ]; then
    SR_INGEST_DIR=/srv/sideroom/ingest
    ingest_roots=
  else
    ingest_roots=/srv/sideroom/ingest
  fi
fi
case $SR_INGEST_DIR in
  /*[!A-Za-z0-9._/-]*|*/../*|*/..) die "SR_INGEST_DIR must be an absolute path of letters, digits, '.', '_', '-' and '/' (got '$SR_INGEST_DIR')" ;;
  /*) ;;
  *) die "SR_INGEST_DIR must be an absolute path (got '$SR_INGEST_DIR')" ;;
esac
# Every value an env file gets is single-quoted, so none may hold a single quote or a line break. The message names
# the answer, so the one to change is not a guess.
check_plain() {
  eval "_v=\${$1:-}"
  case $_v in
    *"'"*) die "$1 may not contain a single quote" ;;
    *'
'*) die "$1 must be a single line" ;;
  esac
}
if [ "$resume" != "1" ]; then
  for name in SR_DOMAIN SR_IMAGE SR_WEB_DIR SMTP_HOST SMTP_PORT SMTP_TLS SMTP_FROM SMTP_USERNAME SMTP_PASSWORD \
    SR_INGEST_DIR SR_PAYMENTS_ESPLORA_URL; do
    check_plain "$name"
  done
fi

# The Owner's password is needed only when the stack is started. It is asked for (or made) and checked BEFORE a file
# is written, with the rule sr-cli enforces for length: a rejected password no longer leaves a half-made install.
generated_password=
if [ "$start" = "1" ]; then
  if [ -z "${SR_OWNER_PASSWORD:-}" ] && [ "$yes" != "1" ] && [ -t 0 ]; then
    ask SR_OWNER_PASSWORD "Owner password (at least $MIN_OWNER_PASSWORD characters)" "" secret
  fi
  if [ -z "${SR_OWNER_PASSWORD:-}" ]; then
    SR_OWNER_PASSWORD=$(rand_hex 12)
    generated_password=$SR_OWNER_PASSWORD
  fi
  # ${#x} counts bytes in some shells, characters in others: a longer count never rejects a good password, and
  # sr-cli still counts characters.
  if [ "${#SR_OWNER_PASSWORD}" -lt "$MIN_OWNER_PASSWORD" ]; then
    die "password must be at least $MIN_OWNER_PASSWORD characters"
  fi
  # The web app is checked before anything is written or started: the server stops at start without it.
  [ -f "$SR_WEB_DIR/app/csp.txt" ] || die "the web build is not in $SR_WEB_DIR (it has no app/csp.txt). Put the contents of apps/web/dist from the release there, then run this script again: the server refuses to start without it."
fi

# --- files ----------------------------------------------------------------------------------------------
umask 077

# Compose env files expand $VAR and treat " #" as a comment; single quotes make a value literal. EVERY value gets
# them (D-747): a mail password with ';', '(', '&' or a backtick is then data for compose and for a shell alike.
envq() {
  case $2 in
    *"'"*) echo "error: $1 may not contain a single quote" >&2; exit 1 ;;
  esac
  printf "'%s'" "$2"
}

# Set KEY=VALUE in FILE through a temporary file, one checked step at a time (a pipe of awks reports only its last
# stage). The key and value travel in the environment, not as awk -v arguments, so backslashes, quotes and '&' stay
# literal.
fill() {
  _q=$(envq "$2" "$3") || exit 1
  FILL_KEY=$2 FILL_VAL=$_q awk 'BEGIN { FS = "=" }
    $1 == ENVIRON["FILL_KEY"] { print ENVIRON["FILL_KEY"] "=" ENVIRON["FILL_VAL"]; hit = 1; next }
    { print }
    END { if (!hit) exit 3 }' "$1" >"$1.part" || { rm -f "$1.part"; die "could not set $2 in the env template (is the key missing from .env.prod.example?)"; }
  mv "$1.part" "$1"
}

# Keep a file a --force run replaces: <file>.bak.<UTC yyyymmddThhmmssZ>, mode 600, never over an earlier backup.
backup_file() {
  [ -f "$1" ] || return 0
  _stamp=$(date -u +%Y%m%dT%H%M%SZ)
  _bak=$1.bak.$_stamp
  _n=1
  while [ -e "$_bak" ]; do _bak=$1.bak.$_stamp-$_n; _n=$((_n + 1)); done
  cp -p "$1" "$_bak"
  chmod 600 "$_bak"
  say "kept the old $(basename "$1") as $_bak"
}

# garage.prod.toml: three CHANGE_ME values, in file order rpc_secret, admin_token, metrics_token.
write_garage() {
  rpc=$(rand_hex 32)
  admin=$(rand_hex 32)
  metrics=$(rand_hex 32)
  awk -v rpc="$rpc" -v admin="$admin" -v metrics="$metrics" '
    /^rpc_secret = / { print "rpc_secret = \"" rpc "\""; next }
    /^admin_token = / { print "admin_token = \"" admin "\""; next }
    /^metrics_token = / { print "metrics_token = \"" metrics "\""; next }
    { print }' "$compose_dir/garage/garage.prod.toml.example" >"$garage_file.new" || { rm -f "$garage_file.new"; die "could not write garage.prod.toml"; }
  if grep -Eq "^[A-Za-z_]+ *=.*CHANGE_ME" "$garage_file.new"; then rm -f "$garage_file.new"; die "a CHANGE_ME placeholder is left in garage.prod.toml"; fi
  chmod 600 "$garage_file.new"
}

if [ "$resume" != "1" ]; then
  postgres_password=$(rand_hex 24)
  secret_key=$(rand_hex 32)
  garage_access="GK$(rand_hex 12)"
  garage_secret=$(rand_hex 32)
  grafana_password=$(rand_hex 12)

  tmp=$env_file.new
  rm -f "$tmp"
  cp "$compose_dir/.env.prod.example" "$tmp"
  fill "$tmp" SR_DOMAIN "$SR_DOMAIN"
  fill "$tmp" SR_IMAGE "$SR_IMAGE"
  fill "$tmp" SR_WEB_DIR "$SR_WEB_DIR"
  fill "$tmp" POSTGRES_PASSWORD "$postgres_password"
  fill "$tmp" SR_SECRET_KEY "$secret_key"
  fill "$tmp" GARAGE_ACCESS_KEY "$garage_access"
  fill "$tmp" GARAGE_SECRET_KEY "$garage_secret"
  fill "$tmp" SMTP_HOST "$SMTP_HOST"
  fill "$tmp" SMTP_PORT "$SMTP_PORT"
  fill "$tmp" SMTP_TLS "$SMTP_TLS"
  fill "$tmp" SMTP_FROM "$SMTP_FROM"
  fill "$tmp" SMTP_USERNAME "$SMTP_USERNAME"
  fill "$tmp" SMTP_PASSWORD "$SMTP_PASSWORD"
  fill "$tmp" SR_INGEST_DIR "$SR_INGEST_DIR"
  fill "$tmp" SR_INGEST_ROOTS "$ingest_roots"
  fill "$tmp" SR_PAYMENTS_ESPLORA_URL "$SR_PAYMENTS_ESPLORA_URL"
  fill "$tmp" SR_PAYMENTS_ALLOW_PRIVATE_HOSTS "$payments_private"
  fill "$tmp" GRAFANA_PASSWORD "$grafana_password"
  if grep -Eq "^[A-Za-z_]+ *=.*CHANGE_ME" "$tmp"; then rm -f "$tmp"; die "a CHANGE_ME placeholder is left in the generated env file"; fi
  if [ "$payments_profile" = "1" ]; then
    printf 'COMPOSE_PROFILES=payments\n' >>"$tmp"
    echo "warning: the payments profile is on. Pin the electrs image by digest in deploy/compose/prod.yml and point SR_ELECTRS_DAEMON_RPC_ADDR / SR_BITCOIND_DIR at your bitcoind before the stack can start it." >&2
  fi
  chmod 600 "$tmp"
  write_garage
  # Only now is anything of an earlier install touched: its files are kept first (--force).
  backup_file "$env_file"
  backup_file "$garage_file"
  mv "$tmp" "$env_file"
  mv "$garage_file.new" "$garage_file"
  say "wrote $env_file and $garage_file (mode 0600)"
elif [ ! -f "$garage_file" ]; then
  # An earlier run stopped between the two files; Garage has not started without its config, so the tokens are new.
  write_garage
  mv "$garage_file.new" "$garage_file"
  say "wrote $garage_file (mode 0600); $env_file was already there"
fi

# The ingest folder is only read; create it when missing so the bind mount has a source.
if [ -n "$ingest_roots" ] && [ ! -d "$SR_INGEST_DIR" ]; then
  if mkdir -p "$SR_INGEST_DIR" 2>/dev/null; then
    say "created $SR_INGEST_DIR (read-only inside the container; put files under $SR_INGEST_DIR/library/)"
  else
    echo "warning: could not create $SR_INGEST_DIR; create it before starting the stack (mkdir -p), or compose creates it root-owned." >&2
  fi
fi

# --- bring the stack up ---------------------------------------------------------------------------------
dc() { docker compose -f "$compose_dir/prod.yml" --env-file "$env_file" "$@"; }
sr_cli() { dc run --rm --no-deps --entrypoint /usr/local/bin/sr-cli sr-server "$@"; }

if [ "$start" != "1" ]; then
  cat <<EOF

Not started (--no-start). To bring the stack up by hand:

  cd $compose_dir
  docker compose -f prod.yml --env-file .env.prod pull
  docker compose -f prod.yml --env-file .env.prod up -d --wait postgres
  docker compose -f prod.yml --env-file .env.prod up -d garage
  SR_COMPOSE_FILE=$compose_dir/prod.yml SR_ENV_FILE=$env_file SR_GARAGE_CAPACITY=$SR_GARAGE_CAPACITY SR_SKIP_CORS=1 $compose_dir/garage/bootstrap.sh
  docker compose -f prod.yml --env-file .env.prod run --rm --no-deps --entrypoint /usr/local/bin/sr-cli sr-server migrate
  docker compose -f prod.yml --env-file .env.prod run --rm --no-deps --entrypoint /usr/local/bin/sr-cli sr-server storage ensure-cors --origin https://$SR_DOMAIN
  SR_OWNER_PASSWORD=... docker compose -f prod.yml --env-file .env.prod run --rm --no-deps -e SR_OWNER_PASSWORD --entrypoint /usr/local/bin/sr-cli sr-server bootstrap --owner-username $SR_OWNER_USERNAME --owner-email $SR_OWNER_EMAIL
  docker compose -f prod.yml --env-file .env.prod up -d
  docker compose -f prod.yml --env-file .env.prod run --rm --no-deps --entrypoint /usr/local/bin/sr-cli sr-server preflight
EOF
  exit 0
fi

# Every step below is safe to run again, which is what makes a resumed run work (D-747):
#   pull                  a second pull of an image that has not changed downloads nothing
#   Postgres, Garage      `up -d` leaves a running container as it is; `--wait` returns at once when healthy
#   the Garage bootstrap  assigns the layout only when the node has none, imports the key and creates the bucket only
#                         when they are missing, and re-grants the permission (garage/bootstrap.sh)
#   migrate               applies only the migrations not yet recorded
#   ensure-cors           replaces the bucket's rule with the same rule
#   the Owner account     the one step with state: `sr-cli bootstrap` refuses a finished install, and that refusal is
#                         read as "done" below (a half-made one is finished by sr-cli itself when the same username,
#                         email and password are given)
#   starter packs         "safe to repeat" (package seed)
#   the final `up -d`, the health wait and the preflight only read or start what is not running
if [ "${SR_INSTALL_SKIP_PULL:-0}" = "1" ]; then
  say "skipping the image pull (SR_INSTALL_SKIP_PULL=1)"
else
  say "pulling images"
  dc pull
fi
say "starting Postgres and Garage"
dc up -d --wait postgres
dc up -d garage
say "creating the storage layout, key and bucket"
SR_COMPOSE_FILE=$compose_dir/prod.yml SR_ENV_FILE=$env_file SR_GARAGE_CAPACITY=$SR_GARAGE_CAPACITY SR_SKIP_CORS=1 \
  "$compose_dir/garage/bootstrap.sh"
say "applying migrations"
sr_cli migrate
say "setting the bucket CORS rule for https://$SR_DOMAIN"
sr_cli storage ensure-cors --origin "https://$SR_DOMAIN"

say "creating the Owner account"
if [ -n "$generated_password" ]; then
  echo "no Owner password given; generated one. It is shown once below." >&2
fi
export SR_OWNER_PASSWORD
owner_rc=0
owner_out=$(dc run --rm --no-deps -e SR_OWNER_PASSWORD --entrypoint /usr/local/bin/sr-cli sr-server \
  bootstrap --owner-username "$SR_OWNER_USERNAME" --owner-email "$SR_OWNER_EMAIL" 2>&1) || owner_rc=$?
[ -z "$owner_out" ] || printf '%s\n' "$owner_out"
if [ "$owner_rc" != "0" ]; then
  case $owner_out in
    *"is not finished"*) die "the Owner account is half made and the answers given now are not the ones of the first run; give the same SR_OWNER_USERNAME, SR_OWNER_EMAIL and SR_OWNER_PASSWORD (see the message above) and run this script again" ;;
    *"already bootstrapped"*)
      if [ "$resume" = "1" ]; then
        say "the Owner account already exists (sr-cli bootstrap refused a second one): going on"
        generated_password=
      else
        die "this database already has accounts, so a first install cannot create the Owner. Use the database this install was made for, or an empty one."
      fi
      ;;
    *) die "sr-cli bootstrap failed (see above); fix the cause and run this script again: it resumes" ;;
  esac
fi

# Starter packs that ship with the release (packages/starter-packs/<name>, D-289): installed as approved
# packages and as the default sticker pack. Safe to repeat; a release without the folder just skips this.
packs_dir=$here/../packages/starter-packs
if [ -d "$packs_dir" ]; then
  packs_dir=$(cd "$packs_dir" && pwd)
  for pack in "$packs_dir"/*/; do
    [ -f "$pack/pack.json" ] || continue
    name=$(basename "$pack")
    say "installing the starter pack $name"
    dc run --rm --no-deps -v "$packs_dir:/starter-packs:ro" --entrypoint /usr/local/bin/sr-cli sr-server \
      package seed "/starter-packs/$name"
  done
fi

say "starting the server and Caddy"
dc up -d

# --- preflight (RDY-OPS-004) ------------------------------------------------------------------------------
# The same checks as Admin > System > Preflight, from a one-off container on the compose network. It waits for
# sr-server to report healthy first, so the public URL row sees the running server behind Caddy.
preflight_mode=${SR_INSTALL_PREFLIGHT:-warn}
preflight_status=0
case $preflight_mode in
  skip) say "skipping sr-cli preflight (SR_INSTALL_PREFLIGHT=skip)" ;;
  warn|strict)
    say "waiting for sr-server to report healthy"
    waited=0
    wait_secs=${SR_INSTALL_WAIT_SECS:-180}
    while :; do
      cid=$(dc ps -q sr-server 2>/dev/null || true)
      state=starting
      if [ -n "$cid" ]; then
        state=$(docker inspect -f '{{.State.Health.Status}}' "$cid" 2>/dev/null || echo starting)
      fi
      [ "$state" = "healthy" ] && break
      if [ "$waited" -ge "$wait_secs" ]; then
        echo "warning: sr-server is still '$state' after ${wait_secs}s; running the preflight anyway" >&2
        break
      fi
      sleep 3
      waited=$((waited + 3))
    done
    say "sr-cli preflight"
    # shellcheck disable=SC2086 # the extra arguments are meant to split
    sr_cli preflight ${SR_INSTALL_PREFLIGHT_ARGS:-} || preflight_status=$?
    ;;
  *) die "SR_INSTALL_PREFLIGHT must be warn, strict or skip (got '$preflight_mode')" ;;
esac

# --- optional: emulator cores (ACT-EMU-003) ---------------------------------------------------------------
# Fetched by pinned URL and SHA-256 into the web directory (they are never part of the release bundle). A failure is a
# warning: the rest of the install is done and the command below can be repeated.
case ${SR_INSTALL_EMU_CORES:-no} in
  yes|y|true|1)
    say "fetching the emulator cores into $SR_WEB_DIR/emu/cores"
    sh "$here/scripts/fetch-emu-cores.sh" --dest "$SR_WEB_DIR/emu/cores" ||
      echo "warning: the emulator cores were not installed; repeat with: deploy/scripts/fetch-emu-cores.sh --dest $SR_WEB_DIR/emu/cores" >&2
    ;;
esac

cat <<EOF

SIDE ROOM is starting at https://$SR_DOMAIN (Caddy fetches the certificate on the first request; DNS and
ports 80/443 must reach this host).

Keep these safe, they cannot be recovered:
  $env_file      (SR_SECRET_KEY decrypts every stored TOTP secret; back it up with the database dumps)
  $garage_file
EOF
if [ -n "${generated_password:-}" ]; then
  echo "Owner password (change it after signing in): $generated_password"
fi
cat <<EOF

Next: set up the nightly backups (deploy/BACKUP.md) and check https://$SR_DOMAIN/healthz. The web app in $SR_WEB_DIR
is the contents of apps/web/dist from the release; replace it there when you upgrade.
EOF
if [ "$preflight_status" != "0" ]; then
  cat >&2 <<EOF

warning: sr-cli preflight reported a FAIL row (table above). The stack is installed, but fix those rows before the
first invite (deploy/GO-LIVE.md), then run it again:
  cd $compose_dir && docker compose -f prod.yml --env-file .env.prod run --rm --no-deps --entrypoint /usr/local/bin/sr-cli sr-server preflight
EOF
  if [ "$preflight_mode" = "strict" ]; then exit 3; fi
fi
