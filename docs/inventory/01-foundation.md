# 01 — Foundation & Design System (`FND`, `DS`)

Phase 0. Spec refs: docs 02, 03, 04, 10, 12.

## FND-REPO — Repository & tooling
- [ ] **FND-REPO-001** [OPS] Cargo workspace root (`Cargo.toml`, `rust-toolchain.toml` pinned stable, edition 2024, workspace lints: `unsafe_code = "forbid"` except FFI crates)
- [ ] **FND-REPO-002** [OPS] Crate skeletons per doc 02 §3 (`sr-core`, `sr-db`, `sr-auth`, `sr-features`, `sr-events`, `sr-gateway`, `sr-api`, `sr-chat`, `sr-markdown`, `sr-search`, `sr-storage`, `sr-media`, `sr-reader`, `sr-tv`, `sr-review`, `sr-payments`, `sr-voice-proto`, `sr-e2ee`, `sr-theme`, `sr-server`, `sr-voice`, `sr-transcoder`, `sr-cli`, `sr-sdk`)
- [ ] **FND-REPO-003** [OPS] pnpm workspace (`apps/web`, `packages/ui`, `packages/api-client`, `packages/wasm`)
- [ ] **FND-REPO-004** [OPS] Vite + React 19 + TS strict app scaffold with TanStack Router, Query, Zustand
- [ ] **FND-REPO-005** [OPS] wasm-pack build pipeline producing `packages/wasm` (ESM, typed bindings) for each `*-wasm` target
- [ ] **FND-REPO-006** [OPS] `cargo-deny` config (licences allowlist, advisories, bans)
- [ ] **FND-REPO-007** [OPS] ESLint/Prettier (or Biome) config, lint-staged + pre-commit hook
- [ ] **FND-REPO-008** [OPS] `.editorconfig`, `CODEOWNERS` (owner), Dependabot/Renovate config
- [ ] **FND-REPO-009** [OPS] Dev stack: `deploy/compose/dev.yml` (Postgres 17, Garage single-node + bucket bootstrap, Caddy, Mailpit for SMTP, coturn)
- [ ] **FND-REPO-010** [OPS] `justfile`/`xtask`: `dev`, `test`, `lint`, `migrate`, `seed`, `gen-api`, `wasm`
- [ ] **FND-REPO-011** [DOC] `docs/dev/getting-started.md` (local setup in < 15 min)

## FND-CI — Continuous integration
- [ ] **FND-CI-001** [OPS] GitHub Actions: Rust fmt + clippy (`-D warnings`) + nextest with Postgres service
- [ ] **FND-CI-002** [OPS] `sqlx prepare --check` (offline query metadata committed)
- [ ] **FND-CI-003** [OPS] Web lint, typecheck, Vitest
- [ ] **FND-CI-004** [OPS] Playwright e2e job against the compose stack (Chromium, Firefox, WebKit)
- [ ] **FND-CI-005** [OPS] WASM build + wasm tests (`wasm-bindgen-test` headless)
- [ ] **FND-CI-006** [OPS] OpenAPI drift check (generated client must match committed)
- [ ] **FND-CI-007** [OPS] Release workflow: static musl binaries (x86_64, aarch64), container images (server, voice, transcoder-cuda), checksums, changelog
- [ ] **FND-CI-008** [OPS] Cache strategy (sccache / rust-cache, pnpm store)

## FND-CORE — `sr-core`
- [x] **FND-CORE-001** [BE] Snowflake generator (SIDE ROOM epoch 2026-01-01, worker/process bits, monotonic), string serde
- [x] **FND-CORE-002** [BE] Error taxonomy (`thiserror`) + mapping to Discord-style JSON error bodies and codes
- [x] **FND-CORE-003** [BE][WASM] Permission bitflags (Discord bits 0–50, SR bits 56–59) + `compute_base`, `compute_overwrites`, `has` helpers
- [x] **FND-CORE-004** [BE] Platform permission bitflags (doc 07 §3)
- [x] **FND-CORE-005** [BE] Domain IDs newtypes (`UserId`, `GuildId`, `ChannelId`, …) with serde
- [x] **FND-CORE-006** [BE] Clock abstraction (injectable for tests: purge/expiry/TV time-travel)
- [x] **FND-CORE-007** [BE] Config types + figment loader (TOML + `SR_*` env)
- [x] **FND-CORE-008** [TEST] Permission computation property tests vs a Discord reference fixture set

