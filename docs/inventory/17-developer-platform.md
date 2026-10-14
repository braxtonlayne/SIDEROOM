# 17 — Developer Platform & Extensions (`DEV`, `EXT`)

Phase 6. Spec: doc 11, doc 05.

## DEV-APP — Developer onboarding & apps
- [ ] **DEV-APP-001** [DB] developer_applications, applications, application_versions, application_commands, application_installs, bot_tokens, oauth2_grants, app_testers (partly done: migrations 0500-0505 add developer_requests, applications, bot_tokens, application_audit, application_installs, application_versions, application_commands, application_command_permissions, oauth2_grants/codes/tokens, interactions; `app_testers` is not built; spec docs/specs/developer-platform.md)
- [x] **DEV-APP-002** [BE] Developer application submit/review (REVIEW_APPS), `APP_DEVELOPER` flag (`POST /developers/apply`, `GET /developers/status`, staff `/admin/developers/requests*` with `REVIEW_APPS`; approval sets accounts.flags bit 1)
- [x] **DEV-APP-003** [BE] Application CRUD, bot user creation, token reset (hashed storage) (applications CRUD under `/applications`, a bot account per application, client secret and bot token shown once and stored as SHA-256, resets need the password, audit trail)
- [x] **DEV-APP-004** [BE] Test-server restriction (dev apps only in guilds ≤ 25 members where dev has MANAGE_GUILD) + error 900120 (development apps install only for their owner in guilds of at most 25 members, else `900120`)
- [x] **DEV-APP-005** [BE] Release review (versions with manifest diff, scopes/intents changes force review), suspend app (versions with manifest snapshot and diff, `needs_review` reasons, staff decision, suspend/unsuspend)
- [x] **DEV-APP-006** [BE] Privileged intents approval flow (developers request privileged intents, staff approve a subset with the version decision; the gateway enforces `requested AND approved` for released apps)
- [ ] **DEV-APP-007** [BE] Install flows (guild install with permissions, user install), OAuth2 authorize/token/revoke, scopes (partly done: server installs (bot with a managed role capped to the installer's permissions, applications.commands), OAuth2 authorize/token/revoke/introspect with PKCE and refresh rotation, scopes identify/email/guilds/bot/applications.commands(.permissions.update); user installs are not built)
- [x] **DEV-APP-008** [FE] OAuth2 authorize screen (app, scopes list, server picker, permissions checkboxes, Authorize)

## DEV-BOT — Bot runtime API
- [x] **DEV-BOT-001** [BE] Bot auth on REST + gateway, intents filtering (incl. MESSAGE_CONTENT) (`Authorization: Bot` and `Bearer` on REST (`AppsAuth` replaces NoBots/NoBearers), bot IDENTIFY and RESUME, privileged-intent gating (4014), MESSAGE_CONTENT redaction)
- [ ] **DEV-BOT-002** [BE] Sharding support (shard id/count in IDENTIFY, `/gateway/bot`) (partly done: `GET /gateway/bot`, `GET /gateway` and the IDENTIFY `shard` field are there, but only shard [0, 1] is accepted (4010 otherwise); real sharding is not built)
- [x] **DEV-BOT-003** [BE] Application commands (chat input, user, message; options; localization; guild & global; permissions v2) (global and guild commands, bulk overwrite, option schema validation, limits, localizations stored, permissions v2 (spec section 3.5))
- [x] **DEV-BOT-004** [BE] Interactions: create, respond (all callback types), defer, follow-ups, edit original, modals, autocomplete, components (`POST /interactions`, `INTERACTION_CREATE` over the gateway, callbacks 4-9, deferral, follow-ups, original edits, ephemeral messages, 3 s and 15 min rules; posted messages can only change content (spec section 2))
- [ ] **DEV-BOT-005** [BE] HTTP interactions endpoint delivery with Ed25519 signatures (`ed25519-dalek`) + endpoint verification ping
- [ ] **DEV-BOT-006** [BE] Webhook execution parity (wait, thread_id, username/avatar override) (partly done: follow-ups honour `wait`; `thread_id` and the username/avatar override of the existing execute route are unchanged)
- [ ] **DEV-BOT-007** [BE] Bots in voice via MLS (SDK support)
- [ ] **DEV-BOT-008** [FE] Slash command picker UI (app sections, option inputs, autocomplete, attachments option)
- [ ] **DEV-BOT-009** [FE] Apps menu on messages/users (context-menu commands)
- [ ] **DEV-BOT-010** [FE] Modal renderer (text inputs, selects) for interactions
- [ ] **DEV-BOT-011** [FE] "APP" badges, "Only you can see this" ephemeral messages + Dismiss
- [ ] **DEV-BOT-012** [FE] App Directory (approved released apps: search, categories, app page, Add App)

## DEV-PORT — Developer portal
- [x] **DEV-PORT-001** [FE] `/developers` landing + Apply flow + status
- [x] **DEV-PORT-002** [FE] Applications list + New Application modal
- [ ] **DEV-PORT-003** [FE] App pages: General Information, Installation, OAuth2 (redirects, URL generator), Bot (token, intents toggles, public bot), Emojis, Webhooks, Rich Presence assets, App Testers, Activities (URL mappings), Review (submit version, status, reviewer messages) _(web partial: General, OAuth2, Bot, Installation, Commands builder, Review done; Emojis, Webhooks, Rich Presence, App Testers, Activities not built because the backend has none)_
- [ ] **DEV-PORT-004** [DOC] API docs site (generated from OpenAPI + gateway docs), porting-from-Discord guide _(web partial: hand-written hub with samples, not generated from OpenAPI)_

## DEV-SDK — SDKs & compatibility
- [ ] **DEV-SDK-001** [BE] `sr-sdk` Rust (client, gateway shard manager, cache, command framework)
- [ ] **DEV-SDK-002** [BE] `@sideroom/bot` TS SDK (discord.js-like)
- [ ] **DEV-SDK-003** [TEST] `sr-compat-tests`: run discord.js, discord.py, serenity, twilight sample bots against a test server in CI (partly done: the in-repo Rust harness (`crates/sr-server/src/bot_pg_tests.rs`) and the compatibility list docs/dev/sdk-compat.md exist; running discord.js, discord.py, serenity and twilight needs those libraries and is not wired into CI)
## DEV-ACT — Activities
- [ ] **DEV-ACT-001** [BE] Activity proxy at `<app-id>.activities.<host>` serving reviewed bundle hash + URL mappings
- [ ] **DEV-ACT-002** [BE] Activity instances (per channel), participants
- [ ] **DEV-ACT-003** [FE] `@sideroom/embedded-app-sdk` (postMessage RPC: authorize, authenticate, getChannel, participants, setActivity, openExternalLink w/ confirm, voice events)
- [ ] **DEV-ACT-004** [FE] Activity launcher (voice call bar + text channel), shelf of approved activities, iframe host with sandbox + Permissions-Policy, PiP while navigating

## EXT-PKG — Declarative packages
- [ ] **EXT-PKG-001** [DB] packages, package_versions, package_enablements
- [ ] **EXT-PKG-002** [BE][WASM] `sr-theme` validator: token schema, color/contrast checks, asset checks, no URLs, restricted CSS subset via `lightningcss`
- [ ] **EXT-PKG-003** [BE] Package submission (zip upload) → REV pipeline kinds: theme, profile_layout, emoji_pack, sticker_pack, sound_pack, recipe, server_template
- [ ] **EXT-PKG-004** [BE] Recipe applier (validate against feature schemas, apply settings bundle to a guild)
- [ ] **EXT-PKG-005** [FE] Theme Store (browse, preview live, apply per section/global)
- [ ] **EXT-PKG-006** [FE] Server Extensions & Themes browser (enable, configure)
- [ ] **EXT-PKG-007** [FE] Package creator docs + local validator CLI (`sr-cli package validate`)

## EXT-PRF — Profile customization ("MySpace mode")
- [ ] **EXT-PRF-001** [BE] profile_doc schema (blocks, grid, style tokens) + validation
- [ ] **EXT-PRF-002** [FE] Profile page renderer (`/u/:username`) in a scoped container
- [ ] **EXT-PRF-003** [FE] Blocks: Header, About me, Top friends, Now watching/reading, Favorites, Gallery, Guestbook, Song (click-to-play embed), Custom box
- [ ] **EXT-PRF-004** [BE] Guestbook comments (friends only, owner moderation)
- [ ] **EXT-PRF-005** [FE] Profile editor (drag blocks on grid, style panel: palette, background image/pattern, font from approved list, cursor set, sparkle overlays, borders; approved layouts)
- [ ] **EXT-PRF-006** [FE] Mini-popout compact rendering of customized profiles
