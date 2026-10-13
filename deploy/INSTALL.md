# Installing SIDE ROOM

This guide installs a single-node instance: one `sr-server` process behind a TLS reverse proxy, with Postgres, S3-compatible object storage and an SMTP server. The steps follow [docs/dev/server.md](../docs/dev/server.md), which describes the commands and configuration keys in more detail. Doc 12 covers the production architecture.

## 1. What you need

| Piece | Notes |
|---|---|
| Postgres 17 | Reached over TLS. See section 2. |
| S3-compatible storage | Garage is used in development. Create one bucket and one access key for it. |
| SMTP server | Use STARTTLS or implicit TLS. `tls = "none"` is for Mailpit only. |
| Reverse proxy | Terminates HTTPS. Forwards `/api/*`, `/gateway` and `/cdn/*` to the server. Never expose `/internal/*`. |
| A public hostname | Set as `server.public_url`, for example `https://chat.example.org`. |

## 2. Database connection and TLS

The server is built with sqlx's `tls-rustls` feature, so connections can be encrypted without a C library. Put the `sslmode` in the database URL. The development default is `prefer`, which does not require TLS. Do not use it in production.

```
SR_DATABASE__URL=postgres://sideroom:<password>@db.example.org:5432/sideroom?sslmode=require
```

- `sslmode=require` is the minimum for production. It encrypts the connection but does not check the server certificate.
- If Postgres runs on another host, use `sslmode=verify-full` with `sslrootcert=/path/to/ca.pem`, so the server certificate is checked too.
- Keep the password out of files that are committed. Pass the URL through the environment (`SR_DATABASE__URL`).

(Q13)

## 3. Secrets

Generate every secret fresh for each installation. Never reuse the development values in `deploy/compose/.env.example`.

- **`server.secret_key`**: 64 hexadecimal characters (32 random bytes). It encrypts TOTP secrets at rest. Set it, because the server refuses to start with an empty key unless `public_url` is a localhost address. **If the key is lost, every stored TOTP secret is lost.** To rotate it, move the old key into `server.secret_key_previous`, set the new key, and run `sr-cli rotate-secret-key`.
- Database password, storage access and secret keys, SMTP password and Discord client secret.

Keep these in the environment (`SR_SERVER__SECRET_KEY`, `SR_STORAGE__SECRET_KEY`, and so on) or in a file readable only by the service account.

## 4. Configuration

Configuration layers, lowest to highest: built-in defaults, the TOML file named by `SR_CONFIG`, then `SR_*` environment variables. Nested keys use a double underscore, so `SR_SMTP__TLS` sets `smtp.tls`.

```toml
[server]
bind = "127.0.0.1:8080"                  # public listener: REST /api/v1, gateway /gateway
public_url = "https://chat.example.org"
metrics_bind = "127.0.0.1:9100"          # /internal/metrics; keep it loopback or firewalled
trust_forwarded_for = true               # only behind your reverse proxy (see section 6)
shutdown_grace_secs = 25

[storage]
endpoint = "https://s3.example.org"
region = "garage"
bucket = "sideroom"

[smtp]
host = "mail.example.org"
port = 587
tls = "starttls"                         # "tls" for port 465; "none" is for Mailpit only
from = "SIDE ROOM <no-reply@example.org>"
```

Put `database.url`, `server.secret_key`, `storage.access_key`, `storage.secret_key`, `smtp.password` and `discord.client_secret` in the environment, not in this file.

Limits that are product settings (upload size, invite caps) are stored in the database and set from the admin pages, not here. (doc 12 §3)

## 5. Install walkthrough

1. **Get the binary or image.** Build the static server image from the repository root:

   ```
   docker build -f deploy/docker/server.Dockerfile -t sideroom-server .
   ```

   The image is a static musl binary on distroless. Or use the release image.

2. **Write the configuration** (section 4) and export the secrets (section 3).

3. **Apply the migrations.** The server does not migrate on boot unless `database.auto_migrate = true`, so run this first:

   ```
   sr-cli migrate
   ```

4. **Bootstrap the instance.** This creates the system user and the Owner account, and prints a one-time login link. The link is single use and valid for one hour, so open it right away. Give the password through `SR_OWNER_PASSWORD` or the prompt, not as a command-line argument, because other local users can read command-line arguments:

   ```
   sr-cli bootstrap --owner-username <name> --owner-email <email>
   ```

   Known gap: bootstrap creates the Staff Server but not its default roles yet (see the open questions).

   Then give the bucket its CORS rule. Chat uploads are presigned `PUT`s from the browser straight to the bucket (D-175), so without the rule the browser blocks them. The command reads the `[storage]` settings and does not need the database. It is safe to run again, because the rule is replaced, not appended:

   ```
   sr-cli storage ensure-cors --origin https://chat.example.org
   ```

   The rule allows `PUT`, `GET` and `HEAD` from the origin, allows the `Content-Type` request header and exposes `ETag`. Without `--origin` it uses `server.public_url`. If the web app is also served from another origin (a second hostname, a dev URL), repeat `--origin`.

5. **Start the server.** `sr-server` (or `sr-server serve`) reads `SR_CONFIG`. Check `GET /healthz` (liveness) and `GET /readyz` (checks Postgres and object storage).

6. **Put the reverse proxy in front.** Forward `/api/*`, `/gateway` and `/cdn/*` to `server.bind`. Do not forward `/internal/*`, and keep `metrics_bind` on loopback.

7. **Set the stop timeout.** The server drains for `shutdown_grace_secs` after SIGTERM, then for jobs again. Set the container or service stop timeout above `2 * shutdown_grace_secs` (50 s by default, so use 60 s). Docker's default of 10 s cuts the drain short.

Do not run `sr-cli seed` on an installation you care about. It refuses unless `server.public_url` is local, and `--force` overrides that check.

Backups: there is no `sr-cli backup` command yet. Back up Postgres and the object-storage bucket with your usual tools, and keep the `server.secret_key` value with the backups, because the TOTP secrets in the database cannot be read without it.

## 6. Reverse proxy and client IP

The server takes the client address from the connection, which behind a proxy is the proxy's address. Set `server.trust_forwarded_for = true` only when the proxy sets `X-Forwarded-For` and no client can reach the server directly. The server then uses the last `X-Forwarded-For` entry. Rate limits and login throttles depend on this address. (Q57, Q143)

## 7. Development stack

For local development use the compose stack in `deploy/compose`. Run `just setup` once. It points git at `.githooks` and runs `corepack enable`.

- `just dev` starts Postgres, Garage, Caddy (http://localhost:8088), Mailpit (http://localhost:8025) and coturn, then bootstraps Garage.
- **Bucket names differ.** Compose creates the bucket named by `GARAGE_BUCKET` in `deploy/compose/.env` (the example value is `sideroom-dev`), while the server default is `sideroom`. Set `SR_STORAGE__BUCKET` to the compose value, or change `GARAGE_BUCKET`, so the two match. (Q105)
- **Secret key.** With `server.secret_key` empty, the server generates a random key on every start, so TOTP secrets are lost on restart. For development, set a fixed 64-character key in your shell or in an ignored local file: `SR_SERVER__SECRET_KEY=<64 hex>`. (Q105)
- `just serve` exports only the database URL. Export the Garage access key and secret from `deploy/compose/.env` yourself until `just serve` does it.
- Development values in `deploy/compose/.env.example` (`sideroom_dev`, the Garage keys, the TURN secret) are for local use only.
