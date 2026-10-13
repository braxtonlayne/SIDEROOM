# Shared helpers for the backup scripts. Sourced, never executed.
# shellcheck shell=sh

die() { echo "error: $*" >&2; exit 1; }
log() { echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*"; }

need() { command -v "$1" >/dev/null 2>&1 || die "$1 is required but not on PATH"; }

# Parses a postgres:// URL into PGHOST/PGPORT/PGUSER/PGPASSWORD/PGDATABASE so the password never appears
# on a command line (visible in `ps`). A sslmode query parameter becomes PGSSLMODE.
pg_env_from_url() {
  url=$1
  case $url in postgres://*|postgresql://*) ;; *) die "database URL must start with postgres://" ;; esac
  rest=${url#*://}
  creds=${rest%%@*}
  hostpart=${rest#*@}
  PGUSER=${creds%%:*}
  PGPASSWORD=
  case $creds in *:*) PGPASSWORD=${creds#*:} ;; esac
  hostport=${hostpart%%/*}
  dbq=${hostpart#*/}
  PGDATABASE=${dbq%%\?*}
  PGHOST=${hostport%%:*}
  PGPORT=5432
  case $hostport in *:*) PGPORT=${hostport#*:} ;; esac
  case $dbq in
    *sslmode=*)
      s=${dbq#*sslmode=}
      PGSSLMODE=${s%%&*}
      export PGSSLMODE
      ;;
  esac
  export PGUSER PGPASSWORD PGDATABASE PGHOST PGPORT
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}
