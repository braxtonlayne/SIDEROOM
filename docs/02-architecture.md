# 02 — Architecture

## 1. Shape of the system

SIDE ROOM is a **modular monolith** plus a few specialized processes. At this scale (≤ 20k accounts, ~2k concurrent), one well-built Rust binary outperforms a microservice mesh and is far easier to operate. Module boundaries are still enforced as separate crates, so a module can be split out later if it ever needs to be.

```
                         ┌───────────────────────── Browser / PWA ─────────────────────────┐
                         │ React + TS app (apps/web)  ·  Rust→WASM modules (markdown,       │
                         │ E2EE frame crypto, theme validator, image tools)                 │
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
| `sr-server` | `crates/sr-server` | REST API, WebSocket gateway, auth, background jobs, search, media URL signing, TV playlist generation, HLS serving, payment watcher (embedded by default) | Vertical first. Horizontal later via the `EventBus` trait (see §5) |
| `sr-voice` | `crates/sr-voice` | WebRTC SFU for voice/video/screen share. Forwards E2EE frames it cannot decrypt | One process per host. Rooms are pinned to a node |
| `sr-transcoder` | `crates/sr-transcoder` | Pulls transcode jobs, uses FFmpeg (NVENC when available) to produce HLS ladders, uploads to S3, reports back | Run 0–N anywhere (rented GPU, home PC) |
| `garage` | external (Rust) | S3-compatible object storage for attachments, avatars, manga pages, HLS segments, originals | Add nodes |
| `postgres` | external | System of record | Vertical. Read replica optional |
| `coturn` | external (C) | TURN relay for clients behind restrictive NATs | Rarely needed |
| `caddy` | external (Go) | TLS termination, HTTP/3, static asset serving, reverse proxy | — |

Non-Rust externals are allowed only as listed in [03 §4](03-tech-stack-and-rust-policy.md#4-allowed-non-rust-components).

## 3. Crate map (Cargo workspace)

| Crate | Kind | Purpose |
|---|---|---|
| `sr-core` | lib | Domain types, Snowflake IDs, permission bitfields, errors, time, config types |
| `sr-db` | lib | sqlx pool, migrations, repositories (one module per aggregate) |
| `sr-auth` | lib | Password hashing (argon2id), sessions, TOTP, WebAuthn, Discord OAuth, invite redemption |
| `sr-features` | lib | Feature registry, settings schemas, feature flags (see doc 11) |
| `sr-events` | lib | `EventBus` trait. In-process implementation; optional multi-node implementation |
| `sr-gateway` | lib | WebSocket protocol, sessions, resume, intents, fan-out, presence, typing |
| `sr-api` | lib | axum routers for REST v1, OpenAPI generation via utoipa |
| `sr-chat` | lib | Guilds, channels, messages, threads, forums, polls, reactions, emoji, stickers, invites, AutoMod engine |
| `sr-markdown` | lib + wasm | Discord-flavored markdown parser and AST. Same code on server (search indexing, mention extraction) and client (rendering) |
| `sr-search` | lib | Tantivy indexes: messages, library titles, members |
| `sr-storage` | lib | Object storage abstraction (`object_store` crate, S3 backend), signed URLs, multipart uploads |
| `sr-media` | lib | Video catalog, ingest, probe, job orchestration, HLS manifests, watch progress |
| `sr-reader` | lib | Reading catalog, archive import (CBZ/ZIP/RAR/7z/PDF/EPUB), page conversion to WebP |
| `sr-tv` | lib | Channel definitions, schedule engine, virtual-linear HLS playlist generation |
| `sr-review` | lib | Shared submission/review pipeline for media, reading, apps, themes, extensions |
| `sr-payments` | lib | Invoices, BTC address derivation (BDK watch-only), confirmation watching, perk entitlements |
| `sr-voice-proto` | lib | Signaling messages between gateway and SFU, shared with the client (via wasm) |
| `sr-e2ee` | lib + wasm | MLS group management (OpenMLS) + frame encryption for calls |
| `sr-theme` | lib + wasm | Theme package schema, validator, CSS-variable compiler |
| `sr-server` | bin | Wires everything together |
| `sr-voice` | bin | SFU |
| `sr-transcoder` | bin | Transcode worker |
| `sr-cli` | bin | Operator CLI: bootstrap Owner, create first invite, backups, re-index, migrate |
| `sr-sdk` | lib | Rust bot SDK (Discord-like ergonomics) |

The TypeScript side:

| Package | Purpose |
|---|---|
| `apps/web` | The SPA/PWA. Routes for all sections |
| `packages/api-client` | Generated from OpenAPI, plus hand-written gateway client |
| `packages/ui` | Design-system components (Lounge kit + Side Room kit) |
| `packages/wasm` | Built bindings for `sr-markdown`, `sr-e2ee`, `sr-theme`, image tools |
| `packages/bot-js` | (Phase 6) JS/TS bot SDK shaped like discord.js |

## 4. Request paths

### 4.1 REST
`Browser → Caddy → sr-server (axum) → service fn → repository (sqlx) → Postgres`. Mutations emit domain events on the `EventBus`. The gateway turns those into dispatches for subscribed sessions.

### 4.2 Gateway
- One WebSocket per client session at `/gateway?v=1&encoding=json` (msgpack/zstd optional later).
- Discord-shaped opcodes (`HELLO`, `IDENTIFY`, `READY`, `DISPATCH`, `HEARTBEAT`, `RESUME`, …). See doc 05.
- Fan-out: each guild has an in-memory subscriber set. Dispatches are filtered per recipient by channel-view permission before sending.
- Presence and typing live purely in memory. They are rebuilt on reconnect.

### 4.3 Voice
1. Client sends gateway op `VOICE_STATE_UPDATE` (join channel).
2. `sr-server` checks permissions and sender allowance, picks an `sr-voice` node, mints a short-lived voice token, and dispatches `VOICE_SERVER_UPDATE`.
3. Client opens WebRTC to `sr-voice` with the token. SFU validates it with a shared HMAC key.
4. MLS group commits for E2EE are relayed through the gateway (the SFU forwards opaque frames). See the voice spec.

### 4.4 Media playback
- Catalog via REST. Playback: `GET /media/v/{rendition_id}/master.m3u8` returns a manifest whose segment URLs are signed and time-limited.
- Segments are streamed from Garage through `sr-server` (or served directly from Garage's web endpoint with signed URLs). Caddy caches hot segments.
- Live TV: `GET /tv/{channel}/live.m3u8` is **generated** each request from the schedule. It stitches the current program's existing VOD segments starting at the wall-clock offset, using `#EXT-X-DISCONTINUITY` between programs. No runtime transcoding, and nothing runs when nobody requests the playlist (D-100, D-101).

