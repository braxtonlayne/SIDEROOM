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
   `just seed` also queues the Live TV slate (the off-air colour-bars loop) when `ffmpeg` is on your PATH and says so when it skips it; without ffmpeg, run `sr-cli tv slate` later on any machine that has it (needs the server and a transcoder worker running).
5. Run the server (`cargo run -p sr-server`) and the web app (`pnpm --filter web dev`) once those crates and apps land.

## Public surfaces, the app and the gate (D-385)

The web app builds twice into `apps/web/dist`: a minimal bundle per public surface (landing, sign-in, invite) and the gated app. `vite dev` imitates the split: `vite/publicDev.ts` serves a public shell for a public path and asks `sr-server` (`SR_DEV_API`, default `http://127.0.0.1:8080`) whether the request has a session, answering `302 /login?next=` when it has not, as production does.

To run the browser tests against the real thing, build and let `sr-server` serve the dist:

```
VITE_SR_E2E=1 pnpm --filter @sideroom/web build
SR_E2E_WEB=server SR_E2E_BASE_URL=http://localhost:8080 SR_WEB__ROOT=$PWD/apps/web/dist SR_WEB__SERVE_PUBLIC=true ./deploy/compose/e2e-run.sh --project=chromium tests/entrance-isolation.spec.ts
node scripts/check-entrance-isolation.mjs
```

`SR_E2E_WEB=dev` (the default) uses the Vite dev server on 5173, `server` uses `sr-server` on 8080 (`SR_E2E_SERVER_PORT` to move it), `preview` is the old ungated `vite preview`. `entrance-isolation.spec.ts` skips itself in `dev` mode. The static suite (`pnpm --filter @sideroom/e2e test:static`) previews the build through `e2e/vite.static.config.ts`, which adds `vite/previewGate.ts` (shell per path, ungated). When you add a feature, add its names to `scripts/entrance-denylist.json`; a word that must be tolerated in a public file needs a reason in `scripts/entrance-allow.json`.

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
