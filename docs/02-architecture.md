# 02 — Architecture

## 1. Shape of the system

SIDE ROOM is a **modular monolith** plus a few specialized processes. At this scale (≤ 20k accounts, ~2k concurrent), one well-built Rust binary outperforms a microservice mesh and is far easier to operate. Module boundaries are still enforced as separate crates, so a module can be split out later if it ever needs to be.

```
                         ┌───────────────────────── Browser / PWA ─────────────────────────┐
                         │ React + TS app (apps/web)  ·  Rust→WASM modules (markdown,       │
                         │ E2EE frame crypto, theme validator, permissions, tools)          │
                         └──────┬───────────────┬──────────────────┬───────────────┬────────┘
                         HTTPS REST      WebSocket (gateway)   WebRTC (UDP/TCP)   HLS (HTTPS)
                                │               │                  │               │
                         ┌──────▼───────────────▼──────┐   ┌───────▼──────┐ ┌──────▼───────┐
   Caddy / reverse proxy │        sr-server (Rust)      │   │ sr-voice     │ │ sr-server     │
   (TLS, HTTP/3) ───────►│  REST API · Gateway · Jobs   │◄─►│ (Rust SFU,   │ │ /media route  │
                         │  Auth · Features · Search    │   │  str0m)      │ │ (signed URLs) │
                         └──┬─────────┬─────────┬───────┘   └──────┬───────┘ └──────┬───────┘
                            │         │         │                  │ TURN fallback  │
                     ┌──────▼──┐ ┌────▼────┐ ┌──▼────────┐   ┌─────▼─────┐    ┌─────▼──────┐
                     │Postgres │ │ Tantivy │ │ Garage    │   │ coturn    │    │ Garage     │
                     │ (sqlx)  │ │ indexes │ │ (S3, Rust)│   │ (TURN)    │    │ (segments) │
                     └─────────┘ └─────────┘ └──┬────────┘   └───────────┘    └────────────┘
                                                │ S3 API
                     ┌──────────────────────────▼───────────────────────────────────┐
                     │ sr-transcoder workers (Rust + FFmpeg/NVENC) on rented GPUs   │
                     │ sr-payments watcher (Rust, BDK) — may run inside sr-server   │
                     └──────────────────────────────────────────────────────────────┘
```

## 2. Processes

| Process | Crate | Responsibilities | Scales by |
|---|---|---|---|
| `sr-server` | `crates/sr-server` | REST API, WebSocket gateway, auth, background jobs, search, media URL signing, TV playlist generation, HLS serving, payment watcher (embedded by default), the gated app files under `/app` (section 8), Activity bundles on their own origins | Vertical first. Horizontal later via the `EventBus` trait (see §5) |
| `sr-voice` | `crates/sr-voice` | WebRTC SFU for voice/video/screen share. Forwards E2EE frames it cannot decrypt. Also the guest-play lobby relay for Activity guest links (`sr_voice::lobby::LobbyHub`, in memory, D-415, D-417). The crate is a library and a standalone binary; the shipped compose file runs the SFU **inside `sr-server`** (`voice.enabled`, `sr_voice::router` merged in `crates/sr-server/src/app.rs`, UDP 50000-50099), and `deploy/docker/voice.Dockerfile` builds the separate binary | One SFU per host. Rooms are pinned to a node |
| `sr-transcoder` | `crates/sr-transcoder` | Pulls transcode jobs, uses FFmpeg (NVENC when available) to produce HLS ladders, uploads to S3, reports back | Run 0–N anywhere (rented GPU, home PC) |
| `garage` | external (Rust) | S3-compatible object storage for attachments, avatars, manga pages, HLS segments, originals | Add nodes |
| `postgres` | external | System of record | Vertical. Read replica optional |
| `coturn` | external (C) | TURN relay for clients behind restrictive NATs | Rarely needed |
| `electrs`, `prometheus`, `grafana` | external | Optional services of `deploy/compose/prod.yml`: a self-hosted Esplora server for the payment watcher, and metrics with dashboards (`deploy/monitoring`) | — |
| `caddy` | external (Go) | TLS termination, HTTP/3, serving the public (signed-out) files, reverse proxy to `sr-server` for everything else (`deploy/compose/caddy`) | — |