### 4.5 Uploads
- Chat attachments: client requests an upload slot (`POST /channels/{id}/attachments`). Server checks the per-user limit (30 MB / 500 MB) and returns a presigned S3 PUT (multipart for > 64 MB). Client uploads, then posts the message referencing the upload IDs.
- The presigned PUT goes from the browser straight to the bucket, so **the bucket needs a CORS rule for the app origin** (D-175): `PUT, GET, HEAD` from `server.public_url`, `Content-Type` allowed, `ETag` exposed. `sr-cli storage ensure-cors --origin <public_url>` applies it (idempotent; the rule is replaced, not appended) and the Garage bootstrap script and the install walkthrough call it. The presigner signs method and path only, so the browser's `Content-Type` is advisory and the server sniffs the real type when the upload is finalised. The call is a hand-signed SigV4 `PutBucketCors` in `sr-storage::s3admin`, because `object_store` only presigns object requests.
- Library submissions: resumable multipart uploads into a `pending/` prefix, any size. Processing jobs run, and the item lands in the review queue (see the submissions spec).
- Bucket CORS: browsers PUT straight to the bucket, so it must carry a CORS rule for the app origin (`server.public_url`): methods `PUT, GET, HEAD`, allowed header `Content-Type`, exposed header `ETag`. `sr-cli storage ensure-cors --origin <server.public_url>` applies it idempotently and is called from bootstrap and install (D-175). The presigner signs method and path only, so the client's `Content-Type` is advisory and the server sniffs the real type on finalize.

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

A Postgres-backed job queue (`jobs` table, `SELECT … FOR UPDATE SKIP LOCKED`) is implemented in `sr-db::jobs`. Job kinds include: thumbnail generation, manga page conversion, transcode dispatch (external workers poll `/internal/jobs/claim`), search indexing, message purge (30-day), large-attachment expiry (7-day), donation entitlement expiry and grace, invoice confirmation polling, invite-graph suspicion evaluation, and scheduled-event reminders.

## 7. Identifiers

Snowflake IDs (64-bit, Discord layout: 42-bit ms timestamp since the **SIDE ROOM epoch 2026-01-01T00:00:00Z**, 5-bit worker, 5-bit process, 12-bit increment). They are serialized as strings in JSON, as Discord does.

## 8. Security model (summary)

- TLS everywhere. HSTS. Strict CSP (`script-src 'self' 'wasm-unsafe-eval'`). Activities live on a separate origin (`activities.<domain>`).
- Sessions: opaque random tokens (256-bit) stored hashed. Cookie `HttpOnly; Secure; SameSite=Lax` for the web app. Bearer tokens for bots (`Bot <token>`).
- Every permission check happens server-side through `sr-core::permissions::compute(member, channel)`.
- Rate limits: `governor` crate, keyed per route bucket and per user/IP, returning Discord-style headers (`X-RateLimit-*`).
- Staff access to private content is logged to an append-only, hash-chained `staff_access_log`.
- Object URLs are signed (HMAC, short TTL). Buckets are never public.

## 9. Observability

`tracing` + `tracing-subscriber` (JSON logs), Prometheus metrics via the `metrics` crate (`/internal/metrics`, bound to localhost), and an optional OpenTelemetry exporter. Health endpoints: `/healthz` (liveness) and `/readyz` (DB + storage reachable).
