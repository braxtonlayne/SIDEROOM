#!/bin/sh
# Production stack smoke (RDY-OPS-001 and RDY-OPS-002): installs deploy/compose/prod.yml with deploy/install.sh
# on a clean Docker host, waits for it through Caddy's internal CA, and runs the backup, restore and WAL drills
# against the running containers. .github/workflows/prod-smoke.yml calls the stages in order; each one can be run
# by hand on a Linux machine with Docker, Node 22 or newer and (for cli-backups) the PostgreSQL 17 client tools.
#
#   deploy/scripts/prod-smoke.sh install       install.sh --yes against prod.yml for https://$SMOKE_DOMAIN
#   deploy/scripts/prod-smoke.sh wait          Caddy root CA, /healthz through Caddy, headers, readiness probe
#   deploy/scripts/prod-smoke.sh migrate       sr-cli migrate (again: it must be a no-op) and the access-log check
#   deploy/scripts/prod-smoke.sh signin        Owner sign-in through Caddy and Admin > System (smoke-admin.mjs)
#   deploy/scripts/prod-smoke.sh backups       backup.sh, basebackup (the systemd command), restore.sh, restore-drill.sh,
#                                              wal-drill.sh and a live WAL segment, then the Admin > System backup time
#   deploy/scripts/prod-smoke.sh cli-backups   sr-cli upgrade --dry-run, backup, restore, restore --force on the host
#   deploy/scripts/prod-smoke.sh logs          container logs and state, for a failed run
#
#   SMOKE_IMAGE     server image already built or loaded on this host (install)
#   SMOKE_WEB_DIR   absolute path of the built web app, apps/web/dist (install)
#   SMOKE_DOMAIN    default smoke.localhost: Caddy issues .localhost names from its own CA, and the name has a dot
#                   as install.sh requires. install adds it to /etc/hosts (sudo) so browsers and curl resolve it.
#   SMOKE_SKIP_HOSTS=1  do not touch /etc/hosts (the name already resolves)
#   SMOKE_WORK      scratch folder for the Owner password, the CA certificate and the host-side dumps
#   SR_CLI          sr-cli binary for cli-backups (the static one taken out of the image)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
. "$here/lib.sh"

repo=$(cd "$here/../.." && pwd)
compose_dir=$repo/deploy/compose
env_file=$compose_dir/.env.prod
domain=${SMOKE_DOMAIN:-smoke.localhost}
work=${SMOKE_WORK:-${RUNNER_TEMP:-/tmp}/sr-prod-smoke}
base_url=https://$domain
ca=$work/caddy-root.crt
pwfile=$work/owner-password

dc() { docker compose -f "$compose_dir/prod.yml" --env-file "$env_file" "$@"; }
sr_cli_c() { dc run --rm --no-deps --entrypoint /usr/local/bin/sr-cli sr-server "$@"; }
# Run a shell command inside the postgres container as the postgres user (what the packaged scripts expect).
pg_user() { dc exec -T -u postgres postgres sh -c "$1"; }
# A query against one database of the stack's Postgres, over the local socket; prints the bare value.
q() { dc exec -T postgres psql -X -A -t -v ON_ERROR_STOP=1 -U sideroom -d "$1" -c "$2"; }
curl_ca() { curl --fail --silent --show-error --cacert "$ca" "$@"; }
env_value() { sed -n "s/^$1=//p" "$env_file" | head -n 1 | sed "s/^'//; s/'\$//"; }

sudo_run() {
  if [ "$(id -u)" = "0" ]; then "$@"; else sudo "$@"; fi
}

