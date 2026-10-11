# Open questions

Questions raised by implementation work that need an owner decision. Each links to the code it affects. Paths are relative to the repo root and resolve once the originating branch is merged. Remove an entry when it is decided (and record the decision in [01-decisions-log.md](01-decisions-log.md)).

## Auth (`feat/AUTH-primitives`)

1. **Recovery code strength.** Codes are 60 bits, stored as argon2id hashes; checking up to 10 hashes makes each attempt slow, so rate limiting matters. Longer? — [recovery.rs](../crates/sr-auth/src/recovery.rs)
2. **TOTP secret encryption key source.** Secrets must be encrypted at rest; where does the key come from (config, KMS)? Not wired in this crate. — [totp.rs](../crates/sr-auth/src/totp.rs), [repo.rs](../crates/sr-auth/src/repo.rs)
3. **Hash invite codes at rest?** Kept plain/canonical because doc 06 treats the code as a lookup key. — [invite.rs](../crates/sr-auth/src/invite.rs)
4. **Argon2 and session defaults.** Chosen by the agent, not from docs: argon2id 64 MiB / 3 passes / 1 lane; sessions 30 days idle, 180 days absolute; reset token 30 min (doc 06 §4), verify token 24 h. — [password.rs](../crates/sr-auth/src/password.rs), [session.rs](../crates/sr-auth/src/session.rs), [email_token.rs](../crates/sr-auth/src/email_token.rs)
5. **Password strength (ENT-AUTH-004, zxcvbn) and CSRF (ENT-AUTH-006)** are not implemented; should they live in `sr-auth`? — [crates/sr-auth](../crates/sr-auth)

## Core (`feat/FND-CORE`)

6. **Discord's extra permission rules** (timed-out members lose most permissions; no VIEW_CHANNEL means nothing) are not implemented. Add them, and where? — [permissions.rs](../crates/sr-core/src/permissions.rs)
7. **Browser clock.** `SystemClock` panics on `wasm32-unknown-unknown`; the wasm crate needs a `Date.now()` clock. — [time.rs](../crates/sr-core/src/time.rs)
8. **Permission fixtures** are hand-derived, not captured from real Discord. Capture real ones? — [crates/sr-core/tests](../crates/sr-core/tests)
9. **Error-code list** is a subset of Discord's; extend as endpoints need. — [error.rs](../crates/sr-core/src/error.rs)
10. **Config schema/defaults** are a guess; review against doc 12. — [config.rs](../crates/sr-core/src/config.rs)

## Database (`feat/FND-DB`)

11. **Migrations never run against Postgres.** Needs a run with `SR_TEST_DATABASE_URL` set (`cargo test -p sr-db -- --ignored`). — [migrations](../crates/sr-db/migrations)
12. **Additions beyond doc 04:** `login_events` (180-day IP history, doc 09), `accounts.age_confirmed` (doc 06 §7), job queue as migration 0004. Confirm. — [0001_identity.sql](../crates/sr-db/migrations/0001_identity.sql), [0004_jobs.sql](../crates/sr-db/migrations/0004_jobs.sql)
13. **sqlx TLS features not enabled**; connections are plaintext until added. — [pool.rs](../crates/sr-db/src/pool.rs)
14. **FND-DB-007** (monthly `messages` partition manager) not started.

## Events and features (`feat/FND-EVT-FEAT`)

16. **`Feature::routes()`, `FeatureCtx`, `Migration`** are placeholders until `AppState` exists. — [feature.rs](../crates/sr-features/src/feature.rs)
17. **Merge conflict:** root `[workspace.dependencies]` was added on both this branch and `feat/FND-CORE`. — [Cargo.toml](../Cargo.toml)

## Web (`feat/FND-REPO-004-web`)

18. **FND-API-007 is ticked but is a stub** (no HELLO/IDENTIFY/heartbeat/RESUME). Untick? — [gateway.ts](../apps/web/src/api/gateway.ts)
19. **Pre-commit hook not installed** (lint-staged configured only; FND-REPO-007). — [package.json](../package.json)
20. **pnpm not installed locally**; the agent used `npx pnpm@10.12.1` with `npm_config_manage_package_manager_versions=false`. — [pnpm-workspace.yaml](../pnpm-workspace.yaml)

