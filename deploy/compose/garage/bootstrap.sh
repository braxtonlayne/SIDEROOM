#!/usr/bin/env bash
# Idempotent Garage bootstrap for the dev stack: layout, dev access key, bucket.
# Run from the repo root after `docker compose -f deploy/compose/dev.yml up -d`.
set -euo pipefail

COMPOSE=(docker compose -f deploy/compose/dev.yml)
ENV_FILE="deploy/compose/.env"
[ -f "$ENV_FILE" ] || ENV_FILE="deploy/compose/.env.example"
set -a; . "$ENV_FILE"; set +a

garage() { "${COMPOSE[@]}" exec -T garage /garage "$@"; }

echo "Waiting for Garage..."
for _ in $(seq 1 30); do garage status >/dev/null 2>&1 && break; sleep 1; done
garage status >/dev/null

NODE_ID="$(garage node id -q 2>/dev/null | cut -d@ -f1)"
if ! garage status | grep -q "$NODE_ID.*dc1"; then
  garage layout assign -z dc1 -c 1G "$NODE_ID"
  garage layout apply --version 1
fi

garage key info "$GARAGE_ACCESS_KEY" >/dev/null 2>&1 \
  || garage key import --yes -n sideroom-dev "$GARAGE_ACCESS_KEY" "$GARAGE_SECRET_KEY"
garage bucket info "$GARAGE_BUCKET" >/dev/null 2>&1 || garage bucket create "$GARAGE_BUCKET"
garage bucket allow --read --write --owner "$GARAGE_BUCKET" --key "$GARAGE_ACCESS_KEY"

echo "Garage ready: bucket=$GARAGE_BUCKET endpoint=http://127.0.0.1:3900 region=garage"
