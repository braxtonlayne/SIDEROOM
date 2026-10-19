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

1. Provision the server and point DNS (`A`/`AAAA` for `<host>`, and `*.activities.<host>` only when `activities.enabled` is on). If activities are on or voice signalling uses another origin, run `node deploy/scripts/render-caddyfile.mjs --public-url https://<host> [--activities] [--voice-url <url>]` and build the web app with `VITE_PUBLIC_URL`, `VITE_ACTIVITIES_ENABLED=true` and `VITE_VOICE_PUBLIC_URL` set to the same values, so the Content-Security-Policy in `Caddyfile.prod` and in the app cannot drift (OQ-XC-3).
2. `curl -fsSL <release>/install.sh | sh`. This downloads release binaries or pulls images, creates the `sideroom` user, writes `/etc/sideroom/config.toml` from prompts, and generates secrets.
3. `sr-cli migrate` → `sr-cli bootstrap --owner-username … --owner-email …` (doc 06 §8).
4. Open the printed one-time link, log in as Owner, and finish the **Setup Wizard** in Admin → Setup: it checks and tests SMTP and Discord OAuth from the config, and lists the two redirect URIs to register on the one Discord application (sign-in: `<public_url>/api/v1/sr/auth/discord/callback`; account linking: `<public_url>/api/v1/connections/discord/callback`, shown with a copy button under Admin → System → Integrations, M-05, D-292), takes the BTC descriptor (optional) and the default limits, runs the storage check and the voice connectivity test; it writes no secret (D-254).
5. Create the first platform invites from your account.

**Switching Activities on (off by default).** Activities run each on `https://<application-id>.activities.<host>/`, served by `sr-server` from the bundle staff reviewed, so turning them on is four things that must agree: set `activities.enabled = true` (`SR_ACTIVITIES__ENABLED=true`); build the web app with `VITE_ACTIVITIES_ENABLED=true` (the call bar and the composer offer Activities only then); re-render the Caddyfile with `--activities` so the wildcard site block reverse-proxies `*.activities.<host>` to `sr-server` and the app's CSP `frame-src` lists the wildcard; and add the DNS wildcard `*.activities.<host>` plus a certificate that covers it (Caddy needs a DNS-challenge provider for a wildcard certificate). Uploaded bundles are stored in the same object storage as every other upload under `activities/` and need no other setting; the hourly `activities.gc` job deletes the files of superseded bundles. Turn the flag off and every activity host answers 404 at once, the shelf and the launcher disappear and nothing else changes.