## Ops (`chore/FND-OPS`)

21. **Pinned versions are guesses** (Garage v1.0.1, Mailpit v1.21) and the Garage bootstrap CLI flags are unverified. — [dev.yml](../deploy/compose/dev.yml), [bootstrap.sh](../deploy/compose/garage/bootstrap.sh)
22. **Unvalidated:** `deny.toml` and `garage.toml` were never parsed; nothing was run in docker/just/cargo. — [deny.toml](../deny.toml)
23. **Workflows depend on missing pieces** (Dockerfiles under `deploy/docker/`, `@sideroom/wasm` and `@sideroom/api-client`, `sr-cli seed`, `sr-server openapi`, `*-wasm` crates), so release and OpenAPI drift jobs fail for now. — [release.yml](../.github/workflows/release.yml), [openapi.yml](../.github/workflows/openapi.yml)

## Storage (`feat/FND-STOR`)

24. **Client-driven/resumable multipart uploads are not possible** with `object_store`: `multipart_create` is server-side only. Presigned part URLs would need the raw S3 API or another crate. Needed? — [storage.rs](../crates/sr-storage/src/storage.rs)
25. **"30 MB" read as 30 MiB** (and 500 MiB, 64 MiB multipart threshold). Confirm binary vs decimal. — [upload/](../crates/sr-storage/src/upload)
26. **EXIF stripping drops the orientation tag.** Preserve orientation (rotate pixels instead)? — [upload/](../crates/sr-storage/src/upload)
27. **Presigned URLs against real S3/Garage are untested**; only the builder and the in-memory fake are covered. — [storage.rs](../crates/sr-storage/src/storage.rs)
28. **FND-STOR-004 left unticked**: the upload-slot endpoint belongs in `sr-api`. FND-STOR-003 (image resizer) untouched. — [crates/sr-storage](../crates/sr-storage)

## Markdown (`feat/LNG-markdown`)

29. **Mention cap of 100 is an assumption**, not from the docs. Confirm the limit. — [crates/sr-markdown](../crates/sr-markdown/src)
30. **WASM not built.** No `wasm` feature or wasm-bindgen yet; the `wasm32-unknown-unknown` target isn't installed here. The crate is pure `std` + serde. (LNG-MD-005 also needs the 500-case shared corpus; ~150 exist.) — [lib.rs](../crates/sr-markdown/src/lib.rs)
31. **Shortcodes and jumbo-emoji detection (LNG-MD-003) not implemented.** — [crates/sr-markdown](../crates/sr-markdown/src)
32. **Parsing quirks to confirm against Discord:** a fenced block is split out wherever it appears (mid-line or inside a quote); `>>>` quotes up to the next code fence; `* text` at line start is a list item, not italic. — [crates/sr-markdown](../crates/sr-markdown/src)

## UI kit (`feat/DS-tokens-prims`)

33. **Token types are hand-written**, not generated from the `sr-theme` schema (DS-TOK-001), which doesn't exist yet. — [packages/ui/src/theme](../packages/ui/src/theme)
34. **Font files missing.** Font config is done (DS-TOK-005) but WOFF2 binaries and licence files must be added by the app. — [fonts.ts](../packages/ui/src/theme/fonts.ts)
35. **`sideroom` token values were not adjusted after viewing the mockups**; they follow docs/10 only. Compare with [docs/assets/mockups](assets/mockups/README.md). — [packages/ui/src/theme](../packages/ui/src/theme)
36. **Tooltip/Popover don't flip at viewport edges** (plain CSS placement). Add a positioning library or custom logic? — [packages/ui/src](../packages/ui/src)
37. **Saturation setting uses a CSS `filter` on the provider root**, which makes `position: fixed` overlays position relative to that root while active. — [packages/ui/src/theme](../packages/ui/src/theme)
38. **Partial primitives left unticked:** Slider, Segmented, ContextMenu, Dropdown, Drawer, Accordion, AvatarStack, Pill/Tag, ProgressBar, EmptyState, error page; DS-PRIM-004, 012–016 (Select, ScrollArea, icons, Ladle stories) not started. — [01-foundation.md](inventory/01-foundation.md)
39. **Verified with npm/npx and no lockfile** (pnpm not installed); the package's lockfile and workspace linking are untested. — [packages/ui/package.json](../packages/ui/package.json)

