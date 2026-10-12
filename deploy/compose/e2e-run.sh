#!/usr/bin/env bash
# Starts sr-server for the e2e run, runs Playwright, and always stops the server. Used by `just e2e`
# and the E2E workflow. The Postgres/Garage stack must be up and migrated.
set -euo pipefail
cd "$(dirname "$0")/../.."

# The API's Origin/CSRF allow-list is the public URL; the browser talks to the Vite dev server on 5173.
export SR_DATABASE__URL="${DATABASE_URL:-postgres://sideroom:sideroom_dev@127.0.0.1:5432/sideroom}"
export SR_SERVER__PUBLIC_URL="${SR_E2E_BASE_URL:-http://localhost:5173}"

# Object storage: the dev Garage key pair and bucket created by deploy/compose/garage/bootstrap.sh.
# /readyz (which this script waits for) checks storage, so the server needs these.
ENV_FILE="deploy/compose/.env"
[ -f "$ENV_FILE" ] || ENV_FILE="deploy/compose/.env.example"
set -a; . "$ENV_FILE"; set +a
export SR_STORAGE__ENDPOINT="${SR_STORAGE__ENDPOINT:-http://127.0.0.1:3900}"
export SR_STORAGE__REGION="${SR_STORAGE__REGION:-garage}"
export SR_STORAGE__BUCKET="${SR_STORAGE__BUCKET:-${GARAGE_BUCKET:-sideroom}}"
export SR_STORAGE__ACCESS_KEY="${SR_STORAGE__ACCESS_KEY:-$GARAGE_ACCESS_KEY}"
export SR_STORAGE__SECRET_KEY="${SR_STORAGE__SECRET_KEY:-$GARAGE_SECRET_KEY}"

./target/debug/sr-server >server.log 2>&1 &
server_pid=$!
trap 'kill "$server_pid" 2>/dev/null || true' EXIT

timeout 60 bash -c 'until curl -fs http://127.0.0.1:8080/readyz >/dev/null; do sleep 1; done' || {
  echo "sr-server did not become ready; server.log:" >&2
  cat server.log >&2
  exit 1
}

pnpm --filter @sideroom/e2e exec playwright test "$@"