Non-Rust externals are allowed only as listed in [03 §4](03-tech-stack-and-rust-policy.md#4-allowed-non-rust-components).

## 3. Crate map (Cargo workspace)

| Crate | Kind | Purpose |
|---|---|---|
| `sr-core` | lib | Domain types, Snowflake IDs, permission bitfields, errors, time, config types (`figment`: TOML + env) |
| `sr-db` | lib | sqlx pool, migrations, repositories (one module per aggregate) |
| `sr-auth` | lib | Password hashing (argon2id), sessions, TOTP, passkeys (own WebAuthn verifier, D-161), invite redemption. The Discord OAuth login lives in `sr-api` |
| `sr-features` | lib | Feature registry, settings schemas, feature flags (see doc 11) |
| `sr-events` | lib | `EventBus` trait. In-process implementation; optional multi-node implementation |
| `sr-gateway` | lib | WebSocket protocol, sessions, resume, intents, fan-out, presence, typing |
| `sr-api` | lib | axum routers for REST v1, OpenAPI generation via utoipa |
| `sr-chat` | lib | Guilds, channels, messages, threads, forums, polls, reactions, emoji, stickers, invites, AutoMod engine |
| `sr-markdown` | lib | Discord-flavored markdown parser and AST. Same code on server (search indexing, mention extraction) and client (rendering, via `sr-markdown-wasm`) |
| `sr-search` | lib | Tantivy indexes: messages, library titles, members |
| `sr-storage` | lib | Object storage abstraction (`object_store` crate, S3 backend), signed URLs, multipart uploads |
| `sr-media` | lib | The Cinema media pipeline as pure logic: probe (`ffprobe`/`ffmpeg` through `CommandRunner`; `ffmpeg-next` only behind the `native-ffmpeg` feature), transcode job orchestration, HLS manifests |
| `sr-cinema` | lib | The Cinema catalog (titles, seasons, episodes, assets) and the drafts behind Cinema submissions |
| `sr-cinema-social` | lib | Per-account Cinema state: watch progress, lists, ratings, reviews, comments, trending, recommendations |
| `sr-reader` | lib | Library ingest naming parser and reading helpers |
| `sr-library` | lib | The Library catalog and ingest (CBZ/ZIP/RAR/7z/PDF/EPUB), uploads, page conversion to WebP, storage collector |
| `sr-library-rules` | lib | The pure archive rules of Library ingest, shared by the server and the browser pre-checks |
| `sr-library-social` | lib | Per-account Library state: ratings, reviews, reading lists, rankings, unread counts |
| `sr-tv` | lib | Channel definitions, schedule engine, virtual-linear HLS playlist generation |
| `sr-watchparty` | lib | Watch Parties: the playback state machine and party service |
| `sr-review` | lib | Shared submission/review pipeline for media, reading, apps, themes, extensions |
| `sr-payments` | lib | Invoices, BTC address derivation (in-house watch-only on `secp256k1`, D-207), Esplora confirmation watching, perk entitlements |
| `sr-voice-proto` | lib | Signaling messages between gateway and SFU |
| `sr-e2ee` | lib | MLS group management (OpenMLS) + frame encryption for calls. Compiled to WASM by `sr-e2ee-wasm` |
| `sr-theme` | lib | Theme package schema, validator, CSS-variable compiler. Compiled to WASM by `sr-theme-wasm` |
| `sr-admin` | lib | The platform admin backend: staff permissions, appointments, bans, audit log, invite graph, announcements, the settings registry (`settings.rs`) |
| `sr-moderation` | lib | Reports, cases, holds, appeals, the takedown register, graduated restrictions and subtree cases |
| `sr-vouch` | lib | Profile vouches and the accountability multiplier (MOD-VCH-001) |
| `sr-apps` | lib | The developer platform: applications, bot accounts, credentials, installs, OAuth2, release review |
| `sr-activities` | lib | Activities and the Arcade: sandboxed bundles, code signing, instances, guest links |
| `sr-packages` | lib | Declarative packages (extensions, themes, profile layouts) and customised profiles |
| `sr-userfeat` | lib | The user-facing feature modules (scheduled messages, reminders, bookmarks, leveling, RSS, tickets, tools, music, scrapbook and more) |
| `sr-bridge` | lib | The Discord bridge (BRG-COR-001): channel maps, sealed bot token, outbound queue |
| `sr-push` | lib | Web Push: VAPID keys, message encryption, delivery |
| `sr-unfurl` | lib | SSRF-safe link previews and the third-party image proxy |
| `sr-net` | lib | The one place outbound HTTP to operator- or user-influenced URLs is made safe (XC-SEC-003) |
| `sr-preflight` | lib | "Is this box ready for its first invite?" checks, run by `sr-cli preflight` and `POST /admin/system/preflight` |
| `sr-memcount` | lib | Test helper: a counting global allocator for memory budget tests |
| `sr-server` | bin | Wires everything together |
| `sr-voice` | bin | SFU |
| `sr-transcoder` | bin | Transcode worker |
| `sr-cli` | bin | Operator CLI: bootstrap Owner, create first invite, backups, re-index, migrate, preflight, `storage ensure-cors`, moderation drill |
| `sr-loadtest` | bin | Load test suite for a running `sr-server` (XC-PERF-003) |
| `sr-sdk` | lib | Rust bot SDK (Discord-like ergonomics): REST client, gateway shards, cache, command framework, optional voice |
| `sr-markdown-wasm`, `sr-permissions-wasm`, `sr-e2ee-wasm`, `sr-theme-wasm`, `sr-library-wasm`, `sr-music-wasm`, `sr-graph-wasm`, `sr-tools-wasm` | lib (cdylib) | The WASM modules the browser loads. `packages/wasm` builds every `crates/*-wasm` crate with `wasm-pack` |

The TypeScript side:

| Package | Purpose |
|---|---|
| `apps/web` | The SPA/PWA. Two builds (D-385, [entrance isolation](dev/entrance-isolation.md)): the public build (`vite.public.config.ts`; the landing, auth, invite, profile, guest-play and legal surfaces, each a minimal bundle or plain HTML, listed in `public-surfaces.json`) and the gated app build (`app.html`, `dist/app/*`, served by `sr-server` only to a signed-in session). `/app` is the neutral door into the app |
| `packages/api-client` | Generated from OpenAPI, plus hand-written gateway client |
| `packages/ui` | Design-system components (Lounge kit + Side Room kit) |
| `packages/wasm` | Built bindings for the `crates/*-wasm` modules |
| `packages/markdown-react` | React renderer for `sr-markdown` output |
| `packages/profile-blocks` | Profile block components shared by the profile page and its editor |
| `packages/embedded-app-sdk` | The SDK an Activity or Arcade game bundle uses to talk to its host |
| `packages/bot` | The JS/TS bot SDK shaped like discord.js (`@sideroom/bot`) |
| `packages/starter-packs` | Example and built-in packs, including the Activities under `starter-packs/activities/*` |
| `e2e` | Playwright end-to-end tests, and `e2e/sdk-compat` (the SDK compatibility run) |

## 4. Request paths

### 4.1 REST
`Browser → Caddy → sr-server (axum) → service fn → repository (sqlx) → Postgres`. Mutations emit domain events on the `EventBus`. The gateway turns those into dispatches for subscribed sessions.

### 4.2 Gateway
- One WebSocket per client session at `/gateway?v=1&encoding=json`. JSON is the only encoding (another value answers `400`); `compress=zstd-stream` is an optional transport compression (`crates/sr-gateway/src/compress.rs`).
- Discord-shaped opcodes (`HELLO`, `IDENTIFY`, `READY`, `DISPATCH`, `HEARTBEAT`, `RESUME`, …). See doc 05.
- Fan-out: each guild has an in-memory subscriber set. Dispatches are filtered per recipient by channel-view permission before sending.
- Presence and typing live purely in memory. They are rebuilt on reconnect.
- **Resync (D-646).** When a pump misses events because the event bus lagged, it first re-checks credentials, drops caches, rebuilds the member list and reconciles memberships (D-628). It then re-sends state to the sessions that are already connected, on the same socket and session id: a fresh `READY` and `GUILD_CREATE`s for a user topic, a per-member `GUILD_CREATE` and an `SR_RESYNC` marker for a server topic, the marker alone for the platform topic. Rounds have a floor per key and a node-wide cap, and bot sessions are not resynced (`crates/sr-gateway/src/resync.rs`).

### 4.3 Voice
1. Client sends gateway op `VOICE_STATE_UPDATE` (join channel).
2. `sr-server` checks permissions and sender allowance, picks an `sr-voice` node, mints a short-lived voice token, and dispatches `VOICE_SERVER_UPDATE`.
3. Client opens WebRTC to `sr-voice` with the token. SFU validates it with a shared HMAC key.
4. MLS group commits for E2EE are relayed through the gateway (the SFU forwards opaque frames). See the voice spec.

### 4.4 Media playback
- Catalog via REST. Playback: `GET /cdn/hls/{asset_id}/master.m3u8?ex=&sig=` returns a manifest whose rendition, audio, subtitle and segment URLs are signed and time-limited (cinema-media §10.3).
- Segments are streamed from Garage through `sr-server` (or served directly from Garage's web endpoint with signed URLs). Caddy caches hot segments.
- Live TV: `GET /tv/channels/{channel_id}/live/master.m3u8` (with `live/audio.m3u8` and `live/v/{file}` beside it) is **generated** each request from the schedule. It stitches the current program's existing VOD segments starting at the wall-clock offset, using `#EXT-X-DISCONTINUITY` between programs. No runtime transcoding, and nothing runs when nobody requests the playlist (D-100, D-101).

### 4.5 Uploads
- Chat attachments: client requests an upload slot (`POST /channels/{id}/attachments`). Server checks the per-user limit (30 MB / 500 MB) and returns a presigned S3 PUT (multipart for > 64 MB). Client uploads, then posts the message referencing the upload IDs.
- The presigned PUT goes from the browser straight to the bucket, so **the bucket needs a CORS rule for the app origin** (D-175): `PUT, GET, HEAD` from `server.public_url`, `content-type` and `range` allowed (wave 30: the floating player streams and the tag reader reads by byte range), `ETag`, `Content-Range`, `Accept-Ranges` and `Content-Length` exposed. The check and the daily recheck (`storage.cors_recheck`) require everything except `range`, so a rule written before wave 30 keeps passing; simple `bytes=a-b` ranges are CORS-safelisted, and the next apply adds `range` for suffix ranges and older browsers. `sr-cli storage ensure-cors --origin <public_url>` applies it (idempotent; the rule is replaced, not appended) and the Garage bootstrap script and the install walkthrough call it. The presigner signs method and path only, so the browser's `Content-Type` is advisory and the server sniffs the real type when the upload is finalised. The call is a hand-signed SigV4 `PutBucketCors` in `sr-storage::s3admin`, because `object_store` only presigns object requests.
- Activity packages: the zip goes to the bucket in parts (the Library's resumable upload) and a job checks it from the bucket, so no package is held in memory; a single-request upload stays for a zip of at most 32 MiB. The size limit has three tiers set by operator settings (`activities.package.max_mb`, `reviewed_max_mb`, `platform_max_mb`), and Activity files are served as range streams (D-654..D-657).
- Library submissions: resumable multipart uploads into a `pending/` prefix, any size. Processing jobs run, and the item lands in the review queue (see the submissions spec).
- A member's own mount gets the same rule from Apply CORS (BYOS-MNT-003).

### 4.6 Third-party images
- The client never loads a third-party image URL by itself (D-176). When a message is rendered, the server adds a `proxy_url` of the form `/cdn/external/{hmac}/{url_b64}` to every http(s) embed image (doc 05 section 3). `GET` on that path is the image proxy: HMAC-checked, fetched with the SSRF-safe fetcher of `sr-unfurl` (addresses checked after DNS and on every redirect, pinned connect, 10 MiB and 10 s), sniffed, decoded and re-encoded without metadata, cached in the bucket under `external/` for 30 days, and rate limited per address, per user and globally. The details and status codes are in doc 05 section 5.2.
- Until a reader clicks "Load image", an embed shows its text only. A click loads the original from the other site without a referrer.

## 5. Events and scale-out

`sr-events` defines:
```rust
#[async_trait]
pub trait EventBus: Send + Sync {
    async fn publish(&self, topic: Topic, event: DomainEvent) -> Result<()>;
    fn subscribe(&self, topic: Topic) -> BoxStream<'static, DomainEvent>;
}
```
- **v1:** `InProcessBus` (tokio broadcast channels). Enough for one `sr-server` node to handle the 20k-account ceiling.
- **Later, if ever needed:** `PgNotifyBus` (Postgres LISTEN/NOTIFY, no new dependency) for 2–3 nodes. Beyond that, a NATS-backed bus behind the same trait, which would need an ADR because NATS is Go.

Caches use `moka` (Rust, in-process). There is no Redis.

## 6. Background jobs

A Postgres-backed job queue (`jobs` table, `SELECT … FOR UPDATE SKIP LOCKED`) is implemented in `sr-db::jobs`. Job kinds include: thumbnail generation, manga page conversion, transcode dispatch (external transcode workers claim jobs with `POST /workers/{worker_id}/claim`), search indexing, message purge (30-day), large-attachment expiry (7-day), donation entitlement expiry and grace, invoice confirmation polling, invite-graph suspicion evaluation, scheduled-event reminders, storage indexing and measuring (`storage.index`, `storage.measure`), status-ring expiry, invite pruning (`invites.prune`), account data export and its sweep (`user.data_export.sweep`), the bulk message self-delete (`user.messages_self_delete`), moderation restriction sweeps, card reminders, Activity package checking and sweeping (`activities.package_ingest`, `activities.package_sweep`, D-654), Activity garbage collection (`activities.gc`), the Arcade manifest recheck (`arcade.manifest_recheck`), the bug report retention sweep (`bugs.retention`, D-647..D-650) and the abandoned-upload sweep (`storage.uploads_sweep`). Each module registers its kinds with the job runner in `crates/sr-server/src/app.rs`.

## 7. Identifiers

Snowflake IDs (64-bit, Discord layout: 42-bit ms timestamp since the **SIDE ROOM epoch 2026-01-01T00:00:00Z**, 5-bit worker, 5-bit process, 12-bit increment). They are serialized as strings in JSON, as Discord does.

## 8. Security model (summary)

- TLS everywhere. HSTS. Strict CSP, with two policies (D-385): the public shells get a plain one (no `wasm-unsafe-eval`, no `blob:`, no frames) and the app shell adds `'wasm-unsafe-eval'`. **No app code is served before sign-in**: a visitor without a session receives only the public surfaces, and `/app/*` answers `302 /login` or `401` the same whether a file exists or not. **Staff code is served only to staff** (D-651, D-652): the build puts every chunk reachable only through the Admin app under `/app/staff/`, and `sr-server` (`crates/sr-server/src/web.rs`) serves that directory only to a session with an active staff appointment, read from the database on every request. Anyone else gets the same `404` as for a missing file, and the files carry `Cache-Control: private, no-store`. The message catalog's staff wording is not gated (D-658). Activities run in sandboxed iframes on a cookieless origin per application (`<application-id>.activities.<host>`, reviewed bundles only; a test-only `same_host_sandboxed` mode exists, D-450). Guests of a game see only their own session (D-397).
- Sessions: opaque random tokens (256-bit) stored hashed. Cookie `HttpOnly; Secure; SameSite=Lax` for the web app. Bearer tokens for bots (`Bot <token>`).
- Every permission check happens server-side through `sr-core::permissions::compute(member, channel)`.
- Rate limits: `governor` crate, keyed per route bucket and per user/IP, returning Discord-style headers (`X-RateLimit-*`).
- Staff access to private content is logged to an append-only, hash-chained `staff_access_log`.
- Object URLs are signed (HMAC, short TTL). Buckets are never public.

## 9. Observability

`tracing` + `tracing-subscriber` (JSON logs), Prometheus metrics via the `metrics` crate (`/internal/metrics`, on `server.metrics_bind`, default `127.0.0.1:9100`; a non-loopback bind needs `server.metrics_token`, sent as a bearer token). There is no OpenTelemetry exporter. Health endpoints: `/healthz` (liveness) and `/readyz` (DB + storage reachable).