## FND-DB — `sr-db`
- [ ] **FND-DB-001** [DB] sqlx pool, migration runner, health check
- [ ] **FND-DB-002** [DB] Migration 0001: persons, accounts, external_identities, sessions, mfa, webauthn, recovery codes, email tokens
- [ ] **FND-DB-003** [DB] Migration 0002: platform_invites, invite_credits_ledger, invite graph view
- [ ] **FND-DB-004** [DB] Migration 0003: platform settings (key/value with schema version), feature enablements
- [ ] **FND-DB-005** [DB] Job queue table + `claim/complete/fail/retry` with `SKIP LOCKED`, backoff, dead-letter
- [ ] **FND-DB-006** [BE] Job runner (tokio tasks, concurrency per kind, graceful shutdown)
- [ ] **FND-DB-007** [DB] Monthly partition manager for `messages` (create-ahead job)
- [ ] **FND-DB-008** [BE] Repository pattern + transaction helper (`with_tx`) + advisory lock helper
- [ ] **FND-DB-009** [OPS] Seed command (dev data: users, servers, messages, library samples with CC0 media)

## FND-EVT — `sr-events`
- [x] **FND-EVT-001** [BE] `EventBus` trait + `InProcessBus` (tokio broadcast per topic)
- [x] **FND-EVT-002** [BE] `DomainEvent` enum + topic routing (guild, channel, user, platform)
- [x] **FND-EVT-003** [TEST] Bus contract tests (reusable for future implementations)

## FND-FEAT — `sr-features`
- [x] **FND-FEAT-001** [BE] `Feature` trait, registry, startup registration macro
- [x] **FND-FEAT-002** [BE] `SettingsSchema` types (bool, int, enum, string, duration, color, role/channel/emoji pickers, list, group) + JSON serialization
- [x] **FND-FEAT-003** [BE] Scoped settings storage (platform/guild/channel/user) + defaults resolution + validation
- [x] **FND-FEAT-004** [BE] `is_enabled(id, scope)` with moka cache + invalidation events
- [ ] **FND-FEAT-005** [BE] `/sr/features` API (list registry, get/set settings per scope, permission-gated)
- [ ] **FND-FEAT-006** [FE] `<SchemaForm>` auto-renderer for every schema field type, with unsaved-changes bar
- [ ] **FND-FEAT-007** [FE] Feature custom-UI slot registry (feature id → lazy React component)

## FND-API — HTTP & gateway plumbing
- [ ] **FND-API-001** [BE] axum app state, router composition, tower layers (trace, compression, timeout, CORS, request id)
- [ ] **FND-API-002** [BE] Auth extractors (session cookie + CSRF, Bot token, Bearer OAuth)
- [ ] **FND-API-003** [BE] Rate limiter (governor) with Discord-style buckets and headers, global + per-route
- [ ] **FND-API-004** [BE] utoipa OpenAPI generation, `/api/v1/openapi.json`, `/api/v10` alias
- [ ] **FND-API-005** [FE] Generated TS client (openapi-typescript + fetch wrapper with retry on 429)
- [ ] **FND-API-006** [BE][GW] Gateway skeleton: HELLO, IDENTIFY, heartbeat, READY, RESUME, INVALID_SESSION, RECONNECT, zstd compression option
- [ ] **FND-API-007** [FE] Gateway client (reconnect with backoff, resume, sequence tracking, dispatch → store)
- [ ] **FND-API-008** [FE] Normalized client store (users, guilds, channels, members, messages caches) fed by gateway + REST
- [ ] **FND-API-009** [BE] `/healthz`, `/readyz`, `/internal/metrics`

## FND-STOR — Storage
- [ ] **FND-STOR-001** [BE] `sr-storage` over `object_store` S3: put/get/delete/list, presigned PUT/GET, multipart create/complete/abort
- [ ] **FND-STOR-002** [BE] Signed CDN URL scheme (`/cdn/...?ex=&sig=`) + verification middleware + Range support
- [ ] **FND-STOR-003** [BE] Image proxy/resizer for avatars/icons/emoji (`?size=` 16–4096, WebP/AVIF/PNG/GIF, animated support)
- [ ] **FND-STOR-004** [BE] Upload slot service (size limit from entitlements, content-type sniffing, EXIF strip on images)

## FND-OBS — Observability
- [ ] **FND-OBS-001** [BE] tracing JSON logs, span per request/gateway session/job
- [ ] **FND-OBS-002** [BE] Prometheus metrics registry + standard metrics (HTTP, DB pool, gateway sessions, jobs)
- [ ] **FND-OBS-003** [OPS] Optional Prometheus + Grafana compose profile + dashboards

