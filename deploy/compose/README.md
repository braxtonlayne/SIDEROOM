# Dev compose stack

`dev.yml` is the local development stack (FND-REPO-009), started by `just dev` or
`docker compose -f deploy/compose/dev.yml up -d`. Everything in it is dev-only: the credentials
are in `.env.example` and must never be reused in production. Production installs are described
in [`../INSTALL.md`](../INSTALL.md).

## Services and pinned tags

| Service | Image (pinned in `dev.yml`) | Host ports | Purpose |
|---|---|---|---|
| postgres | `postgres:17` | 127.0.0.1:5432 | System of record. User, database `sideroom`; password `POSTGRES_PASSWORD` (default `sideroom_dev`). |
| garage | `dxflrs/garage:v1.0.1` | 127.0.0.1:3900 (S3), 127.0.0.1:3903 (admin) | S3-compatible object storage. Needs `garage/bootstrap.sh` once. |
| caddy | `caddy:2` | 127.0.0.1:8088 | Reverse proxy in front of the dev server (`caddy/Caddyfile`). |
| mailpit | `axllent/mailpit:v1.21` | 127.0.0.1:1025 (SMTP), 127.0.0.1:8025 (web UI) | Catches outgoing mail. |
| coturn | `coturn/coturn:4.6` | 3478 (udp/tcp), 49152-49200/udp | TURN/STUN for voice and video (`coturn/turnserver.conf`). |

When you bump a tag, change it in `dev.yml`, in the table above, and re-run the two workflows
below; do not use `latest`. `caddy:2` is the only floating major tag (it follows Caddy 2.x).

## Garage bootstrap

Garage starts empty. After `up -d`, run from the repository root:

```sh
./deploy/compose/garage/bootstrap.sh
```

The script loads `deploy/compose/.env` (or `.env.example` when there is no `.env`) and is
idempotent. It runs the `garage` CLI inside the container (`docker compose ... exec -T garage /garage`)
and does, in order:

1. waits for `garage status` to answer (30 attempts, 1 s apart);
2. assigns this node a layout if it has none: `garage layout assign -z dc1 -c 1G <node-id>` then
   `garage layout apply --version 1`;
3. imports the dev key pair if it is missing: `garage key import --yes -n sideroom-dev <GARAGE_ACCESS_KEY> <GARAGE_SECRET_KEY>`;
4. creates the bucket `GARAGE_BUCKET` if it is missing: `garage bucket create <bucket>`;
5. grants the key access: `garage bucket allow --read --write --owner <bucket> --key <GARAGE_ACCESS_KEY>`.

Variables (from `.env.example`): `GARAGE_ACCESS_KEY`, `GARAGE_SECRET_KEY`, `GARAGE_BUCKET`
(`sideroom-dev`), `POSTGRES_PASSWORD`, `TURN_SECRET`. The server then uses endpoint
`http://127.0.0.1:3900` and region `garage`.

Flag check: these are the flags for Garage v1.0.1 as used by the script. They were not re-checked
against `garage --help` when this file was written (no Docker on the authoring machine); the
`storage.yml` workflow runs the script against the pinned image on every relevant change, so a
flag that stops working fails there. After a tag bump, confirm with
`docker compose -f deploy/compose/dev.yml exec garage /garage layout assign --help` (and the same
for `key import`, `bucket allow`).

## First run of the application

```sh
sr-cli migrate
sr-cli bootstrap --owner-username <name> --owner-email <email>   # password: SR_OWNER_PASSWORD or the prompt
```

`bootstrap` creates the system user, the Owner account and the Staff Server, and prints a one-time
login link. It is safe to re-run. Prefer `SR_OWNER_PASSWORD` over `--owner-password`, because
command-line arguments are visible to other local users. `sr-cli seed` (dev data, refuses
non-local instances without `--force`) is separate.

## How the stack is validated in CI

- **`.github/workflows/storage.yml`** brings up `postgres` and `garage` from `dev.yml`, runs
  `garage/bootstrap.sh`, then runs the live S3 suite (`cargo test -p sr-storage --test s3_live -- --ignored --test-threads=1`)
  and an end-to-end driver against `sr-server` (`e2e/support/storage-live.mjs`). It triggers on
  changes under `deploy/compose/**`, so edits to `dev.yml` or the bootstrap script are exercised.
- **`.github/workflows/e2e.yml`** brings up the same two services, bootstraps Garage, and runs
  Playwright through `e2e-run.sh`, which exports the `.env.example` Garage credentials to
  `sr-server`.
- **`.github/workflows/rust.yml`** uses its own `postgres:17` service container (user `sideroom`,
  password `sideroom_ci`) for the Postgres-backed tests, not this compose file.

Caddy, Mailpit and coturn are not started by those workflows; they are only for local development.