stage_install() {
  : "${SMOKE_IMAGE:?set SMOKE_IMAGE}"
  : "${SMOKE_WEB_DIR:?set SMOKE_WEB_DIR}"
  need docker
  [ -f "$SMOKE_WEB_DIR/index.html" ] || die "$SMOKE_WEB_DIR has no index.html: build the web app first (pnpm --filter @sideroom/web build)"
  umask 077
  mkdir -p "$work"
  if [ ! -f "$pwfile" ]; then
    need openssl
    openssl rand -hex 16 >"$pwfile"
  fi
  if [ "${SMOKE_SKIP_HOSTS:-0}" != "1" ] && ! grep -q "[[:space:]]$domain\$" /etc/hosts 2>/dev/null; then
    log "adding $domain to /etc/hosts"
    echo "127.0.0.1 $domain" | sudo_run tee -a /etc/hosts >/dev/null
  fi
  log "install.sh --yes for https://$domain with image $SMOKE_IMAGE"
  SR_DOMAIN=$domain SR_IMAGE=$SMOKE_IMAGE SR_WEB_DIR=$SMOKE_WEB_DIR \
    SMTP_HOST=mail.example.org SMTP_PORT=587 SMTP_TLS=starttls \
    SR_OWNER_USERNAME=owner SR_OWNER_EMAIL=owner@example.test SR_OWNER_PASSWORD=$(cat "$pwfile") \
    SR_GARAGE_CAPACITY=1G SR_INGEST_DIR="$work/ingest" SR_PAYMENTS_ELECTRS=no \
    SR_INSTALL_SKIP_PULL=1 \
    sh "$repo/deploy/install.sh" --yes
  [ "$(stat -c %a "$env_file")" = "600" ] || die ".env.prod is not mode 0600"
}

fetch_ca() {
  i=0
  while [ "$i" -lt 60 ]; do
    if dc cp caddy:/data/caddy/pki/authorities/local/root.crt "$ca" >/dev/null 2>&1 && [ -s "$ca" ]; then
      return 0
    fi
    i=$((i + 1))
    sleep 2
  done
  die "Caddy never wrote its internal root certificate"
}

stage_wait() {
  log "waiting for sr-server to report healthy (migrations and the search index can take a while on first boot)"
  i=0
  while :; do
    state=$(docker inspect -f '{{.State.Health.Status}}' "$(dc ps -q sr-server)" 2>/dev/null || echo missing)
    [ "$state" = "healthy" ] && break
    i=$((i + 1))
    [ "$i" -lt 90 ] || die "sr-server is $state after 3 minutes"
    sleep 2
  done
  fetch_ca
  log "healthz through Caddy at $base_url"
  i=0
  until curl_ca "$base_url/healthz" >/dev/null 2>&1; do
    i=$((i + 1))
    [ "$i" -lt 60 ] || { curl_ca -v "$base_url/healthz" || true; die "$base_url/healthz never answered"; }
    sleep 2
  done
  log "healthz: $(curl_ca "$base_url/healthz")"

  headers=$(curl_ca -I "$base_url/")
  echo "$headers" | grep -qi '^strict-transport-security:' || die "no HSTS header on /"
  echo "$headers" | grep -qi '^content-security-policy:.*frame-ancestors' || die "no CSP with frame-ancestors on /"
  code=$(curl --silent --output /dev/null --write-out '%{http_code}' --cacert "$ca" "$base_url/internal/metrics")
  [ "$code" = "404" ] || die "/internal/metrics answered $code through Caddy (want 404)"
  curl_ca "$base_url/" | grep -qi '<div id="root"' || die "the web app shell is not served at /"
  # The container health check: sr-server probes its own /readyz (database and storage).
  dc exec -T sr-server /usr/local/bin/sr-server healthcheck
  log "stack is up behind Caddy"
}

stage_migrate() {
  log "sr-cli migrate (install.sh already ran it once: this one must change nothing)"
  sr_cli_c migrate
  sr_cli_c verify-access-log
  # install.sh created the Owner with bootstrap; a second bootstrap must refuse rather than make a second one.
  out=$(SR_OWNER_PASSWORD=$(cat "$pwfile") dc run --rm --no-deps -e SR_OWNER_PASSWORD --entrypoint /usr/local/bin/sr-cli sr-server \
    bootstrap --owner-username owner --owner-email owner@example.test 2>&1) && second=ok || second=refused
  echo "second bootstrap: $second"
  echo "$out" | tail -n 3
}

stage_signin() {
  NODE_EXTRA_CA_CERTS=$ca SMOKE_URL=$base_url SMOKE_OWNER_PASSWORD_FILE=$pwfile \
    node "$here/smoke-admin.mjs" signin
}

