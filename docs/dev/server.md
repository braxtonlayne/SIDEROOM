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
| `voice.enabled` | `false` | Starts the embedded SFU (WebSocket at `/voice/ws` on the public listener, media on UDP). Everything below applies only when on. |
| `voice.udp_port_min`, `voice.udp_port_max` | `50000`, `50099` | Media UDP range (at most 256 sockets bound). Open it in the firewall. |
| `voice.egress_budget_mbps` | `0` | Egress budget of the SFU node for the egress governor. Above it, simulcast layers are capped (server level 0 first, Patrons last) and clients get a "Reduced quality: the room is busy" notice. `0` is off. Use about 70 % of the measured uplink. |
| `voice.video_bwe` | `false` | Per-subscriber TWCC bandwidth estimation for choosing video layers. Off by default because it makes the SFU pad every sending connection to 50 kbit/s (audio-only ones too); the fallback uses receiver-report loss. Turn on for nodes that mostly carry video. |
| `voice.shards` | `1` | Media actors the SFU spreads rooms over (by channel id), each with its own share of the UDP sockets and of `voice.egress_budget_mbps`. Stays `1` (the single-actor SFU) until the AX42 run is recorded here; then it becomes `0`. `0` = half the available cores, at most 8; capped by the number of bound media sockets, so give the node a port range of at least that many ports. See `docs/dev/handoff/feat-p13-voice.md` for measured numbers. |
| `voice.e2ee` | `true` | End-to-end encryption for calls (D-053, D-249): the gateway is the MLS delivery service over op 101 `E2EE_MLS` (relay, commit ordering, external-sender removals) and tells each joining client `sr_e2ee: true` in `VOICE_SERVER_UPDATE`. While on, a join that does not declare E2EE is refused with `e2ee_required` (D-263): browsers without an encoded-frame transform API (see the voice spec, section 5, "Browsers") and bots whose voice client does not speak MLS cannot join calls. `false` makes calls DTLS-SRTP only; web clients follow the server, no rebuild needed (doc 12, section 3). On by default since the media path passed in Chromium and Firefox in CI (VOX-E2E-003). |
| `voice.tcp_port` | `0` (off) | ICE-TCP passive port for clients that cannot use UDP. |
| `voice.announce_ips` | `["127.0.0.1"]` | Addresses advertised as ICE candidates: set the node's public IP(s) in production. Firefox failed ICE against the `127.0.0.1` default in CI (it pairs loopback only with loopback, and may gather only the default-route address): to test with Firefox locally, announce a LAN address, e.g. `SR_VOICE__ANNOUNCE_IPS="$(node scripts/ice-announce.mjs)"`. |
| `voice.bind_ip` | `0.0.0.0` | Interface for the media sockets. |
| `voice.endpoints` | derived | Public `wss://` URL(s) of SFU nodes; default `ws(s)://<public_url host>/voice/ws`. |
| `voice.token_secret` | derived | Join-token secret (16+ chars); empty derives one from `server.secret_key`. Must match across nodes. |
| `voice.max_participants`, `voice.max_room_size` | `2000`, `99` | SFU capacity limits. |
| `voice.session_grace_secs` | `20` | How long someone stays in a voice channel after their gateway socket drops. |
| `voice.turn.urls`, `voice.turn.secret`, `voice.turn.ttl_secs` | empty, empty, `3600` | `stun:`/`turn:` URLs and the coturn `static-auth-secret` (dev stack: `dev-turn-shared-secret`). |

Voice metrics on `/internal/metrics`: `sr_voice_rooms`, `sr_voice_participants`, `sr_voice_egress_bits_per_second`, `sr_voice_ingress_bits_per_second`, `sr_voice_music_egress_bits_per_second` (the music tracks' share of the egress, MUS-JB-001), `sr_voice_packet_loss_ratio`, `sr_voice_forwarded_packets_total`. Voice state is held in memory on the node; see `docs/specs/lounge-voice-video.md` section 4.

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

## Preflight (RDY-OPS-004 to 006)

`crates/sr-preflight` holds the checks as functions over small probe traits (`DbProbe`, `NetProbe`, `HostProbe`, the `ObjectStorage` trait); `probes` has the real implementations, tests use fakes. Two callers: `sr-cli preflight` (`crates/sr-cli/src/ops.rs`) and `POST /admin/system/preflight` (`PreflightRunner` in `AdminDeps`, implemented by `sr_server::preflight::LivePreflight`). `--alert-test` calls `sr_server::staff_feed::alert_test`, which builds a minimal `StaffFeed` over the pool and runs `deliver_alert` with the new `StaffAlertKind::AlertTest`. TURN allocation is a small RFC 5766 client in `sr_preflight::turn` (MD5 is implemented locally for the long-term credential key; no new dependency).

**Row ids are the stable key (D-356).** `sr_preflight::CHECK_IDS`, in table order: `database`, `migrations`, `clock`, `storage`, `smtp`, `public-url`, `turn`, `vapid`, `disk`, `secrets`, `esplora`, `voice`, `web` (SEC-ENT-003: the web root the server serves the app from; a warning when `web.root` is unset, a failure when `app.html` or `app/csp.txt` is missing); `sr-cli` adds `mail-send` (`--send-to`), `alert-card` and `alert-mail` (`--alert-test`) (`CLI_EXTRA_IDS`). The unit test `check_ids_are_stable` pins the set, so a rename is deliberate; `deploy/scripts/prod-smoke.sh` keys on them too. `title` and `detail` are English operator text and are not localized (the OpenAPI schema says so). When a second locale lands, titles become `admin.preflight.row.<id>` client strings with the server title as the fallback; details stay English, like the probe errors.

**Insecure TLS (D-357).** `SR_PREFLIGHT_INSECURE_TLS=1` (the Admin card) is honoured only when `server.public_url` is a development host (`sr_core::config::is_development_host`: `localhost`, `127.*`, `[::1]`, `*.localhost`); otherwise `sr-server` refuses to start. The CLI's `--insecure-tls` works on any host. Either way a run that skipped certificate verification reports the public-URL row as WARN ("certificate not verified (insecure TLS)"), never PASS.
