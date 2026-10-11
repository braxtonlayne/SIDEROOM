# Getting started (local dev)

Goal: running stack in under 15 minutes.

## Prerequisites

- Rust stable (installed via `rustup`; `rust-toolchain.toml` pins the channel)
- Node LTS and pnpm 9 (`corepack enable`)
- Docker with Compose v2
- [`just`](https://github.com/casey/just), `cargo-nextest`, `cargo-deny`, `sqlx-cli`
  (`cargo install just cargo-nextest cargo-deny sqlx-cli --locked`)
- On Windows, run `just` recipes from Git Bash or WSL.

## Steps

1. Clone the repo and `cd` into it.
2. `pnpm install`
3. `just dev` starts the stack from `deploy/compose/dev.yml` and bootstraps Garage (layout, dev key, bucket).
4. `just migrate` then `just seed`.
5. Run the server (`cargo run -p sr-server`) and the web app (`pnpm --filter web dev`) once those crates and apps land.

## Services

| Service | Address |
|---|---|
| Postgres 17 | `postgres://sideroom:sideroom_dev@127.0.0.1:5432/sideroom` |
| Garage S3 | `http://127.0.0.1:3900`, region `garage`, bucket `sideroom-dev` |
| Caddy (dev proxy) | `http://localhost:8088` |
| Mailpit | SMTP `127.0.0.1:1025`, UI `http://localhost:8025` |
| coturn | `127.0.0.1:3478` |

All credentials are dev-only and live in `deploy/compose/.env.example`; copy it to `deploy/compose/.env` to override.

## Everyday commands

`just test`, `just lint`, `just gen-api`, `just wasm`, `just dev-down`, `just dev-reset` (wipes volumes).

## Troubleshooting

- Garage bootstrap fails: wait a few seconds and rerun `./deploy/compose/garage/bootstrap.sh`; it is idempotent.
- coturn on Docker Desktop cannot expose its full relay range; voice relay testing needs a Linux host.
- Caddy proxies to the host at `host.docker.internal:8080` (server) and `:5173` (Vite).