stage_backups() {
  need docker
  fetch_ca
  q sideroom "select count(*) from accounts" >/dev/null

  log "1/6 backup.sh inside the postgres container (the pg-backups volume sr-server reads)"
  pg_user 'SR_DATABASE_URL=postgres://sideroom@/sideroom SR_BACKUP_DIR=/var/backups/sideroom SR_BACKUP_DIR_MODE=750 sh /opt/sideroom/scripts/backup.sh'
  dump=$(pg_user 'ls -1 /var/backups/sideroom/db/sideroom-*.dump | sort | tail -n 1' | tr -d '\r')
  [ -n "$dump" ] || die "backup.sh left no dump"
  log "dump: $dump"

  log "2/6 the base backup, with the exact command of deploy/systemd/sideroom-basebackup.service"
  unit_cmd=$(sed -n 's/^ExecStart=.* exec -T postgres sh -c "\(.*\)"$/\1/p' "$repo/deploy/systemd/sideroom-basebackup.service")
  [ -n "$unit_cmd" ] || die "could not read the command out of sideroom-basebackup.service"
  dc exec -T postgres sh -c "$unit_cmd"
  dc exec -T postgres sh -c 'ls /var/backups/sideroom/base/base-*/backup.json' >/dev/null || die "no base backup was written"

  log "3/6 restore.sh into a second database"
  q postgres "drop database if exists sideroom_restored"
  q postgres "create database sideroom_restored"
  pg_user "sh /opt/sideroom/scripts/restore.sh '$dump' postgres://sideroom@/sideroom_restored"
  src_tables=$(q sideroom "select count(*) from information_schema.tables where table_schema='public'")
  new_tables=$(q sideroom_restored "select count(*) from information_schema.tables where table_schema='public'")
  src_mig=$(q sideroom "select count(*) from _sqlx_migrations")
  new_mig=$(q sideroom_restored "select count(*) from _sqlx_migrations")
  src_acc=$(q sideroom "select count(*) from accounts")
  new_acc=$(q sideroom_restored "select count(*) from accounts")
  echo "tables $src_tables/$new_tables, migrations $src_mig/$new_mig, accounts $src_acc/$new_acc (live/restored)"
  [ "$src_tables" = "$new_tables" ] && [ "$src_mig" = "$new_mig" ] && [ "$src_acc" = "$new_acc" ] \
    || die "the restored database differs from the live one"
  # Restoring again without SR_RESTORE_FORCE must be refused.
  if pg_user "sh /opt/sideroom/scripts/restore.sh '$dump' postgres://sideroom@/sideroom_restored" >/dev/null 2>&1; then
    die "restore.sh overwrote a non-empty database without SR_RESTORE_FORCE"
  fi
  q postgres "drop database sideroom_restored"

  log "4/6 restore-drill.sh against the newest dump (leaves restore-drill.ok for Admin > System)"
  pg_user 'SR_DATABASE_URL=postgres://sideroom@/sideroom SR_BACKUP_DIR=/var/backups/sideroom sh /opt/sideroom/scripts/restore-drill.sh'

  log "5/6 wal-drill.sh: a scratch cluster in the postgres image, restored to a chosen moment"
  pg_user 'sh /opt/sideroom/scripts/wal-drill.sh'

  log "6/6 the live cluster archives WAL into the wal-archive volume"
  before=$(pg_user 'ls /var/lib/postgresql/wal-archive | wc -l' | tr -d '\r ')
  q sideroom "select pg_switch_wal()" >/dev/null
  i=0
  while :; do
    now=$(pg_user 'ls /var/lib/postgresql/wal-archive | wc -l' | tr -d '\r ')
    [ "$now" -gt "$before" ] && break
    i=$((i + 1))
    [ "$i" -lt 30 ] || die "no WAL segment reached the archive (archive_command is failing: see the postgres log)"
    sleep 2
  done
  echo "wal-archive holds $now segment(s)"

  log "Admin > System shows the backup time"
  NODE_EXTRA_CA_CERTS=$ca SMOKE_URL=$base_url SMOKE_OWNER_PASSWORD_FILE=$pwfile \
    node "$here/smoke-admin.mjs" backup-time --dir /var/backups/sideroom --max-age-min 45 --expect-verify
}

