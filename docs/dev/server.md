# Running and operating sr-server

`sr-server` is the single backend process: REST API, gateway WebSocket, job runner and
metrics listener. Wiring lives in `crates/sr-server/src/app.rs`.

## Commands

| Command | What it does |
|---|---|
| `sr-server` / `sr-server serve` | Run the server. Reads `SR_CONFIG` (TOML path) and `SR_*` env vars. |
| `sr-server openapi` | Print the OpenAPI JSON to stdout (no config, no logging). |
| `just serve` | `cargo run -p sr-server` against the dev stack (`DATABASE_URL` is passed as `SR_DATABASE__URL`). |
| `just gen-api` | Regenerate `packages/api-client/openapi.json` and the TypeScript types. |

Run `just migrate` first; the server does not migrate on boot.

## Configuration

Layering: defaults, TOML file, `SR_*` env (`__` separates nested keys).

| Key | Default | Notes |
|---|---|---|
| `server.bind` | `127.0.0.1:8080` | Public listener: REST at `/api/v1`, gateway at `/gateway`. |
| `server.metrics_bind` | `127.0.0.1:9100` | `/internal/metrics`. Keep it loopback or firewalled. |
| `server.secret_key` | empty | 64 hex chars; encrypts TOTP secrets. Empty = random per boot (dev only). |
| `server.shutdown_grace_secs` | `25` | Drain budget for HTTP, then again for jobs. |
| `database.url`, `database.max_connections` | see `sr-core` | |
| `storage.*` | see `sr-core` | S3-compatible (Garage in dev). |

## Graceful shutdown

On SIGINT or SIGTERM the server:

1. sends RECONNECT (op 7) to every gateway client (sessions stay resumable);
2. stops accepting HTTP connections and drains in-flight requests for up to
   `shutdown_grace_secs`. WebSockets never finish on their own, so any connection
   still open after the grace period is dropped;
3. stops gateway background tasks and signals the job runner, which stops claiming
   and lets running jobs finish (again up to the grace period);
4. closes the Postgres pool.

Set the container/systemd stop timeout above `2 * shutdown_grace_secs` (default 50 s;
Docker's default 10 s is too short, use `stop_grace_period: 60s`).

## Readiness and metrics

`/healthz` is liveness. `/readyz` checks Postgres (`SELECT 1`) and object storage (a
`head` of a sentinel key; "not found" counts as reachable). Gateway session count and
pool gauges refresh every 15 s on `/internal/metrics`.

## Adding routes

Feature routers are merged in `crates/sr-api/src/routes/mod.rs` (`all()` for handlers,
`openapi()` for the document). After changing any annotated handler run `just gen-api`
and commit `packages/api-client`; CI fails when the generated client drifts.

## Docker images

`deploy/docker/{server,voice,transcoder}.Dockerfile` build from the repo root, e.g.
`docker build -f deploy/docker/server.Dockerfile -t sideroom-server .` (static musl
binary on distroless). The release workflow publishes them.
