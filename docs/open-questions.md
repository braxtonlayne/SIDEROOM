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
30. **WASM not built.** No `wasm` feature or wasm-bindgen yet; the `wasm32-unknown-unknown` target isn't installed here. The crate is pure `std` + serde. (The shared corpus now has 1,100+ cases; see 41.) — [lib.rs](../crates/sr-markdown/src/lib.rs)
31. **(Resolved in #40-45)** Shortcodes and jumbo-emoji detection (LNG-MD-003) are implemented. — [crates/sr-markdown](../crates/sr-markdown/src)
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
## UI kit: misc primitives and SR chrome (`feat/DS-prims-misc`)

40. **DS-PRIM-009 left unticked:** Avatar has no "animated on hover" behaviour (needs a static-frame/animated source pair from the media pipeline). The decoration slot and AvatarStack are done. — [Avatar.tsx](../packages/ui/src/primitives/Avatar/Avatar.tsx)
41. **DS-SR-008 left unticked:** the door mark is an SVG component plus `packages/ui/assets/door-mark.svg`, but the favicon `.ico` and PWA PNG icon sizes (192/512, maskable, apple-touch) are not generated. Who produces the final artwork? — [packages/ui/assets](../packages/ui/assets)
42. **DS-PRIM-016 left unticked:** Ladle is configured and builds with stories for this batch only; the other primitives (Button, inputs, Modal, Tabs, menus, Select, etc.) need stories and axe coverage, and CI doesn't run `ladle build`. axe runs in jsdom with color-contrast disabled (no layout). — [packages/ui/.ladle](../packages/ui/.ladle)
43. **DatePicker/TimePicker wrap native inputs** (`type=date/time/datetime-local`), so the calendar popup styling and first-day-of-week follow the browser. Build a custom calendar grid for a consistent look? — [DateTime.tsx](../packages/ui/src/primitives/DateTime/DateTime.tsx)
44. **DropdownChip uses a native `<select>`**, so its popup can't be themed. Swap to the Select/Popover primitives once they land? — [FilterChips.tsx](../packages/ui/src/sideroom/FilterChips.tsx)
45. **Icon set is a first pass:** about 60 round-stroke icons on the 24 grid, scaled for 20/16 by stroke weight only (no hand-tuned 16px variants). Needs a design review against the mockups; the `settings` glyph is a sliders icon, not a gear. — [paths.ts](../packages/ui/src/icons/paths.ts)
46. **`@testing-library/dom` and `axe-core`, `@ladle/react` added to `packages/ui` devDependencies**; `pnpm-lock.yaml` was not regenerated (pnpm not installed). Run `pnpm install` before merging. — [package.json](../packages/ui/package.json)
47. **Drawer duplicates Modal's focus-trap code** and does not join Modal's stack, so a Modal opened from a Drawer (or vice versa) can have both handle Escape. Extract a shared overlay hook? — [Drawer.tsx](../packages/ui/src/primitives/Drawer/Drawer.tsx)
## Entrance UI (`feat/ENT-ui`)

40. **Auth endpoints are not specified in doc 05/06.** The UI assumes `POST /sr/auth/login` (`{login,password}` → `{status:"ok"}` or `{status:"mfa_required",methods}`), `POST /sr/auth/2fa` (`{method:"totp"|"recovery",code,trust_device}`), `POST /sr/auth/forgot` and `/reset`, `POST /sr/auth/passkey/options` + `/verify`, `GET /sr/usernames/check?username=` (→ `{available}`), and `GET/DELETE /sr/auth/discord/pending` (prefill/unlink after OAuth) plus `GET /sr/auth/discord/start?intent=login|register&next=`. Confirm names and shapes with the backend. — [entrance.ts](../apps/web/src/api/entrance.ts)
41. **`/oauth/discord/callback` is treated as server-side**: the backend finishes OAuth and redirects to `/register` (pending link held server-side) or `/login?error=discord_unlinked`. The spec lists the callback as a web route; should the SPA own it? — [Register.tsx](../apps/web/src/routes/entrance/Register.tsx)
42. **CSRF header `X-SR-CSRF` is not sent yet.** Doc 05 requires it on mutations, but the token source (cookie name or bootstrap endpoint) is undefined (ENT-AUTH-006). — [client.ts](../apps/web/src/api/client.ts)
43. **Door image is a CSS-drawn placeholder**, not the photograph in the mockups (no asset in the repo). Operators can override via `--ent-door-image`; supply the real asset and the Admin → Entrance upload path. Display fonts (Newsreader, Hanken Grotesk) are not bundled either (see 34). — [Door.tsx](../apps/web/src/components/Door.tsx)
44. **`@sideroom/ui` peers on React 18, `apps/web` uses React 19.** `apps/web/tsconfig.json` maps `react` types to the web copy so `tsc` passes; align the versions properly. Also, `PasswordInput` shows "Show/Hide" text rather than the mockup's eye icon, and the TextInput label row can't hold the "Forgot password?" link beside the label as in the mockup. — [tsconfig.json](../apps/web/tsconfig.json)
45. **Client password rules are a heuristic** (min 10 chars plus the UI kit's `passwordStrength` >= 2), not zxcvbn score >= 3. The server's zxcvbn verdict is authoritative; expose it via `errors.password` on 422. Is a WASM zxcvbn wanted client-side? — [validation.ts](../apps/web/src/lib/validation.ts)
46. **Passkey/WebAuthn flow is untested in a real browser** (jsdom has no WebAuthn); it relies on `PublicKeyCredential.parseRequestOptionsFromJSON` and `toJSON()`. Also, `/login/2fa` assumes a partial-session cookie and doesn't know which methods the account has, so it always offers authenticator code, recovery code and (if supported) passkey. — [PasskeyButton.tsx](../apps/web/src/components/PasskeyButton.tsx)
47. **Not built:** `/verify-email/:token` page and in-app banner (ENT-UI-020), first-run onboarding (022), operator-editable copy and markdown for `/house-notes` and `/no-invitation` (010 partial, 025), Playwright flow (026). Layouts were checked only through jsdom tests and `vite build`, not visually against the mockups at 1440/1704px or under 900px. — [02-entrance.md](inventory/02-entrance.md)
## Markdown corpus and React renderer (`feat/LNG-markdown-2`)

40. **Corpus expectations are blessed from the parser output**, not hand-derived (`SR_MD_BLESS=1`). Only a sample was reviewed against Discord; the 1,100+ cases pin current behavior, including its quirks. Needs a Discord-parity review pass. — [tests/corpus](../crates/sr-markdown/tests/corpus)
41. **LNG-MD-005 "run in WASM" is half done.** Rust runs the corpus and the React package renders it, but the WASM wrapper (other agent) must add a test asserting deep-equal AST/plain/jumbo against the same JSON files. — [tests/corpus.rs](../crates/sr-markdown/tests/corpus.rs)
43. **Shortcode table is hand-curated (~650 names incl. aliases)**, not the full Discord/emojibase set, and has no skin-tone shortcodes (`:wave::skin-tone-2:`). Generate from emojibase data at build time instead? — [emoji.rs](../crates/sr-markdown/src/emoji.rs)
44. **Jumbo rules assumed:** 1 to 27 emoji (unicode, shortcode, custom) plus whitespace, ignoring nothing else (any other node, even a mention, disables it). Unicode detection is range-based, not a full emoji-sequence table; text-presentation symbols like `☺` count as emoji. — [emoji.rs](../crates/sr-markdown/src/emoji.rs)
45. **Shortcode parsing side effects:** `<:x:1>` (invalid custom emoji, name too short) now yields `<`, `:x:` emoji, `1>`; `:name:` inside an unrecognized token also converts. Matches Discord? — [inline.rs](../crates/sr-markdown/src/inline.rs)
46. **Parser quirks surfaced by the corpus** (unchanged, need a decision): `\<https://e.com>` keeps the trailing `>` inside the autolink; `***a** b*` yields bold `*a` then ` b*`; `**a*` yields italic `*a`; `* *` is a list item. — [tests/corpus](../crates/sr-markdown/tests/corpus)
47. **Renderer gaps:** no syntax highlighting yet (only the `renderCodeBlock` hook; LNG-MD-006 left unticked), relative timestamps do not live-update, custom-emoji URL scheme and Cinema/Library cards are supplied via resolvers, and the CSS is structural only. The package was verified with npm (no pnpm), so `pnpm-lock.yaml` was not updated. — [packages/markdown-react](../packages/markdown-react)
## Chat model (`feat/LNG-chat-model`)

40. **Member roles are a join table (`member_roles`)**, not doc 04's `guild_members.roles bigint[]`, so role deletion cascades and roles are FK-checked. Fine to diverge from the doc? — [0010_guilds.sql](../crates/sr-db/migrations/0010_guilds.sql)
41. **Migrations start at 0010** to avoid colliding with parallel work on 0005-0009. sqlx rejects a DB that already applied a higher version when a lower one is added later, so merge order matters. `moderation_holds` is created minimally in 0011; the Phase 7 owner should extend it, not recreate it. — [migrations](../crates/sr-db/migrations)
42. **Message partitions** are created for 2026-01 through 12 months after the migration runs; there is no maintenance job yet to create future months (an insert into an uncovered month fails). Needs a recurring job calling `sr_ensure_message_partition(month)`. `message_edits` has no FK to the partitioned `messages`, so purge jobs must delete it explicitly. — [0012_messages.sql](../crates/sr-db/migrations/0012_messages.sql)
43. **No `PIN_MESSAGES` bit** exists in `sr-core::Permissions` (Discord split it from MANAGE_MESSAGES in 2025); pinning currently requires MANAGE_MESSAGES. Add the bit? — [message_rules.rs](../crates/sr-chat/src/message_rules.rs)
44. **Slowmode error** maps to 429 `TooManyRequests` because `sr-core::ErrorCode` lacks Discord's 20016 "Slowmode rate limit". Add it? The user-visible retry delay is carried in `ChatError::Slowmode`. — [error.rs](../crates/sr-chat/src/error.rs)
45. **Nonce dedupe (LNG-MSG-002) not implemented**: `messages.nonce` exists and `ValidatedMessage` carries it, but no unique index or lookup yet (Discord dedupes per author/channel for a short window). — [0012_messages.sql](../crates/sr-db/migrations/0012_messages.sql)
46. **Domain `Message` only carries `embed_count` and `has_poll`**, so `PgChatRepo` stores embeds as `[]` and polls as `{}`. Real embed/poll/component payload types are needed (LNG-MSG-010/017). Mentioned user ids are not filtered by guild membership. — [chat_repo.rs](../crates/sr-db/src/chat_repo.rs)
47. **Postgres code is unexecuted.** No Postgres/Docker on the dev machine; migrations 0010-0013 and `chat_repo.rs` are compiled and clippy-clean but the `#[ignore]` tests in `crates/sr-db/tests/chat_repo.rs` have never run. — [chat_repo.rs](../crates/sr-db/tests/chat_repo.rs)
48. **Markdown AST ids are now JSON strings** (previously numbers, which lose precision above 2^53 in JS). Deserialization still accepts numbers. Any consumer (the `sr-markdown-wasm` wrapper on `feat/sr-theme-wasm`, the React renderer, corpus fixtures) must treat `id` as a string; none exist on this branch. — [ast.rs](../crates/sr-markdown/src/ast.rs)
49. **Inventory BE items left unticked** (GLD-002/007, CHN-002/003, MSG-002..-009, -011): the pure rules and repository exist, but no HTTP endpoints, gateway events, forward/poll/reaction persistence, or member-list/ban/prune repos yet.