## CLI and auth repositories (`feat/FND-CLI`)

40. **No Postgres available here, so no SQL was executed.** The `sr-db` auth repositories, account helpers and partition manager are covered by `#[ignore]` tests (`crates/sr-db/tests/auth_repo.rs`) that need `SR_TEST_DATABASE_URL`; they have never run. Run them before merging. — [auth_repo.rs](../crates/sr-db/src/auth_repo.rs)
41. **Migration 0005 recreates `recovery_codes`.** The `sr-auth` trait stores argon2id PHC strings, but 0001 had `code_hash BYTEA CHECK (octet_length = 32)`. Confirm that argon2 per-code hashing (not SHA-256 with an index lookup) is the intended design. — [0005](../crates/sr-db/migrations/0005_recovery_code_phc.sql)
42. **TOTP secret encryption key.** `PgAuthRepo` takes a 32-byte AES-256-GCM key (AAD = account id). Where does it come from (config `secrets.*`)? There is no key rotation story yet. — [auth_repo.rs](../crates/sr-db/src/auth_repo.rs)
43. **FND-DB-007 is generic.** `messages` is not in the migrations, so `sr-db::partitions` takes a table name and assumes `PARTITION BY RANGE (id)` with Snowflake bounds per UTC month. Nothing schedules the `db.partitions.ensure` job yet. — [partitions.rs](../crates/sr-db/src/partitions.rs)
44. **Staff Server not created by `bootstrap`** (no guilds/roles tables), so FND-CLI-001 is left unticked; `staff transfer-owner` (FND-CLI-002) and FND-CLI-003 are not started.
45. **One-time Owner login link reuses the `reset_password` email-token purpose** (24h TTL) and points at `/sr/login/link?token=...`, a route that does not exist yet. Add a dedicated `login_link` purpose in `sr-auth` and the enum? — [commands.rs](../crates/sr-cli/src/commands.rs)
46. **System user (id 1) is stored as a `deactivated` account** with an unusable password and `system@sideroom.invalid`, so it cannot log in and is outside the 20,000 cap. Acceptable, or add a dedicated flag/state?
47. **`seed` only creates accounts and invites** (alice, bob, carol, dave with a shared dev password and 5 credits each); servers, messages and CC0 library samples wait on those schemas, so FND-DB-009 is left unticked.
48. **`invite create` bypasses credit, verification and account-age rules** (operator emergency, doc 06 section 5.1) and does not write to the credit ledger or an audit log. Should emergency invites consume credits or be logged?
49. **`user reset-2fa` also removes passkeys and revokes all sessions**; the docs only say "reset 2FA". No audit-log entry is written (no table yet).
## Gateway (`feat/FND-GATEWAY`)

