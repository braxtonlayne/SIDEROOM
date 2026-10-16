#!/usr/bin/env bash
# Idempotent Garage bootstrap for the dev stack: layout, dev access key, bucket.
# Run from the repo root after `docker compose -f deploy/compose/dev.yml up -d`.
set -euo pipefail

# The production installer (deploy/install.sh) reuses this script with SR_COMPOSE_FILE=prod.yml,
# SR_ENV_FILE=.env.prod, SR_GARAGE_CAPACITY (layout capacity) and SR_SKIP_CORS=1 (the installer applies the CORS
# rule through the sr-cli that ships in the server image).
COMPOSE=(docker compose -f "${SR_COMPOSE_FILE:-deploy/compose/dev.yml}")
if [ -n "${SR_ENV_FILE:-}" ]; then
  ENV_FILE="$SR_ENV_FILE"
  COMPOSE+=(--env-file "$ENV_FILE")
else
  ENV_FILE="deploy/compose/.env"
  [ -f "$ENV_FILE" ] || ENV_FILE="deploy/compose/.env.example"
fi
set -a; . "$ENV_FILE"; set +a

garage() { "${COMPOSE[@]}" exec -T garage /garage "$@"; }

echo "Waiting for Garage..."
for _ in $(seq 1 30); do garage status >/dev/null 2>&1 && break; sleep 1; done
garage status >/dev/null

NODE_ID="$(garage node id -q 2>/dev/null | cut -d@ -f1)"
if ! garage status | grep -q "$NODE_ID.*dc1"; then
  garage layout assign -z dc1 -c "${SR_GARAGE_CAPACITY:-1G}" "$NODE_ID"
  garage layout apply --version 1
fi

garage key info "$GARAGE_ACCESS_KEY" >/dev/null 2>&1 \
  || garage key import --yes -n "${SR_GARAGE_KEY_NAME:-sideroom-dev}" "$GARAGE_ACCESS_KEY" "$GARAGE_SECRET_KEY"
garage bucket info "$GARAGE_BUCKET" >/dev/null 2>&1 || garage bucket create "$GARAGE_BUCKET"
garage bucket allow --read --write --owner "$GARAGE_BUCKET" --key "$GARAGE_ACCESS_KEY"

echo "Garage ready: bucket=$GARAGE_BUCKET endpoint=http://127.0.0.1:3900 region=garage"
if [ "${SR_SKIP_CORS:-0}" = "1" ]; then exit 0; fi

# D-175: browsers PUT uploads straight to the bucket, so the bucket needs a CORS rule for the app
# origin(s). Uses sr-cli when one is available (SR_CLI, a built binary, or PATH); otherwise prints
# the command. Idempotent: the rule is replaced, not appended.
SR_CLI_BIN="${SR_CLI:-}"
[ -n "$SR_CLI_BIN" ] || { [ -x target/debug/sr-cli ] && SR_CLI_BIN=target/debug/sr-cli; } || true
[ -n "$SR_CLI_BIN" ] || SR_CLI_BIN="$(command -v sr-cli || true)"
CORS_ORIGINS=(--origin "${SR_CORS_ORIGIN:-http://localhost:8088}" --origin "http://localhost:8080")
if [ -n "$SR_CLI_BIN" ]; then
  SR_STORAGE__ENDPOINT=http://127.0.0.1:3900 SR_STORAGE__REGION=garage SR_STORAGE__BUCKET="$GARAGE_BUCKET" \
    SR_STORAGE__ACCESS_KEY="$GARAGE_ACCESS_KEY" SR_STORAGE__SECRET_KEY="$GARAGE_SECRET_KEY" \
    "$SR_CLI_BIN" storage ensure-cors "${CORS_ORIGINS[@]}"
else
  echo "sr-cli not found; after building it run: sr-cli storage ensure-cors ${CORS_ORIGINS[*]}"
fi
