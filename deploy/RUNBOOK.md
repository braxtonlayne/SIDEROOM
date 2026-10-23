# Runbook

Short, ordered checks for the alerts in `deploy/monitoring/alerts.yml`. Logs are JSON lines on stdout (`docker compose logs sr-server` or `journalctl -u sr-server`); every request line carries `request_id`, `method` and `path`, and the same id comes back to clients in `X-Request-Id`, so a user report with that header leads straight to the line.

Useful everywhere: `GET /healthz` (process is up), `GET /readyz` (Postgres and object storage reachable; failing checks are named in the body), `GET :9100/internal/metrics` (loopback or compose network only).

## Server down

1. `/healthz` from the host. Down: `systemctl status sr-server` or `docker compose ps`; read the last 100 log lines.
2. Crash loop with `secret_key` or config errors: the message names the key. Fix the environment, restart.
3. Starts but `/readyz` is 503: the body says which check failed. Postgres: is it up and accepting connections, is `database.url` right, does `sslmode` match. Object storage: is Garage up, are the key and bucket right.
4. Out of disk is the usual cause of both Postgres and Garage failing together: `df -h`, then see Disk pressure.

## High error rate

1. Grafana "Requests per second" split by status: which status, and the "Slowest routes" panel for which route.
2. Search the logs for the failing route and read the lines around the `request_id` of a 5xx response.
3. A deploy in the last hour? Roll back (UPGRADE.md).
4. Sudden 429s are the rate limiter, not an outage: see the "Rate limited" panel and `scope`.

## Slow requests

1. "REST latency" p95 and "Database pool": a pool at zero idle means queries are waiting for connections.
2. In Postgres: `select pid, now()-query_start as age, state, left(query,120) from pg_stat_activity where state <> 'idle' order by age desc;`
3. A single slow route points at a missing index: run `EXPLAIN (ANALYZE, BUFFERS)` on its main query (XC-PERF-004 tracks the review).

## Database pool exhausted

Long transactions or a leak. Same `pg_stat_activity` query; terminate the oldest idle-in-transaction session (`select pg_terminate_backend(pid)`), then raise `database.max_connections` only if Postgres has headroom (`show max_connections`).

## Jobs failing

The `jobs_processed_total{outcome="failed|dead"}` series names the `kind`. Failed attempts retry with backoff until `max_attempts`; then the job is `dead`. Read the rows (`select id, kind, attempts, last_error from jobs where state = 'dead' order by finished_at desc limit 20;`), fix the cause (SMTP down, object storage full), and set the row back to `queued` with `attempts = 0` once it is fixed.

## Disk pressure

1. `du -xh /var/lib | sort -h | tail`. Postgres WAL, Garage data, search index (`search.dir`) and the backup directory are the usual four.
2. Backups: lower `SR_BACKUP_KEEP` or move them to the Storage Box.
3. Never delete files inside the Postgres or Garage data directories by hand.

## Lost or suspect secret key

See BACKUP.md "Secrets rotation". If the key leaked, rotate it and ask members to re-enroll TOTP only if the dump also leaked.

## Locked-out owner

`sr-cli user reset-2fa <user>` removes TOTP, recovery codes and passkeys and revokes every session. `sr-cli invite` creates a platform invite without credits (it writes a platform audit row), and `sr-cli staff` manages appointments.

## Staff alerts and the second-factor window

The Staff Server's `#alerts`, `#new-members` and `#donations` channels are fed by the system user. A 15-minute health job raises an alert for a disk that is too full, a missing or stale backup, and each failed transcode job; the same problem is not raised again within a day. Tune it in Admin, Limits (group `alerts`), or `PATCH /admin/settings`:

| Setting | Default | Meaning |
|---|---|---|
| `alerts.disk_percent` | 80 | Alert when the watched disk is this percent full. |
| `ops.disk_path` | empty | Directory whose disk is watched; empty watches the server's working directory. Point it at the Postgres and Garage volume. |
| `ops.backup_dir` | empty | `SR_BACKUP_DIR` of `deploy/scripts/backup.sh`. Empty switches the backup alert off. |
| `alerts.backup_max_age_hours` | 36 | A backup (`*.dump` or manifest `*.json`) older than this raises an alert. |
| `alerts.email_to_owner` | off | Also email each alert to the Staff Server owner, at most one mail per kind every 30 minutes. Needs `[smtp]`. |

Sensitive admin actions (reading private content, changing an account's email, forcing a password reset, resetting 2FA, appointing or revoking staff, announcements) need a second factor checked on the session in the last 15 minutes. Staff without TOTP or a passkey cannot use them until they enroll. For local development and end-to-end tests set `SR_ADMIN_REQUIRE_REAUTH=0` to switch the check off; never do that on a real installation.

## Upgrade note: legacy download fallback is off

`SR_DOWNLOADS_LEGACY_FALLBACK` is gone and migration 0871 drops `cinema_download_grants`: only entitlements grant the Patron download perk. **Upgrade order:** run a Phase 12 release first (its migration 0542 copied unexpired legacy grants into entitlements), check Admin > Entitlements that everyone who should keep downloads has a grant, then upgrade to this release. Migration 0871 refuses to run ("run a Phase 12 release first so entitlements are backfilled") when a legacy grant row is newer than the newest entitlement row, so a skipped release stops at startup instead of silently dropping the perk; the fix is to deploy the Phase 12 release, then retry. The guard cannot see a grant created after the last entitlement; if you added legacy rows by hand, add the matching entitlement in Admin > Entitlements first.

## Preflight and the alert-path test

`sr-cli preflight` answers "is this box ready for the first invite?" (RDY-OPS-004). It checks the database and migrations, an object storage round trip (put, get, presign, multipart create and abort), the mail relay (connect only), the public URL and its headers (HSTS, CSP, `frame-ancestors`), TURN (credentials derive and an allocation succeeds over UDP), the Web Push key, free disk space, clock skew against the database, secret file modes (0600 or 0400) and, when on, Esplora and the voice node. It prints a table of PASS, WARN and FAIL rows with a sentence each and exits 1 when any row fails; warnings do not fail it. `--json` prints the report as JSON, `--insecure-tls` accepts a private CA when fetching the public URL (the smoke stack), `--send-to you@example.org` also sends a test mail through the relay.

The same checks run from Admin > System ("Preflight" card, `POST /admin/system/preflight`, `OPERATOR`) and as the last step of the Setup checklist on the Admin dashboard; each run is written to the access log (`system.preflight`). Set `SR_PREFLIGHT_INSECURE_TLS=1` on the server to let that card accept a private CA.

`sr-cli preflight --alert-test` (RDY-OPS-006) posts a test alert through the real staff feed path: the alert is stored, a card that names the host, the instance and the UTC time is posted to `#alerts` of the Staff Server, and, when Admin > Settings has the Owner email for alerts on, the Owner is mailed. The table gets an "Alert card" and an "Alert mail" row. The command is a separate process, so connected clients see the card the next time they open the channel, not instantly. Without a Staff Server (no `bootstrap` yet) the card row fails.