40. **Dispatch payloads are skeletal.** `DomainEvent`s carry only IDs, so `MESSAGE_CREATE`, `GUILD_MEMBER_ADD` etc. contain ids, not Discord-shaped objects. Should events carry full payloads, or should the gateway hydrate via a trait? — [hub.rs](../crates/sr-gateway/src/hub.rs)
41. **zstd is per-message frames, not one continuous stream.** Uses pure-Rust `ruzstd` (no C toolchain) which has no streaming/flush encoder, so each message is an independent zstd frame in a binary frame. Real `zstd-stream` clients (shared decoder context) should accept this, but it's untested against discord.js/serenity. Switch to the `zstd` crate (needs a C compiler; ADR per doc 03 section 4)? — [compress.rs](../crates/sr-gateway/src/compress.rs)
42. **DMs and group DMs are not fanned out.** Events with no guild (Channel/User topics) are ignored; per-user topic subscription and DM channel membership lookup need a decision. — [hub.rs](../crates/sr-gateway/src/hub.rs)
43. **Ops 4, 8, 31, 100-102 are accepted and ignored** (owned by voice/members/watch-party/E2EE work). Is a pluggable op-handler trait wanted now? Also `READY` omits relationships/read_state/user_settings (the `GuildDirectory::ready_extras` hook exists). — [conn.rs](../crates/sr-gateway/src/conn.rs)
44. **Intent handling assumptions:** non-bot sessions receive all intents; bots may use privileged intents only per `AuthedUser::allowed_privileged_intents` (D-062 approval flow); `MESSAGE_CONTENT` is not stripped since events carry no content yet. No sharding or per-connection rate limit (Discord 120 events/60 s) yet. — [protocol.rs](../crates/sr-gateway/src/protocol.rs)
45. **Presence semantics:** presence is dropped only when a user's last session expires (2 min resume window), not at socket close. `GUILD_CREATE` does not include initial `presences`; clients only see changes. Typing is entered via `Gateway::typing_start` (REST route must call it). — [hub.rs](../crates/sr-gateway/src/hub.rs)
46. **Slow consumers** are detached (queue full) rather than closed with a code, and resume from the buffer. Session ids are 128-bit random, tied to the user's token on resume. Fan-out is single-node (in-memory subscriber sets), as per doc 02 section 5. — [session.rs](../crates/sr-gateway/src/session.rs)
## HTTP API (`feat/FND-API`)

40. **CSRF design (answers part of 5).** `X-SR-CSRF` is a SHA-256 derived from the raw session token (no storage), fetched by the web app from `GET /api/v1/sr/csrf`. Acceptable, or prefer a separate random per-session secret? No Origin/Referer check yet. — [csrf.rs](../crates/sr-api/src/auth/csrf.rs)
41. **Auth backends are in-memory stubs.** Sessions plug in via `sr_auth::repo::SessionRepository` (blanket impl); bot tokens and OAuth bearer tokens have no `sr-auth` traits, so `BotBackend`/`BearerBackend` live in `sr-api`. Move them into `sr-auth`? Bot-token format/hashing is undecided. — [backend.rs](../crates/sr-api/src/auth/backend.rs)
42. **Rate limit numbers are guesses** (global 50/s per caller like Discord, default bucket 40/10 s, a few route overrides). `Remaining`/`Reset` headers come from a fixed-window side tracker because governor does not expose remaining burst, so headers and enforcement can differ slightly. Callers are keyed by hashed `Authorization`/cookie or peer IP; behind a reverse proxy (Caddy) the peer IP is the proxy, so `X-Forwarded-For` trust must be configured. — [rate_limit.rs](../crates/sr-api/src/rate_limit.rs)
43. **`/internal/metrics` listener.** Served by a separate router on a hard-coded `127.0.0.1:9100` in `sr-server`; make it configurable in `sr-core` config. `/readyz` has no DB/storage checks wired yet (`ReadinessCheck` trait only; `sr-db::health_check` and `sr-storage` need adapters). DB pool, gateway and job metrics have helper functions but no callers. — [main.rs](../crates/sr-server/src/main.rs), [health.rs](../crates/sr-api/src/health.rs)
44. **OpenAPI.** Only placeholder routes (`/users/@me`, `/sr/csrf`) are documented; utoipa 5 pinned (6.0 exists). The `sr-server openapi` subcommand and `/developers/docs/api` viewer expected by CI are not implemented. No OpenTelemetry exporter (doc 02 section 9 lists it as optional). — [openapi.rs](../crates/sr-api/src/openapi.rs)
## UI overlays (`feat/DS-prims-overlays`)

