# 12 — Deployment and Operations

Decision: D-153. Target: one operator, one rented dedicated server, an install anyone can follow.

## 1. Reference deployment (tier S, see doc 08 §6)

```
Hetzner AX42 (Ubuntu 24.04 LTS)
├── caddy            :80/:443 (TLS via ACME, HTTP/3)
├── sr-server        127.0.0.1:8080  (API, gateway, HLS, jobs, payments watcher)
├── sr-voice         UDP 50000-50100 + TCP 3479 (ICE-TCP), signaling via sr-server
├── coturn           UDP/TCP 3478, TLS 5349, relay range 49152-49999
├── postgres 17      127.0.0.1:5432  (NVMe RAID1)
├── garage           127.0.0.1:3900  (S3), data on NVMe (S) or on the storage server (S+)
└── backups → Hetzner Storage Box (restic/rustic over SFTP)
```

Everything ships as **Docker Compose** (`deploy/compose/`) **and** as systemd units with static binaries (`deploy/systemd/`). The operator picks either.

## 2. Install steps (documented in `deploy/INSTALL.md`)

1. Provision the server and point DNS (`A`/`AAAA` for `<host>` and `*.activities.<host>`).
2. `curl -fsSL <release>/install.sh | sh`. This downloads release binaries or pulls images, creates the `sideroom` user, writes `/etc/sideroom/config.toml` from prompts, and generates secrets.
3. `sr-cli migrate` → `sr-cli bootstrap --owner-username … --owner-email …` (doc 06 §8).
4. Open the printed one-time link, log in as Owner, and finish the **Setup Wizard** in Admin → Setup: SMTP, Discord OAuth app credentials (optional), BTC xpub (optional), storage check, voice connectivity test, and default limits.
5. Create the first platform invites from your account.

## 3. Configuration

`/etc/sideroom/config.toml` (figment, overridable by `SR_*` env vars). Contains infrastructure only: DB URL, storage endpoint and keys, domains, secrets, voice ports, TURN shared secret. **Everything product-level** (limits, prices, feature toggles, cap) lives in the database and is edited in Admin pages, so changing it never needs a restart.

## 4. Transcode workers

- `sr-transcoder` is a single static binary plus FFmpeg (with NVENC) in a Docker image `ghcr.io/<owner>/sideroom-transcoder:<ver>-cuda`.
- Run it on a rented GPU box (Vast.ai, RunPod, Hetzner GEX) or any home PC:
  `sr-transcoder --server https://<host> --token <worker-token> --concurrency 2`
- Workers authenticate with a worker token created in Admin → Transcoding. They claim jobs over HTTPS, download originals with signed URLs, encode, and upload renditions with presigned multipart PUTs. **No inbound ports are needed on the worker.**
- CPU fallback (x264) is automatic when no NVENC is detected (slow but works).
- Admin → Transcoding shows the queue, ETA, per-worker throughput, failures with logs, and a "rent a GPU" checklist.

## 5. Backups and restore

| What | How | Frequency | Retention |
|---|---|---|---|
| Postgres | `pg_dump` (custom format) + WAL archiving to the Storage Box | Nightly full, continuous WAL | 14 daily, 8 weekly, 12 monthly |
| Garage: attachments, avatars, manga pages | `rustic` (Rust restic-compatible) snapshot | Nightly | 30 days |
| Garage: video originals | Optional (large). Originals can be kept on the Storage Box instead of Garage | Weekly | Operator choice |
| HLS renditions | **Not backed up**. Re-creatable from originals by re-transcoding | — | — |
| Config + secrets | Encrypted tarball | On change | 10 versions |

`sr-cli restore --from <snapshot>` with a documented, tested restore drill (CI runs a restore test on a sample dataset weekly).

Shipped so far: `deploy/scripts/backup.sh`, `restore.sh` and `restore-drill.sh` (see [deploy/BACKUP.md](../deploy/BACKUP.md)), systemd timers for both, and `.github/workflows/backup-drill.yml`. `sr-cli backup` and `restore` are not written; the scripts are the interface. WAL archiving is not scripted.

## 6. Upgrades

Semantic-versioned releases. `sr-cli upgrade` takes a pre-upgrade DB snapshot, runs migrations, and restarts. Migrations are forward-only and must be backward-compatible with the previous release's binaries for one version (expand/contract), so rollbacks are safe.

## 7. Monitoring

- `/internal/metrics` (Prometheus format; `http_request_duration_seconds` is a real histogram with buckets around the 100 ms budget). Dashboards and alert rules live in `deploy/monitoring`, the on-call steps in [deploy/RUNBOOK.md](../deploy/RUNBOOK.md). An optional Compose profile adds Prometheus + Grafana with shipped dashboards: gateway sessions, REST latency, voice rooms/egress, job queue, storage usage, invoice states.
- Admin → System shows: disk usage, egress, active voice rooms, account count vs cap, job backlog, worker status, and the last backup and its verification.
- Alerts post to the Staff Server `#alerts` channel (and optional email).

## 8. Security hardening checklist

UFW (only 22/80/443/3478/5349/voice ports open), SSH keys only, unattended-upgrades, fail2ban (or `sr-server`'s own login throttling), LUKS disk encryption where the provider supports it, a separate Postgres role per service, and secrets with file mode 0600.

## 9. Operator obligations note

Before opening the instance to anyone beyond close friends, the operator should understand the legal obligations of their hosting jurisdiction: copyright takedown handling, illegal-content reporting duties, data-subject rights, and retention rules. SIDE ROOM's Phase 7 adds the tooling. Policy is the operator's responsibility. This section is intentionally a reminder, not legal advice.