**Donations: chain and price sources (D-288, M-02).** `SR_PAYMENTS_ESPLORA_URL` is empty by default, which keeps donations off. The `payments` Compose profile (`docker compose --profile payments up -d`, or answer yes to the installer's electrs question, which writes `COMPOSE_PROFILES=payments`) runs an Esplora-compatible `electrs` on the private compose network (no published port) and the installer then sets the URL to `http://electrs:3000` together with `SR_PAYMENTS_ALLOW_PRIVATE_HOSTS=1`, because the SSRF-safe client refuses private addresses unless the operator opts in for the payments client. Derived invoice addresses then never leave the box. `electrs` needs your own fully synced, non-pruned `bitcoind` (about 700 GB today; point `SR_ELECTRS_DAEMON_RPC_ADDR` and `SR_BITCOIND_DIR` at it) plus its own index (about 50 GB) and the first sync takes days; the service in `deploy/compose/prod.yml` carries a placeholder image tag that must be pinned by digest before the first start. Until it is synced, the documented interim is `SR_PAYMENTS_ESPLORA_URL=https://mempool.space/api` (mempool.space then learns which addresses this server watches). `SR_PAYMENTS_RATE_SOURCES` defaults to three keyless public APIs, the median of which is used: Coinbase spot, Kraken ticker and CoinGecko simple price (the JSON is in `.env.prod.example`). Admin → Donations shows each source's last answer, age and error.

## 3. Configuration

`/etc/sideroom/config.toml` (figment, overridable by `SR_*` env vars). Contains infrastructure only: DB URL, storage endpoint and keys, domains, secrets, voice ports, TURN shared secret. **Everything product-level** (limits, prices, feature toggles, cap) lives in the database and is edited in Admin pages, so changing it never needs a restart. Two feature schemas hold the platform defaults an administrator edits (D-254): `platform.appearance` (the default theme of each section, `GET /sr/appearance/defaults`; a member's own choice wins) and `platform.integrations` (the non-secret policy: `unfurl` deny and allow hosts and per-host rate, `gif` region and locale, `lookup` providers offered). The config keys of the same names are the bootstrap value and a saved database row wins and applies at once, without a restart (D-300: the link-preview hosts and rate, the GIF region and locale and the lookup providers are read from a swappable in-process handle; clients, keys and endpoints stay startup-only). Secrets and endpoints (`smtp.*`, `discord.client_secret`, `gif.api_key`, `lookup.*_api_key`) stay file-based operator config, shown in Admin → System only as configured or not, with the test send.

**Local import folder (H-48).** The Compose stack mounts one optional folder, `${SR_INGEST_DIR:-/srv/sideroom/ingest}`, read-only into `sr-server` at `/srv/sideroom/ingest` and sets `SR_INGEST__ROOTS` to it, which is what lets staff point a watched folder at it (REV-UP-005); an empty `SR_INGEST_ROOTS` leaves local folders off. One root, a `library/` sub-folder per the naming conventions, read-only, and the ledger never moves files. `deploy/install.sh` asks for the folder and creates it when missing. The Storage Box stays a backup target; mounting it as the ingest folder is the owner's action on the production box.

### 3.1 Secrets: files, stored credentials and rotation (XC-SEC-004)

**File-based secrets.** Every secret config key can be supplied from a file instead of the environment, which keeps it out of `/proc/<pid>/environ`, `docker inspect` and shell history. Set `SR_<KEY>_FILE` to the path (the key written as the env name, `.` becoming `__`): `SR_SERVER__SECRET_KEY_FILE`, `SR_DATABASE__URL_FILE`, `SR_SMTP__PASSWORD_FILE`, `SR_STORAGE__ACCESS_KEY_FILE`, `SR_STORAGE__SECRET_KEY_FILE`, `SR_VOICE__TOKEN_SECRET_FILE`, `SR_VOICE__TURN__SECRET_FILE`, `SR_DISCORD__CLIENT_SECRET_FILE`, `SR_GIF__API_KEY_FILE`, `SR_LOOKUP__TMDB_API_KEY_FILE`, `SR_LOOKUP__COMICVINE_API_KEY_FILE`, and `SR_SERVER__SECRET_KEY_PREVIOUS_FILE` (one key per line). The standalone voice node reads `SR_VOICE_TOKEN_SECRET_FILE`, the transcode worker `--token-file`. Rules: the file's trailing newline is trimmed; a missing or empty file is a startup error; setting both `SR_X` and `SR_X_FILE` is a startup error (so a stale plain value cannot shadow a rotated file). Use Docker/Kubernetes secrets or systemd `LoadCredential=` and mode 0600/0400 files owned by the service account. The list of keys is `sr_core::config::SECRET_FILE_KEYS`; a test fails if a new secret field is added without it.

**Stored credentials.** Nothing that grants access is stored in a recoverable form.

| Credential | Stored as |
|---|---|
| Account password | argon2id PHC string (`accounts.password_hash`) |
| Session token, trusted-device token, email verification/reset token | SHA-256 digest (`sessions`, `trusted_devices`, `email_tokens`; `token_hash`) |
| Recovery codes | argon2id PHC / SHA-256 digest (`recovery_codes`) |
| Bot token, interaction token, OAuth2 access/refresh token and authorization code, application client secret | SHA-256 digest (`bot_tokens`, `interactions`, `oauth2_*`, `applications.client_secret_hash`) |
| Webhook token | SHA-256 digest (`webhooks.token_hash`) |
| Transcode worker token | SHA-256 hex digest (`worker_tokens.token_sha256`) |
| TOTP secret | AES-GCM ciphertext under `server.secret_key` (`secret_enc`); it must be recoverable to verify codes |
| Interactions signing seed (bots with an HTTP endpoint) | AES-GCM ciphertext under `server.secret_key` (`applications.interactions_signing_seed_sealed`, D-185); the public key is derived on read |
| Passkeys | public key only (`passkey_credentials.cose_key`) |
| Platform invite, server invite, server template and watch-party codes | plaintext, deliberately: shareable identifiers the issuer must be able to re-display; they grant a narrow, revocable, rate-limited capability and never account access |

The crate test `crates/sr-db/tests/secret_columns.rs` scans every migration and fails if a column named like a credential is neither a digest/ciphertext (`*_hash`, `*_sha256`, `*_phc`, `*_enc`) nor on the plaintext allow list above. Secrets shown once at creation (bot tokens, client secrets, worker tokens, webhook URLs) cannot be shown again; rotating means issuing a new one.

**Rotation.**

- `server.secret_key` (TOTP encryption): generate a new key (`openssl rand -hex 32`), move the old one to `server.secret_key_previous`, restart, then run `sr-cli rotate-secret-key` to re-encrypt every TOTP secret and every sealed interactions signing seed. After it reports zero remaining, remove the old key from `server.secret_key_previous`.
- `database.url` password: create the new password in Postgres, update the secret file, restart, then drop the old password. Use a separate Postgres role per service so each rotates independently.
- `storage.*` keys: create a second key pair at the provider, switch the secret files, restart, delete the old pair.
- `voice.token_secret`: the gateway and every SFU must share it. Change it on all nodes and restart them together; open voice sessions reconnect and fetch new join tokens. `voice.turn.secret` must match coturn's `static-auth-secret`; change both and restart coturn then the server.
- `smtp.password`, `discord.client_secret`: rotate at the provider, update the file, restart.
- Per-user and per-application credentials: bot tokens and client secrets are rotated from the Developer portal (the old value stops working immediately); worker tokens are revoked and re-issued in Admin → Transcoding; webhook tokens are reset from server settings; users revoke sessions in Settings → Security.
- After any suspected leak: rotate the secret, then revoke sessions (Admin → Accounts, or `sr-cli user reset-2fa <user>` for one account) if `server.secret_key` or the database was exposed.

**Voice end-to-end encryption (D-249, D-263).** Calls are end-to-end encrypted by default (`voice.e2ee = true`): the SFU and the server only see ciphertext. While it is on, a client that cannot encrypt is refused instead of joining in the clear. A browser without the encoded-frame transform API shows "Your browser can't join encrypted calls; use a current Chrome, Edge, Firefox or Safari.", and a bot whose voice client does not speak MLS gets `e2ee_required` (as with Discord's DAVE). Chromium (Chrome, Edge) and Firefox are tested in CI; Safari has the API (15.4 and later) but is untested (voice spec, section 5). **To turn it off,** set `voice.e2ee = false` (or `SR_VOICE__E2EE=false`) and restart `sr-server`. Clients follow the server: `VOICE_SERVER_UPDATE` then carries `sr_e2ee: false` and the web app joins calls without E2EE, with no rebuild. Calls stay encrypted in transit (DTLS-SRTP), but the voice node can read them, and the shield and privacy codes are not shown. The web build flag `VITE_SR_VOICE_E2EE=0` also exists. It removes E2EE from the build, so use it only together with `voice.e2ee = false`, or every join is refused.

**Outbound requests.** All fetches of URLs a user or remote server influenced (link previews, the image proxy, payment rate and chain sources) go through the shared SSRF-safe client in `crates/sr-net`: only http/https, no embedded credentials, every resolved address checked against private/loopback/link-local/metadata ranges, the connection pinned to the checked address, redirects revalidated hop by hop (max 3), bounded size and time, no environment proxies. If your esplora node or rate source lives on a private network, set `SR_PAYMENTS_ALLOW_PRIVATE_HOSTS=1`; it applies only to the payments client.

**Upload hygiene.** Uploaded bytes are sniffed, never trusted by declared type. Profile and expression images accept only PNG, JPEG, GIF and WebP (re-encoded); SVG, HTML and XML are rejected there. Chat attachments of those formats are stored as `application/octet-stream` and served as downloads under `nosniff` and a sandboxing CSP, so they never render on the site origin.

## 4. Transcode workers

- `sr-transcoder` is a single static binary plus FFmpeg (with NVENC) in a Docker image `ghcr.io/<owner>/sideroom-transcoder:<ver>-cuda`.
- Run it on a rented GPU box (Vast.ai, RunPod, Hetzner GEX) or any home PC:
  `sr-transcoder --server https://<host> --token <worker-token> --concurrency 2`
- Workers authenticate with a worker token created in Admin → Transcoding. They claim jobs over HTTPS, download originals with signed URLs, encode, and upload renditions with presigned multipart PUTs. **No inbound ports are needed on the worker.**
- CPU fallback (x264) is automatic when no NVENC is detected (slow but works).
- Admin → Transcoding shows the queue, ETA, per-worker throughput, failures with logs, and a "rent a GPU" checklist.
- Images (CUDA and CPU), the run commands and the full "rent a GPU" guide are in [`deploy/TRANSCODER.md`](../deploy/TRANSCODER.md); the Worker API is in [`specs/cinema-media.md`](specs/cinema-media.md) section 4.4.

**The Live TV slate (D-266, D-269).** A channel with nothing on shows a colour-bars loop with a tone at -20 dBFS instead of a black screen. Make it once, on a machine that has ffmpeg, the database and the bucket configured, while sr-server and a transcoder worker run: `sr-cli tv slate` (options `--picture bars|moving`, `--seconds 12`, `--ffmpeg <path>`, `--wait-secs`). It draws a 1920x1080, 30 fps loop from ffmpeg `lavfi` sources only (nothing is downloaded), stores it as a system-owned asset outside the Cinema catalogue (no title, no guide entry, hidden from Admin -> Cinema assets), lets the ordinary probe and transcode jobs cut the standard ladder, waits for the renditions, writes the asset id to the platform setting `tv.slate_asset` and prints it. A running server follows the setting within seconds, no restart. A second run replaces the slate and removes the previous asset. `SR_TV_SLATE_ASSET` still works as a bootstrap fallback when the setting is empty. Admin -> Cinema -> Live TV shows whether a slate is set and plays. The command refuses, with a message, when ffmpeg is missing or `storage.endpoint` is `memory:`. `sr-cli tv slate --remove` is the inverse (D-286): it clears the setting, retires the asset and says so (it also reports a `SR_TV_SLATE_ASSET` fallback that is still set; unset the variable). Replacing the slate with a loop of another length can renumber the segments once for a client that is on it at that moment.

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