## FND-CLI — `sr-cli`
- [ ] **FND-CLI-001** [BE] `migrate`, `bootstrap` (Owner, Staff Server, system user, one-time login link)
- [ ] **FND-CLI-002** [BE] `invite create` (operator emergency), `user reset-2fa`, `staff transfer-owner`
- [ ] **FND-CLI-003** [BE] `backup`, `restore`, `reindex`, `verify-access-log`, `upgrade`

## DS-TOK — Design tokens & theming runtime
- [ ] **DS-TOK-001** [FE] Token schema (TS types generated from `sr-theme` JSON schema)
- [ ] **DS-TOK-002** [FE] `lounge-dark`, `lounge-light`, `lounge-ash`, `lounge-onyx` token sets
- [ ] **DS-TOK-003** [FE] `sideroom` and `sideroom-broadcast` token sets (calibrated to mockups)
- [ ] **DS-TOK-004** [FE] Theme provider: per-section theme resolution (platform default → user global → per-section override → server accent)
- [ ] **DS-TOK-005** [FE] Font loading (Newsreader, Hanken Grotesk, IBM Plex Mono, a Lounge sans), `font-display: swap`, self-hosted WOFF2
- [ ] **DS-TOK-006** [FE] Reduced-motion, contrast, saturation, and font-scale runtime variables

## DS-PRIM — Primitives (`packages/ui`)
- [ ] **DS-PRIM-001** [FE] Button (primary/secondary/danger/link/ghost; sizes; loading; icon-only)
- [ ] **DS-PRIM-002** [FE] TextInput, TextArea (auto-grow), PasswordInput (eye toggle, strength meter)
- [ ] **DS-PRIM-003** [FE] Checkbox, Radio group, Switch, Slider (with marks), Segmented control
- [ ] **DS-PRIM-004** [FE] Select / Combobox / MultiSelect (searchable, virtualized)
- [ ] **DS-PRIM-005** [FE] Tooltip, Popover, ContextMenu (nested submenus, checkbox/radio items, keyboard nav), Dropdown menu
- [ ] **DS-PRIM-006** [FE] Modal (sizes, stacked modals, focus trap), ConfirmDialog (typed confirmation variant), Drawer
- [ ] **DS-PRIM-007** [FE] Toast/notice system, inline Notice banners (info/warn/danger/success)
- [ ] **DS-PRIM-008** [FE] Tabs (underline + pill variants), Accordion, Collapsible category
- [ ] **DS-PRIM-009** [FE] Avatar (status dot, speaking ring, decoration slot, animated on hover), AvatarStack
- [ ] **DS-PRIM-010** [FE] Badge, Pill, Tag/chip (server tag chip, genre chip), Counter badge (mentions)
- [ ] **DS-PRIM-011** [FE] Skeleton loaders, Spinner, ProgressBar (linear/circular), Empty state, Error boundary page
- [ ] **DS-PRIM-012** [FE] ScrollArea (thin/auto scrollbars) + VirtualList + VirtualGrid wrappers
- [ ] **DS-PRIM-013** [FE] Kbd, ColorPicker (swatches + hex + eyedropper), DatePicker/TimePicker, FileDrop zone
- [ ] **DS-PRIM-014** [FE] Icon set (original SVGs, 24/20/16 grid) + Icon component
- [ ] **DS-PRIM-015** [FE] Full-screen settings layout (sidebar sections, ESC close, unsaved-changes bar)
- [ ] **DS-PRIM-016** [FE] Ladle stories for every primitive + axe a11y tests

## DS-SR — Side Room editorial kit
- [ ] **DS-SR-001** [FE] Eyebrow (hairline + letter-spaced caps), DisplayHeading (serif, italic accent span)
- [ ] **DS-SR-002** [FE] SR header (door logo, wordmark, tagline, right action), SR footer ("Not everywhere. *Just here.*")
- [ ] **DS-SR-003** [FE] Section top bar (`SIDE ROOM / The lounge · The cinema · The library`, active underline, voice indicator slot)
- [ ] **DS-SR-004** [FE] ArrowLink/ArrowButton (↗), GoldButton, OutlineButton
- [ ] **DS-SR-005** [FE] PosterCard (2:3, hover actions, progress, corner tag), PosterRow (horizontal scroller with arrows, "Explore all ↗")
- [ ] **DS-SR-006** [FE] Spotlight/Hero (backdrop, gradient, carousel index, auto-advance)
- [ ] **DS-SR-007** [FE] FilterChips row (single/multi), Dropdown chip ("Room selection ▾")
- [ ] **DS-SR-008** [FE] Door logo mark (SVG) + favicon/PWA icon set