40. **Q36 resolved:** Tooltip/Popover now use a custom positioning helper (flip + shift, no `@floating-ui` dependency) shared by menus and select popups. Is a custom helper acceptable long-term, or should we adopt `@floating-ui/react` (arrow, virtual-element hit-testing, `autoUpdate` on ancestor scroll)? — [useFloating.ts](../packages/ui/src/primitives/Overlay/useFloating.ts), [position.ts](../packages/ui/src/util/position.ts)
41. **Menus have no touch long-press** for ContextMenu (right-click, Menu key and Shift+F10 only), no submenu hover-intent delay or safe-triangle, and no arrow on Tooltip/Popover. Needed for Phase 0? — [Menu.tsx](../packages/ui/src/primitives/Menu/Menu.tsx)
42. **Menus portal into the theme root with `position: fixed`**, which Q37's saturation `filter` breaks while active (offsets become relative to the filtered root). Same root cause as Q37. — [useFloating.ts](../packages/ui/src/primitives/Overlay/useFloating.ts)
43. **Select/Combobox choices to confirm:** checkbox/radio menu items close the menu unless `keepOpen`; Combobox is single-value with no free-text "create" option; MultiSelect shows chips (no "+N more" overflow); option rows are fixed-height (virtualization) so `description` needs `rowHeight` ~52. No axe run on these yet (DS-PRIM-016). — [Select.tsx](../packages/ui/src/primitives/Select/Select.tsx)
44. **`@tanstack/react-virtual` added** as a `packages/ui` dependency (no lockfile, installed with npm). — [package.json](../packages/ui/package.json)
## Theme and WASM (`feat/sr-theme-wasm`)

40. **DS-TOK-001 left unticked.** `sr-theme` emits a committed JSON Schema ([theme-package.schema.json](../crates/sr-theme/schema/theme-package.schema.json)) and a parity test keeps its token registry equal to `packages/ui/src/tokens`, but the UI kit's TS types are still hand-written (another agent owns `packages/ui`). Which generator turns the schema into TS (`json-schema-to-typescript`, or emit `UI_TOKEN_NAMES` from the registry)? The registry is not in the JSON Schema (token names are map keys), so token-name unions need a separate export. — [crates/sr-theme](../crates/sr-theme)
41. **EXT-PKG-002 left unticked.** Done: known tokens, value grammar (no URLs, `var()`, `;{}`), WCAG AA warnings on 7 fg/bg pairs, asset and font checks (2 MB / 10 MB caps, PNG/WebP/AVIF, WOFF2 + licence). Not done: the restricted CSS subset via `lightningcss` (Phase 6+), pixel/format sniffing of real asset bytes (the validator only sees declared metadata), and font licence file contents. — [validate.rs](../crates/sr-theme/src/validate.rs)
42. **Assumptions to confirm:** 1 MB cap per font (docs only give 2 MB images, 10 MB total); contrast only checks opaque hex pairs; `extends` accepts only built-in ids; built-in ids are reserved; user themes compile to `[data-theme="<id>"]`, so `ThemeProvider` must accept non-built-in ids. No token takes an image yet, so package assets are validated but never referenced by compiled CSS (the signed-CDN rewrite from docs/10 is unimplemented). — [compile.rs](../crates/sr-theme/src/compile.rs)
43. **Markdown AST ids lose precision in JS.** `sr-markdown` serializes mention/emoji ids as JSON numbers (`u64`), so `parseMarkdown` returns e.g. `123456789012345680` for `<@123456789012345678>`. `extractMentions` in the wasm wrapper returns decimal strings, but the AST itself does not. Serialize ids as strings in the AST (breaking change for the React renderer)? — [ast.rs](../crates/sr-markdown/src/ast.rs)
44. **WASM pipeline unverified in CI and by browser.** Built locally with wasm-pack on Windows (`--target web`, wasm-opt enabled) and smoke-tested under Node only; `wasm-pack test --headless --chrome` finds no wasm-bindgen tests (unit tests are native). `packages/wasm` has no TS sources or dependencies so the lockfile needed only an empty `packages/wasm` importer entry added by hand (pnpm not installed); regenerate with `pnpm install` to confirm. `--target web` needs the app to pass the `.wasm` URL (Vite `?url`) or accept the default `import.meta.url` resolution. Bundler target instead? — [packages/wasm](../packages/wasm)
45. **`*-wasm` crates are workspace members that build natively too** (`crate-type = ["cdylib","rlib"]`) so fmt/clippy/test cover them; wasm-only code paths (`JsError`) are not exercised by `cargo test`. `sr-e2ee`, image tools and `sr-voice-proto` wasm wrappers do not exist yet, so FND-REPO-005 is ticked for the three existing targets only. — [crates](../crates)
