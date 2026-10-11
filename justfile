# SIDE ROOM task runner (FND-REPO-010). Install: https://github.com/casey/just
set shell := ["bash", "-euo", "pipefail", "-c"]

compose := "docker compose -f deploy/compose/dev.yml"
export DATABASE_URL := env_var_or_default("DATABASE_URL", "postgres://sideroom:sideroom_dev@127.0.0.1:5432/sideroom")

default:
    @just --list

# Start the dev stack (Postgres, Garage, Caddy, Mailpit, coturn) and bootstrap Garage
dev:
    {{compose}} up -d --wait postgres
    {{compose}} up -d
    ./deploy/compose/garage/bootstrap.sh
    @echo "Stack up. Mailpit: http://localhost:8025  App proxy: http://localhost:8088"

# Stop the dev stack
dev-down:
    {{compose}} down

# Stop the dev stack and wipe all data volumes
dev-reset:
    {{compose}} down -v

# Run all tests
test:
    cargo nextest run --workspace
    pnpm -r --if-present test

# Format check, clippy, deny, and web lint/typecheck
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo deny check
    pnpm -r --if-present lint
    pnpm -r --if-present typecheck

# Apply database migrations
migrate:
    cargo run -p sr-cli -- migrate

# Load dev seed data
seed:
    cargo run -p sr-cli -- seed

# Regenerate OpenAPI spec and the TypeScript client
gen-api:
    cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json
    pnpm --filter @sideroom/api-client generate

# Build WASM packages into packages/wasm
wasm:
    pnpm --filter @sideroom/wasm build

# Print the OpenAPI document (what CI diffs against packages/api-client/openapi.json)
openapi:
    cargo run -q -p sr-server -- openapi

# Run the server against the dev stack (SR_* env overrides apply)
serve:
    SR_DATABASE__URL="$DATABASE_URL" cargo run -p sr-server