stage_cli_backups() {
  : "${SR_CLI:?set SR_CLI to the sr-cli binary}"
  need pg_dump
  need pg_restore
  need docker
  pgip=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$(dc ps -q postgres)")
  garageip=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$(dc ps -q garage)")
  [ -n "$pgip" ] && [ -n "$garageip" ] || die "could not find the postgres and garage container addresses"
  pgpw=$(env_value POSTGRES_PASSWORD)
  [ -n "$pgpw" ] || die "no POSTGRES_PASSWORD in $env_file"
  cli() {
    db=$1
    shift
    env "SR_DATABASE__URL=postgres://sideroom:$pgpw@$pgip:5432/$db?sslmode=disable" \
      "SR_SERVER__PUBLIC_URL=$base_url" "SR_SERVER__SECRET_KEY=$(env_value SR_SECRET_KEY)" \
      "SR_STORAGE__ENDPOINT=http://$garageip:3900" SR_STORAGE__REGION=garage \
      "SR_STORAGE__BUCKET=$(env_value GARAGE_BUCKET)" "SR_STORAGE__ACCESS_KEY=$(env_value GARAGE_ACCESS_KEY)" \
      "SR_STORAGE__SECRET_KEY=$(env_value GARAGE_SECRET_KEY)" \
      "SR_SMTP__HOST=mail.example.org" "SR_SMTP__FROM=SIDE ROOM <no-reply@example.test>" \
      "$SR_CLI" "$@"
  }
  dir=$work/srcli-backups
  rm -rf "$dir"

  log "1/4 sr-cli upgrade --dry-run on the live database"
  cli sideroom upgrade --dry-run --backup-dir "$dir"

  log "2/4 sr-cli backup"
  cli sideroom backup --dir "$dir"
  dump=$(ls -1 "$dir"/db/sideroom-*.dump | sort | tail -n 1)
  [ -f "$dump.json" ] || die "sr-cli backup wrote no manifest"
  grep -q '"sha256"' "$dump.json" || die "the manifest has no checksum"

  log "3/4 sr-cli restore into an empty database, then again without --force (refused), then with --force"
  q postgres "drop database if exists sideroom_srcli"
  q postgres "create database sideroom_srcli"
  cli sideroom_srcli restore "$dump"
  if cli sideroom_srcli restore "$dump" >/dev/null 2>&1; then
    die "sr-cli restore overwrote a non-empty database without --force"
  fi
  cli sideroom_srcli restore "$dump" --force
  src_tables=$(q sideroom "select count(*) from information_schema.tables where table_schema='public'")
  new_tables=$(q sideroom_srcli "select count(*) from information_schema.tables where table_schema='public'")
  src_acc=$(q sideroom "select count(*) from accounts")
  new_acc=$(q sideroom_srcli "select count(*) from accounts")
  echo "tables $src_tables/$new_tables, accounts $src_acc/$new_acc (live/restored)"
  [ "$src_tables" = "$new_tables" ] && [ "$src_acc" = "$new_acc" ] || die "the sr-cli restore differs from the live database"

  log "4/4 sr-cli upgrade --dry-run on the restored copy, and a tampered dump is refused"
  cli sideroom_srcli upgrade --dry-run --backup-dir "$dir"
  cp "$dump" "$dir/tampered.dump"
  cp "$dump.json" "$dir/tampered.dump.json"
  echo tamper >>"$dir/tampered.dump"
  if cli sideroom_srcli restore "$dir/tampered.dump" --force >/dev/null 2>&1; then
    die "sr-cli restore accepted a dump that does not match its manifest"
  fi
  q postgres "drop database sideroom_srcli"
  log "sr-cli backup, restore, restore --force and upgrade --dry-run all work"
}

stage_logs() {
  dc ps -a || true
  dc logs --no-color --tail 80 || true
}

stage=${1:-}
case $stage in
  install) stage_install ;;
  wait) stage_wait ;;
  migrate) stage_migrate ;;
  signin) stage_signin ;;
  backups) stage_backups ;;
  cli-backups) stage_cli_backups ;;
  logs) stage_logs ;;
  *) die "usage: prod-smoke.sh install|wait|migrate|signin|backups|cli-backups|logs" ;;
esac
