# SIDE ROOM task runner (FND-REPO-010). Install: https://github.com/casey/just
set shell := ["bash", "-euo", "pipefail", "-c"]

compose := "docker compose -f deploy/compose/dev.yml"
export DATABASE_URL := env_var_or_default("DATABASE_URL", "postgres://sideroom:sideroom_dev@127.0.0.1:5432/sideroom")

default:
    @just --list

# One-time developer setup: use the repo's git hooks and enable corepack (for pnpm)
setup:
    git config core.hooksPath .githooks
    corepack enable

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
    pnpm --filter @sideroom/api-client mocks:generate

# Build WASM packages into packages/wasm
wasm:
    pnpm --filter @sideroom/wasm build

# Print the OpenAPI document (what CI diffs against packages/api-client/openapi.json)
openapi:
    cargo run -q -p sr-server -- openapi

# Run the server against the dev stack (SR_* env overrides apply).
# Only the database URL is exported here. Set SR_STORAGE__BUCKET and the Garage keys
# from deploy/compose/.env yourself, and set SR_SERVER__SECRET_KEY for stable TOTP secrets.
serve:
    SR_DATABASE__URL="$DATABASE_URL" cargo run -p sr-server

# Phase 0 exit-gate e2e (FND-CI-004): fresh DB, migrate, run sr-server, run Playwright.
# Usage: just e2e [playwright args], e.g. `just e2e --project=chromium`. Wipes the dev Postgres volume.
e2e *args:
    pnpm install --frozen-lockfile
    pnpm --filter @sideroom/e2e exec playwright install --with-deps chromium firefox webkit
    {{compose}} down -v
    {{compose}} up -d --wait postgres
    {{compose}} up -d garage
    ./deploy/compose/garage/bootstrap.sh
    cargo build -p sr-server -p sr-cli
    ./target/debug/sr-cli migrate
    ./deploy/compose/e2e-run.sh {{args}}

# Regenerate the PWA and favicon icons from packages/ui/assets/door-mark.svg (commit the output)
icons:
    cargo run -q -p sr-theme --features icons --bin sr-theme-icons -- packages/ui/assets/door-mark.svg apps/web/public/icons

# XC-PERF-004: EXPLAIN the hot read paths on a seeded scratch DB (needs SR_TEST_DATABASE_URL)
explain-hot-paths:
    cargo test -p sr-db --test explain_hot_paths -- --ignored --nocapture
