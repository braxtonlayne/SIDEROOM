# 03 — Tech Stack and Rust Policy

## 1. The policy (D-150)

> If it can sensibly be Rust, it is Rust. If a good Rust implementation of a component exists, prefer it over a non-Rust one. Don't reinvent mature infrastructure just to make it Rust.

Applied as rules:

1. **All code we write for the backend is Rust.** No exceptions without an ADR.
2. **Infrastructure we run** (databases, storage, relays) should be Rust when a production-ready Rust option exists. Otherwise use the best mature option and record why in an ADR.
3. **Frontend** is TypeScript + React. Any logic that also exists on the server, or that is CPU- or crypto-heavy, is a Rust crate compiled to WASM (`wasm-bindgen` + `wasm-pack`).
4. **Codecs and native libraries** may be used through Rust bindings (e.g. FFmpeg via `ffmpeg-next`). The orchestration around them is Rust.

## 2. Backend stack

| Concern | Choice | Rust? | Notes |
|---|---|---|---|
| Async runtime | `tokio` | ✅ | |
| HTTP/REST | `axum` 0.8 + `tower`/`tower-http` | ✅ | Compression, CORS, tracing, timeouts as tower layers |
| WebSocket | `axum::extract::ws` (tungstenite) | ✅ | |
| OpenAPI | `utoipa` + `utoipa-axum` | ✅ | Generates `openapi.json` → TS client |
| Serialization | `serde`, `serde_json`, `rmp-serde` (msgpack, later) | ✅ | |
| Database driver | `sqlx` 0.9 (Postgres, compile-time checked queries) | ✅ | |
| Database | **PostgreSQL 17** | ❌ C | See ADR-0001. SurrealDB evaluated and rejected for the system of record |
| Cache | `moka` | ✅ | In-process. No Redis |
| Job queue | Postgres `SKIP LOCKED` in `sr-db::jobs` | ✅ | |
| Search | `tantivy` | ✅ | Embedded. Index dir on local NVMe |
| Object storage | **Garage** (S3-compatible) | ✅ | ADR-0002. Client: `object_store` crate (S3) |
| Password hashing | `argon2` (argon2id) | ✅ | |
| 2FA | `totp-rs` | ✅ | |
| Passkeys (WebAuthn RP) | own verifier in `sr-auth::webauthn` on `p256`, `ed25519-dalek`, `crypto-bigint`, `ciborium` | ✅ | D-161: `webauthn-rs` is not used (it links C `openssl`). Feature `webauthn`; see `docs/dev/passkey-spike-report.md` |
| OAuth (Discord login) | `oauth2` | ✅ | |
| Rate limiting | `governor` | ✅ | |
| Validation | `validator` / `garde` | ✅ | |
| Errors | `thiserror` (libs), `anyhow` (bins) | ✅ | |
| Observability | `tracing`, `metrics`, `metrics-exporter-prometheus` | ✅ | |
| Config | `figment` (TOML + env) | ✅ | |
| WebRTC SFU | `str0m` (sans-IO) inside our `sr-voice` | ✅ | ADR-0003. `webrtc-rs` is the fallback |
| TURN | **coturn** | ❌ C | ADR-0004. Revisit if a maintained Rust TURN server appears |
| E2EE key agreement | `openmls` (MLS, RFC 9420) | ✅ | Also compiled to WASM |
| Frame crypto | `aes-gcm` (RustCrypto) | ✅ | Also WASM |
| Video transcoding | `ffmpeg-next` bindings → FFmpeg + NVENC | ⚠️ C libs, Rust orchestration | ADR-0005. `rav1e` (Rust AV1) is optional for an AV1 rung later |
| Media probing | `ffmpeg-next` | ⚠️ | |
| Audio (soundboard/voice msgs) | `symphonia` (decode), `opus` bindings (encode) | ✅/⚠️ | |
| Images | `image`, `fast_image_resize`, `webp` (libwebp bindings, lossy) | ✅/⚠️ | Manga pages → WebP |
| Archives | `zip`, `unrar` bindings, `sevenz-rust`, `pdfium-render` (PDF raster) | ✅/⚠️ | |
| EPUB | `epub` crate (metadata, spine). Rendered client-side | ✅ | |
| Bitcoin | `bdk_wallet` (watch-only descriptor), `bdk_esplora` | ✅ | ADR-0006 |
| Lightning (later) | LDK (`lightning`, `ldk-node`) | ✅ | |
| Email (verification, recovery) | `lettre` (SMTP) | ✅ | Operator supplies SMTP |
| Reverse proxy | **Caddy** | ❌ Go | Only for TLS/HTTP3/static. Optional. `sr-server` can serve TLS itself via `rustls` |

## 3. Frontend stack

| Concern | Choice |
|---|---|
| Language | TypeScript (strict) |
| UI | React 19 |
| Build | Vite |
| Routing | TanStack Router |
| Server state | TanStack Query (REST) + gateway-driven normalized store |
| Client state | Zustand |
| Styling | CSS Modules + CSS custom properties (design tokens). No Tailwind in the Lounge kit, to keep Discord-like precision |
| Virtualized lists | TanStack Virtual (message list, member list, grids) |
| Video player | hls.js (Safari uses native HLS) |
| Reader | Custom canvas/IMG reader. `pdf.js` for PDFs. `foliate-js` for EPUB |
| PWA | `vite-plugin-pwa` (Workbox). Web Push via VAPID (Rust `web-push` crate server-side) |
| Tests | Vitest + Testing Library. Playwright e2e |
| Lint/format | ESLint + Prettier (or Biome) |
| Package manager | pnpm workspaces |

**Rust → WASM modules on the client**

| Module | Why Rust |
|---|---|
| `sr-markdown` | The same parser on server and client means identical rendering, mention extraction, and limits |
| `sr-e2ee` | MLS + frame encryption in an `RTCRtpScriptTransform` worker. Crypto must match the server-side test vectors |
| `sr-theme` | Validates and compiles theme packages exactly as the server does before upload |
| `sr-image-tools` | Client-side resize/convert before upload (avatars, manga pages), EXIF stripping, perceptual hash for duplicate hints |
| `sr-permissions` | The same permission computation as the server, for instant UI greying-out |

## 4. Allowed non-Rust components

These are the **only** non-Rust runtime components. Adding another needs an ADR.

| Component | Language | Reason | ADR |
|---|---|---|---|
| PostgreSQL | C | No Rust relational DB is mature enough to be the system of record | 0001 |
| FFmpeg libs (via bindings) | C | No Rust equivalent for full decode/scale/H.264 NVENC | 0005 |
| libwebp, pdfium, unrar (via bindings) | C/C++ | Format support | 0005 |
| coturn | C | No maintained Rust TURN server | 0004 |
| Caddy (optional) | Go | Convenience. Can be replaced by rustls in `sr-server` | — |
| Browser JS ecosystem | TS | Frontend (D-151) | — |

## 5. Toolchain

- Rust stable, pinned in `rust-toolchain.toml`. Edition 2024.
- `cargo-deny` (licenses, advisories), `cargo-nextest`, `sqlx-cli` (offline mode with `.sqlx/` committed).
- Node LTS + pnpm.
- Pre-commit: `cargo fmt`, `pnpm lint-staged`.
