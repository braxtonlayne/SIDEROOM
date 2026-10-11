# Open question answers

Answers to every entry in [open-questions.md](open-questions.md) (171 questions, same numbering, state as of `integration/phase0` at `a21ac26`). Rules applied: [01-decisions-log.md](01-decisions-log.md) wins over everything; Discord parity by default (AGENTS.md §3.3); Rust first (D-150); no telemetry; product-level limits live in the database, infrastructure in config (doc 12 §3).

Status key:

- `DEFAULT`: implement as decided; the owner need not weigh in.
- `OWNER`: a product/policy/cost/security-posture choice; a recommendation is given.
- `NO-CODE`: needs a human action (run Postgres/Docker, add assets, visual review) or is informational.
- `DONE`: already resolved by later work on the integration branch (verified in code).

## Summary

| Status | Count |
|---|---|
| DEFAULT | 125 |
| OWNER | 4 |
| NO-CODE | 23 |
| DONE | 19 |

DONE: 2, 14, 16, 17, 22, 27, 29, 30, 37, 58, 63, 67, 76, 100, 101, 146, 162, 165, 166.
NO-CODE: 8, 11, 19, 20, 21, 26, 33, 34, 38, 69, 81, 85, 99, 102, 108, 121, 122, 124, 126, 127, 130, 139, 168.

### OWNER questions (recommendation in one line each)

- **Q46** Emergency `invite create` from the CLI: recommend it consumes no credit and writes a `platform_audit_log` row (actor = system user 1); it is an operator action outside D-024's per-user economy.
- **Q133** Guild audit-log retention: recommend Discord's 45 days, operator-configurable in Admin → Limits (`0` = forever); the platform audit log and staff access log stay forever (doc 09).
- **Q136** AutoMod preset word lists: recommend shipping no built-in lists; the operator curates them in Admin → Features → AutoMod (optionally seeding from a permissively licensed public list). Shipping lists is an editorial/legal stance only the owner can take.
- **Q161** `GET /guilds/{id}/channels` hides channels the caller cannot view (Discord lists them all): recommend keeping the stricter behaviour (privacy over bot parity) and documenting it in the porting guide (doc 05 §7).

### Implementation work packages (all DEFAULT questions)

Shared-file hotspots and the rule for each:

- `crates/sr-api/src/state.rs` and `crates/sr-api/src/routes/mod.rs`: owned by WP-2 only. WP-6 and WP-11 consume the `Ext<T>` extractor WP-2 adds; land WP-2 first.
- `crates/sr-events/src/event.rs` (`DomainEvent`): owned by WP-5. WP-6/WP-7 request new variants in WP-5's PR or rebase onto it; never add variants in parallel.
- `crates/sr-core/src/error.rs` and `permissions.rs`: owned by WP-4. Other packages map to codes WP-4 adds; land WP-4 first (it is small).
- `crates/sr-db/migrations/`: numbering per decade, assigned here: WP-1 `0040–0049`, WP-3 `0050–0059`, WP-6 `0060–0069`, WP-7 `0070–0079`, WP-8 `0080–0089`. Never renumber a merged file.
- `docs/open-questions.md`: no package edits it. The integrator deletes decided entries in one sweep after this file merges.
- `pnpm-lock.yaml`: only WP-9, WP-10, WP-12 touch it; each regenerates with `pnpm@10.12.1` and rebases before merge.

| WP | Scope (crates/dirs) | Questions |
|---|---|---|
| WP-1 Auth & accounts | `sr-auth`, `sr-db/{auth_repo,registration,accounts}.rs`, `sr-cli`, `sr-api/src/routes/auth` (store trait move only), migrations 004x | 1, 3, 4, 5, 39, 40, 42, 43, 44, 45, 47, 56, 141, 142, 143, 144, 145 |
| WP-2 Composition root & API plumbing | `sr-core/src/config.rs`, `sr-api/src/{state,router,rate_limit,openapi}.rs`, `sr-api/src/routes/mod.rs`, `sr-api/src/auth/csrf.rs`, `sr-server/src/*`, `sr-features/src/feature.rs` | 10, 15, 55, 57, 59, 104, 105, 106, 107, 140, 147, 149, 155, 156 |
| WP-3 Data, jobs, storage & ops | `sr-db/{pool,partitions,jobs}.rs`, migrations 005x, `sr-storage`, `sr-server/src/jobs.rs` (new, handlers only), `deploy/`, `.github/workflows`, `.githooks`, docs 04 | 12, 13, 18, 23, 24, 25, 41, 92, 93, 94, 103, 109 |
| WP-4 sr-core primitives & error codes | `sr-core/src/{error,permissions,time}.rs`, `sr-permissions-wasm`, `sr-chat/src/message_rules.rs` (pin rule only) | 6, 7, 9, 95, 96, 120, 135 |
| WP-5 Gateway & events | `sr-gateway`, `sr-events`, `sr-server/src/gateway_pg.rs`, `apps/web/src/gateway/client.ts` (IDENTIFY token only) | 48, 49, 50, 51, 52, 53, 54, 123, 125, 150, 159, 160 |
| WP-6 Messages & guild endpoints | `sr-api/src/routes/{messages,guilds}`, `sr-chat/src/{repo,admin_service,message*}.rs`, `sr-db/{chat_repo,guild_admin_repo}.rs`, migrations 006x | 97, 98, 148, 151, 152, 153, 154, 157, 158 |
| WP-7 Social model | `sr-chat/src/social`, `sr-db/social_repo.rs`, migrations 007x | 110, 111, 112, 113, 114, 115, 116, 117, 118, 119 |
| WP-8 Search & server settings | `sr-search`, `sr-chat/src/settings`, `sr-db/settings_repo.rs`, migrations 008x | 131, 132, 134, 137, 138 |
| WP-9 Markdown & WASM | `sr-markdown`, `sr-markdown-wasm`, `packages/markdown-react`, `packages/wasm`, `.github/workflows/wasm.yml` | 28, 31, 68, 86, 87, 88, 89, 90, 91 |
| WP-10 UI kit & theme tooling | `packages/ui`, `sr-theme` (TS emitter, validator), `apps/web/public/icons` | 32, 35, 36, 60, 61, 62, 64, 65, 66, 70, 71, 72, 73, 74, 75, 77 |
| WP-11 Entrance web | `apps/web/src/{routes/entrance,components,api/client.ts,api/entrance.ts,lib}`; passkey/Discord routes in `sr-api/src/routes/auth/{passkey,discord}.rs` (new files) | 78, 79, 80, 82, 83, 84 |
| WP-12 Lounge web shell & store | `apps/web/src/{lounge,store,router.tsx}` | 128, 129, 163, 164, 167, 169, 170, 171 |

Suggested order: WP-4 and WP-2 first (everything else plugs into them), then WP-5, then the rest in parallel.

---

## Auth (`feat/AUTH-primitives`)

**1. Recovery code strength.**
Decision: 10 codes of 80 bits each, shown as `xxxx-xxxx-xxxx-xxxx` (16 Crockford base32 chars), stored as SHA-256 digests with a unique index on `(account_id, code_hash)` so verification is one lookup, not 10 argon2 checks (see Q39 for the migration). Wrong-code handling stays as doc 05 §5.1 (pending login dropped after 5 wrong codes).
Rationale: doc 04 §1 and doc 09 §2 say tokens (incl. `recovery_codes`) are SHA-256 hashes; 80 random bits need no slow hash.
Status: `DEFAULT`

**2. TOTP secret encryption key source.**
Decision: resolved by `server.secret_key` (64 hex, `sr-core` config) passed to `PgAuthRepo` in `sr-server/src/app.rs`. Rotation is Q40.
Rationale: `crates/sr-core/src/config.rs::ServerConfig::secret_key`, `crates/sr-server/src/app.rs::secret_key`.
Status: `DONE`

**3. Hash invite codes at rest?**
Decision: no. Keep the canonical plain code as the unique lookup key. A leaked code is single-use, revocable, cap-gated and consumes the issuer's credit, so it is not a secret worth a slow hash.
Rationale: doc 06 §5.1 treats the code as a lookup key; doc 04 §1 `platform_invites.code text unique`.
Status: `DEFAULT`

**4. Argon2 and session defaults.**
Decision: keep argon2id 64 MiB / t=3 / p=1; sessions 30 days idle, 180 days absolute (cookie `Max-Age` = 180 d, idle enforced server-side); reset token 30 min; verify token 24 h. Expose the four numbers as `AuthConfig` fields, not product settings.
Rationale: OWASP argon2id guidance; doc 05 §5.1 already codifies 180 d absolute and the token lifetimes; doc 06 §4.
Status: `DEFAULT`

**5. Password strength (zxcvbn) and CSRF location.**
Decision: move the zxcvbn check now in `sr-api/src/routes/auth/validate.rs` into `sr_auth::password::strength(password, user_inputs) -> Verdict` (min 10 chars, score ≥ 3) so the CLI (`bootstrap`, `user set-password`) uses the same rule. CSRF stays in `sr-api` (HTTP concern, Q55).
Rationale: doc 06 §3 (zxcvbn Rust crate, score ≥ 3); doc 02 §3 puts password logic in `sr-auth`.
Status: `DEFAULT`

## Core (`feat/FND-CORE`)

**6. Discord's extra permission rules.**
Decision: add to `sr_core::permissions::compute`: (a) a member whose `communication_disabled_until > now` keeps only `VIEW_CHANNEL | READ_MESSAGE_HISTORY` unless `ADMINISTRATOR`/owner; (b) without `VIEW_CHANNEL` the channel permission set is empty; (c) without `SEND_MESSAGES` strip `SEND_TTS_MESSAGES`, `MENTION_EVERYONE`, `EMBED_LINKS`, `ATTACH_FILES`; (d) without `CONNECT` strip the voice bits. Needs the browser clock from Q7 for (a).
Rationale: Discord permission docs ("implicit permissions", timeout semantics); doc 07 §1 "Discord's model exactly".
Status: `DEFAULT`

**7. Browser clock.**
Decision: add `JsClock` (`js_sys::Date::now()`) in `crates/sr-permissions-wasm/src/clock.rs`, used by its exported `compute`; `sr-core` keeps `SystemClock` behind `#[cfg(not(target_arch = "wasm32"))]` and a `Clock` trait that both implement.
Rationale: doc 03 §3 (`sr-permissions` to WASM); `crates/sr-core/src/time.rs` comment.
Status: `DEFAULT`

**8. Permission fixtures hand-derived.**
Decision: no capture now; the Phase 6 conformance suite (`crates/sr-compat-tests`, doc 05 §7) records real fixtures from a test guild. Keep the hand-derived ones.
Rationale: needs a human with a Discord bot and guild; doc 13 Phase 6.
Status: `NO-CODE`

**9. Error-code list is a subset.**
Decision: WP-4 adds in one PR: `20016 Slowmode rate limit` (429), `30008 Max emojis`, `30039 Max stickers`, `30031 Max group DMs`/friend limits as in Discord's table, the `8000x` relationship family (Q120), `10062 Unknown interaction` placeholder not needed yet. Copy codes/messages verbatim from Discord's "Opcodes and Status Codes" table; SIDE ROOM-only cases use `9001xx` (relationships) and `9002xx` (expressions).
Rationale: doc 05 §3 (reuse Discord codes; SR codes ≥ 900000).
Status: `DEFAULT`

**10. Config schema/defaults.**
Decision: accept the schema; it matches doc 12 §3 (infra only). Add now: `server.trust_forwarded_for: bool = false` (Q57/Q143), `[smtp] host, port, username, password, tls = "starttls"|"tls"|"none", from` (Q145), `database.auto_migrate = false` (Q106), `server.secret_key_previous: Vec<Secret>` (Q40). Everything product-level (limits, prices, cap) stays in the DB.
Rationale: doc 12 §3.
Status: `DEFAULT`

## Database (`feat/FND-DB`)

**11. Migrations never run against Postgres.**
Decision: owner runs `just dev && SR_TEST_DATABASE_URL=postgres://sideroom:sideroom_dev@127.0.0.1:5432/sideroom cargo test -p sr-db -- --ignored` before the next integration merge; add the same as a `rust.yml` job with a `postgres:17` service container (WP-3 may add the CI job).
Rationale: AGENTS.md §4 (PRs must pass tests); no Postgres on agent machines.
Status: `NO-CODE`

**12. Additions beyond doc 04.**
Decision: confirmed. `login_events` (180 d, doc 09 §1), `accounts.age_confirmed` (doc 06 §7), `jobs` as 0004. Record them in doc 04 §1 and §13 (WP-3 doc edit).
Rationale: doc 09 §1 row "Sessions, login IPs"; doc 06 §7; doc 02 §6.
Status: `DEFAULT`

**13. sqlx TLS features.**
Decision: enable sqlx feature `tls-rustls` in `crates/sr-db/Cargo.toml`; default `sslmode=prefer` in the URL docs, `require` in `deploy/INSTALL.md`. No native-tls.
Rationale: D-150 (pure Rust); doc 12 §8 hardening.
Status: `DEFAULT`

**14. FND-DB-007 partition manager not started.**
Decision: exists as `crates/sr-db/src/partitions.rs` plus SQL `sr_ensure_message_partition` in 0012; scheduling is Q41/Q94.
Rationale: commit `4ce04d6`, migration 0012.
Status: `DONE`

## Events and features (`feat/FND-EVT-FEAT`)

**15. `Feature::routes()`, `FeatureCtx`, `Migration` placeholders.**
Decision: `Feature::routes(&self) -> Option<axum::Router>` (unit state; features own their `Arc` state) nested by `sr-server` under `/api/v1/sr/features/{id}/`; `FeatureCtx { bus: Arc<dyn EventBus>, settings: FeatureService, clock: Arc<dyn Clock> }`; drop `Feature::migrations()` from the trait, feature tables go in `crates/sr-db/migrations` like everything else (one forward-only series).
Rationale: doc 11 §1.1 (shape), AGENTS.md §4 (single migration series); avoids an `sr-features` → `sr-api` dependency cycle.
Status: `DEFAULT`

**16. Merge conflict in root `[workspace.dependencies]`.**
Decision: resolved; one table in `Cargo.toml`.
Rationale: verified on `integration/phase0`.
Status: `DONE`

## Web (`feat/FND-REPO-004-web`)

**17. FND-API-007 ticked but a stub.**
Decision: resolved; the full client lives in `apps/web/src/gateway/`. WP-12 deletes the leftover `apps/web/src/api/gateway.ts` stub.
Rationale: commit `7e73adb`, Q122–Q129.
Status: `DONE`

**18. Pre-commit hook not installed.**
Decision: add `.githooks/pre-commit` (POSIX sh: `cargo fmt --all --check` and `pnpm exec lint-staged`) and `just setup` that runs `git config core.hooksPath .githooks` and `corepack enable`. No husky dependency.
Rationale: doc 03 §5 (pre-commit: `cargo fmt`, `pnpm lint-staged`).
Status: `DEFAULT`

**19. pnpm not installed locally.**
Decision: install via `corepack enable && corepack prepare pnpm@10.12.1 --activate` (documented in `docs/dev/getting-started.md`). Agents must not fall back to npm for lockfile-changing work.
Rationale: `package.json` `packageManager`.
Status: `NO-CODE`

## Ops (`chore/FND-OPS`)

**20. Pinned versions are guesses.**
Decision: owner (or first agent with Docker) runs `just dev`, pins Garage and Mailpit to the latest tags that start, and checks `bootstrap.sh` flags against `garage --help`; record the versions in `deploy/compose/README.md`.
Rationale: needs Docker; doc 12 §1.
Status: `NO-CODE`

**21. `deny.toml` / `garage.toml` unvalidated.**
Decision: `cargo deny check` already runs in `just lint`/CI (`rust.yml`); `garage.toml` is validated by Q20's run. No further action.
Rationale: `justfile` `lint` recipe.
Status: `NO-CODE`

**22. Workflows depend on missing pieces.**
Decision: resolved: `deploy/docker/*.Dockerfile`, `packages/api-client`, `packages/wasm`, `sr-server openapi`, `sr-cli seed` and the three `*-wasm` crates exist. First green run of `release.yml`/`openapi.yml` is confirmed by CI, not by code.
Rationale: commit `554b70a`; `docs/dev/server.md`.
Status: `DONE`

## Storage (`feat/FND-STOR`)

**23. Client-driven resumable multipart.**
Decision: not needed for chat (one presigned PUT up to 500 MiB; S3 single PUT allows 5 GiB). Library submissions (Phase 4) need resumable uploads: implement presigned `CreateMultipartUpload`/`UploadPart`/`Complete` with our own SigV4 signer in `sr-storage::multipart` (no new crate) when `sr-review` lands.
Rationale: doc 02 §4.5 (chat: presigned PUT; submissions: resumable multipart); D-082.
Status: `DEFAULT`

**24. "30 MB" as MiB.**
Decision: binary units internally (30 MiB = 31,457,280; 500 MiB; 64 MiB), displayed as "30 MB" in copy, matching Discord (its "10 MB" is 10 MiB). Add a one-line note to doc 08 §2.1.
Rationale: Discord parity; D-042 fixes the numbers, not the unit.
Status: `DEFAULT`

**25. EXIF stripping drops orientation.**
Decision: apply the orientation to pixels first (`image::DynamicImage::apply_orientation` from the decoded EXIF), then strip all metadata. Same in `sr-image-tools` (client) later.
Rationale: Discord strips EXIF and uploads display upright; doc 03 §3 `sr-image-tools` "EXIF stripping".
Status: `DEFAULT`

**26. Presigned URLs untested against Garage.**
Decision: run the `#[ignore]` test against `just dev` (add `SR_TEST_S3_ENDPOINT` gating if missing) before Phase 0 exit.
Rationale: needs Docker.
Status: `NO-CODE`

**27. FND-STOR-004 unticked; FND-STOR-003 untouched.**
Decision: FND-STOR-004 is done (`crates/sr-api/src/routes/messages/attachments.rs`, commit `5f042a8`); tick it. FND-STOR-003 (image resizer) remains its own inventory item (needed by Q70/Q157).
Rationale: commit `5f042a8`.
Status: `DONE`

## Markdown (`feat/LNG-markdown`)

**28. Mention cap of 100.**
Decision: keep 100 unique ids per kind (users, roles, channels) per parse; later ones are rendered as text.
Rationale: Discord's `allowed_mentions` arrays cap at 100 ids; no documented parser cap exists.
Status: `DEFAULT`

**29. WASM not built.**
Decision: resolved: `crates/sr-markdown-wasm` and `packages/wasm` exist (commit `21de13e`); CI/browser verification is Q68.
Rationale: repo state.
Status: `DONE`

**30. Shortcodes and jumbo detection.**
Decision: resolved (LNG-MD-003 implemented).
Rationale: commit `06202c1`.
Status: `DONE`

**31. Parsing quirks to confirm.**
Decision: (a) a fence starting mid-line opens a code block: keep; (b) `>>> ` quotes to the **end of the message**, with code fences nested inside the quote (not terminating it): change; (c) `* text` at line start is a list item: keep.
Rationale: Discord client behaviour (simple-markdown rules; multi-line quotes swallow the rest of the message).
Status: `DEFAULT`

## UI kit (`feat/DS-tokens-prims`)

**32. Token types hand-written.**
Decision: see Q64: `packages/ui/src/theme/tokens.generated.ts` is emitted from the `sr-theme` registry by a Rust binary; hand-written types are deleted.
Rationale: DS-TOK-001; AGENTS.md §3.2 (shared logic from one Rust crate).
Status: `DEFAULT`

**33. Font files missing.**
Decision: owner adds Newsreader, Hanken Grotesk, IBM Plex Mono (all OFL) and a Lounge sans as self-hosted WOFF2 under `apps/web/public/fonts/<family>/` with each `OFL.txt` beside them; `fonts.ts` already expects those paths.
Rationale: DS-TOK-005; doc 10 §2.
Status: `NO-CODE`

**34. `sideroom` tokens not calibrated against mockups.**
Decision: keep doc 10 §2 values until the owner does the visual pass at 1440/1704 px against `docs/assets/mockups`; adjustments land as token-only PRs.
Rationale: doc 10 §2 "calibrate during Phase 0" (a visual judgement).
Status: `NO-CODE`

**35. Saturation via CSS `filter` on the provider root.**
Decision: drop the `filter`. Derive saturated colour tokens with CSS relative colour syntax in `ThemeProvider` (`--c: oklch(from var(--c-base) l calc(c * var(--ui-saturation)) h)`) so no containing block is created and `position: fixed` overlays work. Same fix closes Q61.
Rationale: CSS `filter` creates a containing block for fixed descendants; Discord's saturation setting recolours, it does not break popouts.
Status: `DEFAULT`

**36. Partial primitives unticked.**
Decision: finish in WP-10: Slider, Segmented, ContextMenu/Dropdown gaps, Drawer (Q77), Accordion, AvatarStack, Pill/Tag, ProgressBar, EmptyState, error page, Select (done), ScrollArea (done), icons (Q75), Ladle stories (Q72).
Rationale: doc 10 §6 component list; inventory 01-foundation.
Status: `DEFAULT`

**37. Verified with npm and no lockfile.**
Decision: resolved; `pnpm-lock.yaml` was regenerated with pnpm 10.12.1 and contains the `packages/ui` importer.
Rationale: Q108; `pnpm-lock.yaml` importers.
Status: `DONE`

## CLI and auth repositories (`feat/FND-CLI`)

**38. No Postgres; `#[ignore]` tests never ran.**
Decision: same run as Q11 (`cargo test -p sr-db -- --ignored`).
Rationale: needs Postgres.
Status: `NO-CODE`

**39. Migration 0005 recreates `recovery_codes` as argon2 PHC.**
Decision: not the intended design. Add `0040_recovery_code_sha256.sql`: `code_hash BYTEA NOT NULL CHECK (octet_length(code_hash) = 32)`, unique `(account_id, code_hash)`, drop `code_phc`; `sr-auth::recovery` verifies by lookup (constant-time compare of the digest). Codes per Q1.
Rationale: doc 04 §1 / doc 09 §2 (tokens are SHA-256); removes the 10-argon2 cost from Q1.
Status: `DEFAULT`

**40. TOTP key rotation.**
Decision: prefix every ciphertext with a 1-byte key version; `server.secret_key` is version N, `server.secret_key_previous = [...]` are decrypt-only; `sr-cli rotate-secret-key` re-encrypts all `mfa_totp` rows to the current key.
Rationale: doc 12 §3 (secrets in config); standard envelope rotation.
Status: `DEFAULT`

**41. FND-DB-007 is generic; nothing schedules it.**
Decision: `sr-server/src/jobs.rs` registers handler `db.partitions.ensure` (ensures `messages` partitions for current month + 2) and enqueues it as a recurring daily job at 00:10 UTC via `sr_db::jobs` on boot (idempotent). Same answer as Q94.
Rationale: doc 02 §6 (Postgres job queue); migration 0012 pre-creates 12 months.
Status: `DEFAULT`

**42. Staff Server not created by `bootstrap`.**
Decision: guild tables now exist (0010/0011); `bootstrap` creates the Staff Server through `sr-chat`'s admin service: `is_staff_guild = true`, owner = Owner account, default channels and roles exactly as doc 07 §4, `#alerts` recorded in `guilds.settings.alerts_channel_id`. Then tick FND-CLI-001.
Rationale: doc 06 §8, doc 07 §4, D-130/D-131.
Status: `DEFAULT`

**43. One-time Owner login link reuses `reset_password`.**
Decision: add `EmailTokenPurpose::LoginLink` (TTL 1 h, single use) in `sr-auth`, the enum value in a WP-1 migration, route `POST /sr/auth/login-link {token}` → session cookie, and web route `/login/link?token=` that posts it (WP-11 adds the page).
Rationale: doc 06 §8 step 4; a reset token must not log anyone in.
Status: `DEFAULT`

**44. System user (id 1) stored as `deactivated`.**
Decision: acceptable. Add account flag bit `SYSTEM` (`accounts.flags`), expose `user.system = true` in user objects, keep state `deactivated` so it never counts (D-013) and never logs in.
Rationale: Discord user objects carry `system: bool`; doc 06 §8 step 3.
Status: `DEFAULT`

**45. `seed` only creates accounts and invites.**
Decision: extend `seed` now that 0010–0013 exist: one guild "Side Room Dev" owned by alice with `#general`, `#random`, voice `General`, all four seeded users as members, 200 markdown-rich messages. Library/Cinema samples wait for Phase 4/5; tick FND-DB-009 then.
Rationale: inventory FND-DB-009; schemas available since `9d6654e`.
Status: `DEFAULT`

**46. Emergency `invite create` bypasses credits and logs nothing.**
Decision (recommended): consumes no credit, is not in `invite_credits_ledger`, but writes `platform_audit_log` (`actor_id = 1`, action `invite.emergency_create`, note) — WP-1 creates `platform_audit_log` (doc 04 §7) in its migration. The CLI requires shell access, which is the authorization.
Rationale: D-024 governs user credits, not operator actions; doc 04 §7 lists `platform_audit_log`.
Status: `OWNER`

**47. `user reset-2fa` also removes passkeys and revokes sessions.**
Decision: keep: "reset 2FA" clears TOTP, recovery codes and passkeys, revokes all sessions, and emails the user. Write the `platform_audit_log` row once Q46's table exists.
Rationale: doc 07 §3 `MANAGE_ACCOUNTS` "reset 2FA"; Discord's support-side 2FA removal clears all second factors.
Status: `DEFAULT`

## Gateway (`feat/FND-GATEWAY`)

**48. Dispatch payloads are skeletal.**
Decision: events carry full Discord-shaped payloads built by the producing service (as `MessageSent`/`ChannelCreatedFull` already do); the gateway never hydrates from the DB. Retire the id-only variants (`MessageCreated`, bare `ChannelCreated`, `MemberJoined` → full member object) in WP-5.
Rationale: doc 02 §4.1 (mutations emit events; the gateway turns them into dispatches); Q150/Q160.
Status: `DEFAULT`

**49. zstd per-message frames with `ruzstd`.**
Decision: keep pure-Rust independent frames. Streaming decoders accept concatenated frames; the only loss is cross-message context. Revisit via ADR only if the Phase 6 discord.js/serenity compat suite fails.
Rationale: D-150; doc 03 §4 (a C dependency needs an ADR); doc 05 §1 marks compression optional.
Status: `DEFAULT`

**50. DMs and group DMs not fanned out.**
Decision: each session subscribes to `Topic::User(self)`. DM/group-DM producers publish once per recipient to their `User` topic (recipients resolved from `channel_recipients` in the chat service, ignoring `hidden_at`, which Q114 reopens). No `Channel` topic subscriptions.
Rationale: doc 02 §4.2 fan-out model; Discord delivers DM events per user.
Status: `DEFAULT`

**51. Ops 4/8/31/100–102 ignored; READY extras.**
Decision: add `trait OpHandler { fn ops(&self) -> &[u8]; async fn handle(&self, session: &SessionHandle, op: u8, d: Value) }` registered in `GatewayConfig.handlers`; unknown ops still ignored. Fill `READY.relationships`, `read_state`, `user_settings` via `GuildDirectory::ready_extras` from `sr-chat` social repos (WP-5 + WP-7 interface).
Rationale: doc 05 §6.1/§6.4; voice, member-list and watch-party work plug in without touching `conn.rs`.
Status: `DEFAULT`

**52. Intent handling, MESSAGE_CONTENT, rate limit.**
Decision: first-party sessions get all intents; bots get only approved privileged intents (D-062). Now that events carry content, strip `content`, `embeds`, `attachments`, `components`, `poll` for bots without `MESSAGE_CONTENT` unless the bot is mentioned, the message is a DM, or the bot authored it (Discord rules). Per-connection inbound limit 120 ops / 60 s → close `4008`. No sharding.
Rationale: doc 05 §6.2; Discord gateway rate limit and message-content intent rules.
Status: `DEFAULT`

**53. Presence semantics.**
Decision: set offline when the user's last socket closes (not after the 2-minute resume window; the buffer stays resumable). `GUILD_CREATE` includes `presences` for guilds with fewer than `large_threshold` (default 50, max 250) members; larger guilds get presences via op 8/op 102. REST typing goes through the event bus (Q150).
Rationale: Discord parity (offline is immediate; `large_threshold` semantics).
Status: `DEFAULT`

**54. Slow consumers detached silently.**
Decision: close the socket with code `4000` when the send queue is full; the session stays resumable for 2 minutes (buffer intact) so the client reconnects and `RESUME`s. Keep 128-bit session ids bound to the token; single-node fan-out stays.
Rationale: doc 05 §6.4; Discord disconnects slow clients and expects a resume.
Status: `DEFAULT`

## HTTP API (`feat/FND-API`)

**55. CSRF design.**
Decision: keep the derived token (`SHA-256("sideroom/csrf/v1" || session token)`, no storage) issued by `GET /sr/csrf`; add an `Origin`/`Referer` allow-list check (same list as CORS) on every cookie-authenticated non-safe request as defence in depth (Q146 already does this pre-login).
Rationale: doc 05 §2; doc 02 §8 (`SameSite=Lax` cookie).
Status: `DEFAULT`

**56. Auth backends in `sr-api`; bot token format.**
Decision: move `BotBackend`/`BearerBackend` traits to `sr_auth::tokens` (`BotTokenRepository`, `OAuthTokenRepository`); `sr-api` keeps extractors only. Bot token: Discord-style three parts `base64url(app_id).base64url(issued_at).base64url(32 random bytes)`, stored as SHA-256 in `bot_tokens` with `revoked_at`. Tables and Postgres impls land in Phase 6 with `applications`; `NoBots`/`NoBearers` stay until then.
Rationale: doc 02 §3 (`sr-auth` owns tokens); doc 09 §2; doc 04 §6.
Status: `DEFAULT`

**57. Rate-limit numbers; headers; proxy IP.**
Decision: keep 50/s global, 40/10 s default and the route overrides; derive `Remaining`/`Reset-After` from governor's `StateSnapshot::remaining_burst_capacity()` / `NotUntil::wait_time_from()` and delete the side tracker. Client IP: peer address unless `server.trust_forwarded_for = true`, then the last `X-Forwarded-For` entry (Caddy appends). Shared with Q143.
Rationale: doc 02 §8 (`governor`, Discord-style headers); doc 12 §1 (Caddy in front).
Status: `DEFAULT`

**58. `/internal/metrics` listener; `/readyz` checks.**
Decision: resolved: `server.metrics_bind` config, `DbReady` + `StorageReady` readiness checks, gateway/pool gauges wired in `sr-server/src/app.rs`. Job metrics get callers in WP-3's `jobs.rs`.
Rationale: `crates/sr-server/src/{app,ready}.rs`.
Status: `DONE`

**59. OpenAPI coverage, viewer, utoipa version.**
Decision: annotate every handler in `routes/{auth,messages,guilds}` with `#[utoipa::path]` and merge each module's `openapi()` in `routes::openapi()`; serve the viewer at `/developers/docs/api` with `utoipa-scalar` (single static HTML, loads the JSON); stay on utoipa 5 until a feature needs 6; no OpenTelemetry exporter.
Rationale: doc 05 §4 (spec at `/api/v1/openapi.json`, docs at `/developers/docs/api`); AGENTS.md §4 (contract generated from Rust).
Status: `DEFAULT`

## UI overlays (`feat/DS-prims-overlays`)

**60. Menus: long-press, hover intent, arrows.**
Decision: Phase 0 needs touch long-press (500 ms) for `ContextMenu` (mobile PWA is in the definition of done) and a 150 ms submenu hover-intent delay; no safe-triangle; `Tooltip` gets an arrow (Discord tooltips have one), `Popover` does not.
Rationale: AGENTS.md §5 (PWA); D-040 (Discord look).
Status: `DEFAULT`

**61. Menus portal into the filtered root.**
Decision: fixed by Q35 (no `filter`); menus keep portalling into the theme root.
Rationale: same root cause as Q35.
Status: `DEFAULT`

**62. Select/Combobox choices.**
Decision: accept all four choices (close on checkbox/radio unless `keepOpen`; single-value Combobox; chips without "+N"; fixed row height with `rowHeight` for descriptions). axe run is Q72.
Rationale: Discord's selects behave the same; nothing in the specs requires more.
Status: `DEFAULT`

**63. `@tanstack/react-virtual` added.**
Decision: resolved; it is the chosen virtualizer and is in the regenerated lockfile.
Rationale: doc 03 §3 (TanStack Virtual); `pnpm-lock.yaml`.
Status: `DONE`

## Theme and WASM (`feat/sr-theme-wasm`)

**64. DS-TOK-001 generator.**
Decision: emit TypeScript from the Rust registry, not from JSON Schema: `cargo run -p sr-theme --features emit --bin sr-theme-emit -- ts > packages/ui/src/theme/tokens.generated.ts` (token-name union, per-theme defaults, `UI_TOKEN_NAMES`); `web.yml` fails on drift. `json-schema-to-typescript` is not used.
Rationale: AGENTS.md §3.2 (one Rust source); the registry is not expressible in the schema.
Status: `DEFAULT`

**65. EXT-PKG-002 remainder.**
Decision: restricted CSS via `lightningcss` stays Phase 6 (leave unticked); add byte sniffing now (magic numbers + dimensions through `image::ImageReader::with_guessed_format`, WOFF2 signature `wOF2`); licence file must exist, be non-empty and ≤ 64 KiB, contents not parsed.
Rationale: doc 10 §4 (Phase 6+ for CSS subset).
Status: `DEFAULT`

**66. Theme assumptions.**
Decision: 2 MB per font file (same as images) within the 10 MB total; opaque-hex-only contrast is fine; `extends` only built-ins for now; `ThemeProvider` accepts any `[data-theme=id]`; the signed-CDN asset rewrite waits until a token takes an image (Phase 6 theme store).
Rationale: doc 10 §4 limits; D-063.
Status: `DEFAULT`

**67. Markdown AST ids lose precision in JS.**
Decision: resolved by Q100 (`#[serde(with = "id_string")]`); `packages/markdown-react/src/ast.ts` types ids as strings.
Rationale: commit `a71a9e8`.
Status: `DONE`

**68. WASM pipeline unverified; `web` vs `bundler` target.**
Decision: keep `--target web`; the app initialises with `init(new URL('@sideroom/wasm/dist/sr_markdown_bg.wasm', import.meta.url))` (Vite resolves it). Add one `wasm_bindgen_test` per `*-wasm` crate so `wasm.yml`'s `wasm-pack test --headless --chrome` exercises `JsError` paths; lockfile already regenerated.
Rationale: doc 03 §1 (`wasm-bindgen` + `wasm-pack`); Vite handles `new URL(..., import.meta.url)` natively.
Status: `DEFAULT`

**69. `*-wasm` crates build natively too.**
Decision: keep (`cdylib` + `rlib`); wasm-only paths are covered by Q68's tests. `sr-e2ee`, image tools and `sr-voice-proto` wrappers arrive with their phases; FND-REPO-005 ticked for the existing three is correct.
Rationale: informational.
Status: `NO-CODE`

## UI kit: misc primitives and SR chrome (`feat/DS-prims-misc`)

**70. Avatar animated-on-hover.**
Decision: add `animatedSrc?: string` to `Avatar`; swap on `:hover`/`:focus-visible` and when the row is hovered (`data-animate-avatars`), honour `prefers-reduced-motion`. The static/animated pair comes from FND-STOR-003 (`?size=&animated=false`).
Rationale: D-044 (animated avatars free); Discord plays animated avatars on hover.
Status: `DEFAULT`

**71. Favicon and PWA icons.**
Decision: generate from `packages/ui/assets/door-mark.svg` with `resvg` in a tiny Rust bin behind `just icons` → `apps/web/public/icons/{icon-192,icon-512,maskable-512,apple-touch-180}.png` and `favicon.ico`; commit outputs. The owner may replace the SVG later; regeneration is one command.
Rationale: D-150 (Rust tooling); doc 10 §8 (installable PWA).
Status: `DEFAULT`

**72. Ladle stories and axe coverage.**
Decision: a story + axe test for every primitive (jsdom, `color-contrast` disabled); add `pnpm --filter @sideroom/ui ladle build` to `web.yml`; contrast is checked later by the Playwright axe pass (FND-CI-004).
Rationale: doc 10 §6 ("every component has a story and an axe test").
Status: `DEFAULT`

**73. Native date/time inputs.**
Decision: build a custom calendar grid (`DatePicker`) and time list for desktop (Discord's search filters and events use custom pickers); keep native inputs under 768 px. Phase 1 priority, not Phase 0.
Rationale: D-040 look parity.
Status: `DEFAULT`

**74. `DropdownChip` native `<select>`.**
Decision: swap to the `Select` primitive now that it exists.
Rationale: themed popups; doc 10 §1 (no hard-coded visuals).
Status: `DEFAULT`

**75. Icon set first pass.**
Decision: change `settings` to a gear now (Discord parity); the remaining glyph review against mockups is the owner's visual pass; hand-tuned 16 px variants only for the ten most-used icons when that review flags them.
Rationale: doc 10 §3 (own icons matching Discord's weight).
Status: `DEFAULT`

**76. devDependencies added without lockfile.**
Decision: resolved; `@testing-library/dom`, `axe-core`, `@ladle/react` are in the regenerated lockfile.
Rationale: `pnpm-lock.yaml` importers.
Status: `DONE`

**77. Drawer duplicates Modal's focus trap.**
Decision: extract `useOverlayStack()` + `useFocusTrap()` in `packages/ui/src/primitives/Overlay/`; Modal, Drawer and Menu join one stack; only the topmost handles Escape and scroll lock.
Rationale: single behaviour for all overlays; WCAG 2.2 focus management (doc 10 §7).
Status: `DEFAULT`

## Entrance UI (`feat/ENT-ui`)

**78. Auth endpoint names and shapes.**
Decision: the contract is doc 05 §5.1 for everything implemented; the UI's assumed names become the contract for the missing pieces: `POST /sr/auth/passkey/options`, `POST /sr/auth/passkey/verify`, `GET /sr/auth/discord/start?intent=login|register&next=`, `GET/DELETE /sr/auth/discord/pending`. WP-11 aligns `entrance.ts` to doc 05 (e.g. `errors.<field>._errors[0].message`).
Rationale: doc 05 §5.1 (implemented table and "not implemented yet" line).
Status: `DEFAULT`

**79. Discord OAuth callback ownership.**
Decision: server-side. The Discord app's redirect URI is `https://<host>/api/v1/sr/auth/discord/callback`; the server exchanges the code, stores the pending link in a 10-minute `sr_oauth` cookie and 302s to `/register` or `/login?error=discord_unlinked`. The SPA route `/oauth/discord/callback` only forwards to the API path (legacy of the spec table).
Rationale: doc 06 §3/§4 (server links identity, never auto-registers); keeps the client secret server-side.
Status: `DEFAULT`

**80. CSRF header not sent.**
Decision: `apps/web/src/api/client.ts` fetches `GET /sr/csrf` on boot and after login/register, caches the token in memory, sends `X-SR-CSRF` on every non-GET; on a 403 CSRF error it refetches once and retries.
Rationale: doc 05 §2; Q55.
Status: `DEFAULT`

**81. Door image and display fonts not bundled.**
Decision: owner supplies `apps/web/public/entrance/door.jpg` (and the Admin → Entrance upload later); fonts per Q33.
Rationale: assets.
Status: `NO-CODE`

**82. Client password rules are a heuristic.**
Decision: no client-side zxcvbn in Phase 0; keep the meter heuristic and surface the server's `errors.password` verdict. Revisit an `sr-auth-wasm` strength export only if the owner wants an exact client meter (dictionary size is the cost).
Rationale: doc 06 §3 makes the server authoritative.
Status: `DEFAULT`

**83. Passkey flow untested; `/login/2fa` offers all methods.**
Decision: `/login/2fa` reads `methods` from the `mfa_required` response (sessionStorage) and shows only those; passkeys are tested in Playwright with CDP's virtual authenticator (`WebAuthn.enable`) once FND-CI-004 exists.
Rationale: doc 05 §5.1 (`methods` array); doc 03 §3 (Playwright).
Status: `DEFAULT`

**84. Not built: verify-email page, onboarding, editable copy, Playwright, visual checks.**
Decision: WP-11 builds ENT-UI-020 (`/verify-email/:token` + banner), 022 (onboarding: theme, avatar, "2FA now?"), 010/025 (operator copy from `/sr/features` `entrance.copy` setting, markdown via `sr-markdown`), 026 (Playwright smoke). The 1440/1704/<900 px visual check is the owner's.
Rationale: spec entry-landing-and-login §3, §7.
Status: `DEFAULT`

## Markdown corpus and React renderer (`feat/LNG-markdown-2`)

**85. Corpus blessed from parser output.**
Decision: owner (or anyone with Discord) reviews a stratified 100-case sample against Discord after Q31/Q89/Q90 land; mismatches become hand-written expectations; the rest stay blessed.
Rationale: needs Discord access; spec lounge-chat §15 (corpus parity).
Status: `NO-CODE`

**86. WASM corpus test missing.**
Decision: add `packages/wasm/test/corpus.test.ts` (Vitest, Node) that loads the built wasm and deep-equals AST/plain/jumbo against `crates/sr-markdown/tests/corpus/*.json`; run in `wasm.yml` after `wasm-pack build`.
Rationale: spec lounge-chat §15 ("identical ASTs in Rust and WASM tests").
Status: `DEFAULT`

**87. Hand-curated shortcode table.**
Decision: generate at build time from vendored emojibase data (`emojibase-data/en/shortcodes/discord.json` + `data.json`, MIT) in `crates/sr-markdown/build.rs` into a `phf` map, including skin-tone suffix forms (`:wave::skin-tone-2:`). Vendored files live in `crates/sr-markdown/data/` with the licence.
Rationale: Discord parity (emojibase ships Discord's own shortcode preset); no runtime dependency.
Status: `DEFAULT`

**88. Jumbo rules.**
Decision: keep 1–27 emoji plus whitespace; any other node disables jumbo. Replace range-based detection with emojibase sequences from Q87 so ZWJ sequences and flags count once; `☺`-style text-presentation symbols still count (Discord twemoji-fies them).
Rationale: Discord client constant; parity.
Status: `DEFAULT`

**89. Shortcode parsing side effects.**
Decision: shortcode → unicode expansion moves out of the render parser into `sr_markdown::expand_shortcodes(&str) -> String`, called by the composer at send time (and exported through wasm). The parser leaves `:name:` literal and only recognises custom `<:name:id>`/`<a:name:id>`. Both side effects then disappear.
Rationale: Discord converts shortcodes in the composer; API-sent `:smile:` renders literally.
Status: `DEFAULT`

**90. Parser quirks from the corpus.**
Decision: autolinks exclude a trailing `>` and trailing `.,:;"')]`; `***a** b*` → italic containing bold `a` then ` b` (simple-markdown nesting); `**a*` → literal `*` + italic `a`; `* *` is a list item containing `*`.
Rationale: Discord's simple-markdown rules; verified in Q85's pass.
Status: `DEFAULT`

**91. Renderer gaps.**
Decision: code highlighting with Shiki, lazy-loaded per language in the client only (explicitly allowed by the spec; a syntect→WASM bundle is several MB); relative `<t:…:R>` timestamps re-render on a shared 1-minute ticker; resolvers for custom emoji URLs and Cinema/Library cards stay injectable; lockfile already regenerated.
Rationale: spec lounge-chat §6.1 ("syntect/tree-sitter → WASM, or Shiki").
Status: `DEFAULT`

## Chat model (`feat/LNG-chat-model`)

**92. `member_roles` join table vs `roles bigint[]`.**
Decision: keep the join table; API objects still expose `roles: [ids]`. Update doc 04 §2.
Rationale: FK integrity and cascade on role delete; doc 04 is logical, not DDL.
Status: `DEFAULT`

**93. Migration numbering and merge order.**
Decision: decades per package (see hotspots above); within a PR, files are numbered at merge time by the integrator if a lower gap is needed. Never enable `ignore_missing`. `moderation_holds` from 0011 is extended, not recreated, in Phase 7. Write this rule into CONTRIBUTING.md.
Rationale: AGENTS.md §4 (forward-only); sqlx version ordering.
Status: `DEFAULT`

**94. Message partitions: future months; `message_edits` purge.**
Decision: the daily `db.partitions.ensure` job from Q41 covers it; `messages.purge_deleted` (hourly, doc 09 §1) deletes `message_edits` rows explicitly in the same transaction.
Rationale: doc 02 §6; doc 09 §1.
Status: `DEFAULT`

**95. No `PIN_MESSAGES` bit.**
Decision: add `PIN_MESSAGES = 1 << 51` and `BYPASS_SLOWMODE = 1 << 52`; pinning requires `PIN_MESSAGES` (not `MANAGE_MESSAGES`); a one-time migration grants `PIN_MESSAGES` to every role that has `MANAGE_MESSAGES`, as Discord did.
Rationale: doc 05 §3 ("exactly Discord's bit positions"); Discord 2025 permission split.
Status: `DEFAULT`

**96. Slowmode error code.**
Decision: add `SlowmodeRateLimit = 20016` (HTTP 429, body `retry_after` seconds as float); `ChatError::Slowmode` maps to it.
Rationale: Discord code table; doc 05 §3.
Status: `DEFAULT`

**97. Nonce dedupe not implemented.**
Decision: index `(channel_id, author_id, nonce) WHERE nonce IS NOT NULL` on `messages` (no unique index: partitioned tables require the partition key in unique constraints) plus a moka cache keyed `(channel, author, nonce)` with a 5-minute TTL in the message service. Semantics per Q151.
Rationale: Discord dedupes per channel/author for a short window; Postgres partition constraints.
Status: `DEFAULT`

**98. Domain `Message` lacks embeds/polls/components; mentions unfiltered.**
Decision: add serde types `sr_chat::message::{Embed, Poll, Component}` mirroring Discord's JSON (stored in the existing jsonb columns); `send_message` filters `mention_user_ids` to guild members who can view the channel (also feeds Q117).
Rationale: doc 04 §3 columns; doc 05 §3 shapes.
Status: `DEFAULT`

**99. Postgres code unexecuted (0010–0013).**
Decision: same run as Q11.
Status: `NO-CODE`

**100. AST ids are now JSON strings.**
Decision: resolved; wasm wrapper and `markdown-react` treat `id` as string, numbers still accepted on input.
Rationale: `crates/sr-markdown/src/ast.rs` (`id_string`), Q67.
Status: `DONE`

**101. Inventory BE items unticked.**
Decision: resolved by later branches: GLD-002/004/007, CHN-002/003 (`de85548`), MSG-002..006/008/011/013 (`5f042a8`). Remaining MSG-007 (forward) and MSG-009 (burst reactions) stay unticked as their own items.
Rationale: commit messages on `integration/phase0`.
Status: `DONE`

## Server wiring (`feat/FND-SERVER-wiring`)

**102. Bot and OAuth bearer backends reject everything.**
Decision: correct until Phase 6; schema and token format are fixed by Q56. No action now.
Rationale: D-062; doc 13 Phase 6.
Status: `NO-CODE`

**103. `feature_enablements` cannot express channel scope or inherit.**
Decision: migration `005x_feature_enablements_v2.sql`: add `'channel'` to `enablement_scope`, make `enabled` nullable (`NULL` = inherit), backfill rows with `config.inherit=true` to `NULL` and strip the key.
Rationale: doc 11 §1.1 scopes `Platform | Guild | Channel | User`.
Status: `DEFAULT`

**104. `AppState` has no extension slot.**
Decision: add a typed extension map to `AppState` (`HashMap<TypeId, Arc<dyn Any + Send + Sync>>`): `AppStateBuilder::extension::<T>(Arc<T>)`, `AppState::extension::<T>() -> Option<Arc<T>>`, and an extractor `Ext<T>` (500 `SrInternal` with a log line when missing). Route modules replace `Extension<Arc<X>>`/`router_with` with `Ext<X>`; `sr-server` registers `AuthService`, `MessagesDeps`, `GuildBackend`, `FeatureService`, storage, `PgAuthRepo`. `routes::all()` must merge the three routers (it is currently empty, so nothing is mounted).
Rationale: one composition pattern for every feature (doc 11 §1); avoids `FromRef` boilerplate per service.
Status: `DEFAULT`

**105. Config additions; empty secret key; mismatched dev defaults.**
Decision: refuse to start when `server.secret_key` is empty unless `server.public_url` host is `localhost`/`127.0.0.1`/`[::1]`; keep bucket default `sideroom` and change the compose bootstrap to create `sideroom`; `just serve` exports both `SR_DATABASE__URL` and the Garage keys from compose.
Rationale: doc 12 §2 (`install.sh` generates secrets); dev/prod parity.
Status: `DEFAULT`

**106. No migrate-on-boot; no job handlers.**
Decision: `database.auto_migrate = false` by default; when true, run migrations before binding (single-node installs). `sr-cli migrate` stays the documented path. Register handlers in `sr-server/src/jobs.rs`: `db.partitions.ensure`, `messages.purge_deleted`, `attachments.expire_large`, `attachments.gc_unreferenced`, `search.index_message`.
Rationale: doc 12 §2 step 3; doc 02 §6 job list; doc 09 §1 job names.
Status: `DEFAULT`

**107. Gateway mounted outside the API layers.**
Decision: correct placement (`/gateway` at the root). Add the request-id layer and a connection cap (`gateway.max_connections`, default 10,000) around the WebSocket route; inbound op rate limit per Q52; no `/api/v10` gateway twin.
Rationale: doc 05 §1 (gateway URL at the root); D-015 capacity target.
Status: `DEFAULT`

**108. FND-CI-002 no-op; FND-CI-004 skeleton; unrun workflows; pnpm action change.**
Decision: FND-CI-002 stays a no-op until queries move to `query!` macros (then commit `.sqlx/`); FND-CI-004 unticks until Q84's Playwright config exists; the pnpm action fix is right. First green runs are a CI matter.
Rationale: doc 03 §5 (sqlx offline mode).
Status: `NO-CODE`

**109. Musl builds under emulation.**
Decision: cross-compile both `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` with `cargo-zigbuild` on the x86 runner (no QEMU); `ring`/`aws-lc` link via zig's cc.
Rationale: build-time tool only; keeps release under 15 min.
Status: `DEFAULT`

## Social model (`feat/LNG-social-model`)

**110. Ignore replaces friendship.**
Decision: Ignore is independent of friendship: migration `007x_user_ignores.sql` creates `user_ignores (user_id, target_id, created_at)`; drop relationship type 6; ignoring hides messages/activity/DM notifications but keeps the friend rows. Fix doc 04 §4 (contradiction noted; doc 04 is not the decisions log).
Rationale: Discord's Ignore (2024) keeps the friendship; AGENTS.md §3.3.
Status: `DEFAULT`

**111. Friend nicknames only on friend rows; no `user_notes`.**
Decision: keep (Discord only allows nicknames for friends); 32 chars; add `user_notes (user_id, target_id, note ≤ 256)` and `GET/PUT /users/@me/notes/{id}` in WP-7.
Rationale: Discord limits (nickname 32, note 256).
Status: `DEFAULT`

**112. Message Requests not persisted.**
Decision: add `channel_recipients.request_state enum('accepted','pending','ignored','spam') DEFAULT 'accepted'`; a non-friend's first DM with a shared server creates the channel with the recipient's row `pending` when their `message_requests` toggle is on; `READY.private_channels` carries `is_message_request`/`is_spam`. Non-friends without a shared server cannot DM (403 50007).
Rationale: Discord parity; spec lounge-chat §3.
Status: `DEFAULT`

**113. Group DM rules.**
Decision: accept all stated rules (10 members, ≥ 2 other recipients to create, any member may add their friends, adding to a 1:1 spawns a group, ownership to longest-standing member, empty group deleted). Keep the 100-groups-per-user cap as a platform setting.
Rationale: Discord parity; spec lounge-chat §1.2/§14.
Status: `DEFAULT`

**114. Close DM `hidden_at`; reopen not called.**
Decision: `ChatService::send_message` calls `reopen_for_message` for DM/group channels before publishing (Q50 fan-out then reaches hidden recipients).
Rationale: Discord reopens closed DMs on a new message.
Status: `DEFAULT`

**115. `user_privacy` table vs `user_settings` jsonb.**
Decision: keep the typed `user_privacy` table (server-enforced rules deserve columns); when the Discord-shaped `user_settings` blob lands, these fields are projected into it on read and written back to the table. Note it in doc 04 §4.
Rationale: doc 09 §3 (privacy toggles are enforced server-side).
Status: `DEFAULT`

**116. Notification scope `guild_id = 0`; mutes; missing features.**
Decision: keep `0` internally for DM scope (composite PK), serialize as `guild_id: null`; expired timed mutes are inactive on read, no cleanup job; keyword highlights, thread follows and `suppress_highlights` are LNG-NTF-002, Phase 1 later.
Rationale: Discord `user_guild_settings` uses `guild_id: null` for DMs.
Status: `DEFAULT`

**117. Mention counts and eligibility.**
Decision: eligibility is computed in the send path (Q98: VIEW_CHANNEL check per mentioned user and for `@everyone`/role fan-out); 9,999 cap and ack semantics stay.
Rationale: AGENTS.md §5 (server-side permissions).
Status: `DEFAULT`

**118. Profile limits.**
Decision: Discord parity: bio 190, pronouns 40, custom status 128; per-guild profiles, avatar decoration and profile effects are LNG-PRF-001 (Phase 1 later); `connections.verified` only set by OAuth flows.
Rationale: AGENTS.md §3.3; limits are not cost-related (D-070 does not apply).
Status: `DEFAULT`

**119. Presence merge order.**
Decision: the chosen status (`dnd`/`idle`/`invisible`) is a user setting that applies to all sessions; otherwise `online` if any session is active, `idle` only when every session is idle. Wire `merge_sessions` into `sr-gateway`'s presence map (WP-5 hook, WP-7 impl).
Rationale: Discord stores status in user settings, not per session.
Status: `DEFAULT`

**120. Relationship/group-DM error codes.**
Decision: WP-4 adds Discord's `80000–80007` family and `30031`-style maxima verbatim from the code table; SIDE ROOM-only failures (ignore list, group-DM cap) use `900101+`.
Rationale: doc 05 §3.
Status: `DEFAULT`

**121. Postgres code unexecuted (0020–0023).**
Decision: same run as Q11.
Status: `NO-CODE`

## Web gateway client and store (`feat/FND-web-gateway-store`)

**122. zstd decoding in the browser.**
Decision: defer. Plain JSON over TLS for Phase 0/1; when payload volume justifies it, compile `ruzstd` into `packages/wasm` (Rust-first) and plug it into the existing `decompress` hook. No `fzstd`.
Rationale: D-150; doc 05 §1 (compression optional); no measured need yet.
Status: `NO-CODE`

**123. IDENTIFY token for cookie sessions.**
Decision: cookie auth on the upgrade: the gateway's `authenticate` accepts an empty token when the upgrade request carries a valid `sr_session` cookie **and** its `Origin` is in the allowed list; `Bot`/`Bearer` tokens unchanged. No ticket endpoint.
Rationale: mirrors REST auth (doc 05 §2); least new surface; Origin check blocks cross-site WebSocket hijacking.
Status: `DEFAULT`

**124. Default intents; no op 102 yet.**
Decision: fine for Phase 1 (REST member list up to 1,000); op 102 lazy ranges arrive with the member-list inventory item.
Rationale: spec lounge-chat §7.
Status: `NO-CODE`

**125. Assumed REST endpoints and READY/GUILD_CREATE hydration.**
Decision: the REST routes exist (Q101); `PgGuildDirectory` (WP-5) must build full `READY` (`user`, `guilds` stubs, `private_channels`, `read_state`, `relationships`, `user_settings`, `sessions`) and `GUILD_CREATE` (`channels`, `roles`, `members` ≤ `large_threshold`, `presences`, `threads`) objects from `sr-chat` repos.
Rationale: doc 05 §6.4; Discord payload shapes.
Status: `DEFAULT`

**126. Fresh READY clears windows; RESUME refetches nothing.**
Decision: correct as is.
Rationale: Discord parity (RESUME replays missed events).
Status: `NO-CODE`

**127. Append-only windows capped at 300; no `around`.**
Decision: accepted; the `around`/permalink window comes with LNG-MSG jump-to-message.
Rationale: Discord's client behaves the same.
Status: `NO-CODE`

**128. Read state and acks.**
Decision: the UI posts `POST /channels/{id}/messages/{id}/ack` when the channel is focused, the window is at the live edge and the newest message is visible (throttled 1/s); role-mention and mute logic come with LNG-NTF work.
Rationale: Discord ack behaviour.
Status: `DEFAULT`

**129. Typing throttle; presence hooks.**
Decision: composer sends `POST /channels/{id}/typing` at most every 8 s while the user types; per-guild presence indexes when the member list lands.
Rationale: Discord typing cadence (~8–10 s).
Status: `DEFAULT`

## Search and server settings model (`feat/LNG-search-settings-model`)

**130. Search filters take ids; dates in UTC; no negation.**
Decision: accepted; the search UI resolves names to ids via autocomplete (Discord's API also takes ids). Negation is a later item.
Rationale: Discord search API parameters.
Status: `NO-CODE`

**131. Permission allow-list cost.**
Decision: cache the per-(user, guild) viewable-channel set in moka (TTL 60 s), invalidated on `RoleUpdated`/`ChannelUpdated`/`MemberUpdated`; NSFW gating uses `accounts.age_confirmed_at`; `authorType:` waits for `webhook_id`/`application_id` in the indexer.
Rationale: doc 02 §5 (moka caches); doc 06 §7.
Status: `DEFAULT`

**132. Indexing not wired.**
Decision: `search.index_message` job handler in `sr-server/src/jobs.rs` consumes `MessageSent/Edited/Deleted`, converts via `impl From<&sr_chat::Message> for sr_search::MessageEvent` (plain text through `sr_markdown::to_plain_text`, `has:` flags computed there); default tokenizer stays.
Rationale: doc 02 §6 (search indexing job); spec lounge-chat §9.
Status: `DEFAULT`

**133. Audit-log retention.**
Decision (recommended): 45 days for guild `audit_log_entries` (Discord), platform setting `moderation.audit_retention_days` (`0` = forever); `platform_audit_log` and `staff_access_log` forever. Each guild mutation calls `record_audit` (LNG-SSET-056) in WP-8.
Rationale: Discord parity; doc 09 §1 is silent for guild logs and explicit (forever) for the staff log.
Status: `OWNER`

**134. Guild invite options; "external"; temporary membership.**
Decision: the invite dialog's "Copy link" action calls `POST /channels/{id}/invites` with `external: true` → `InviteOptions::external_default()` (max_uses 1); in-platform shares use the server default. Kick-on-disconnect for temporary members is Phase 2 (voice).
Rationale: D-022; doc 06 §6.
Status: `DEFAULT`

**135. Expression limits and levels.**
Decision: server levels do not raise emoji/sticker/sound slots (250+250 / 60 / 48 for everyone); remove the "injectable from level" idea; add codes `30008`/`30039` (WP-4).
Rationale: doc 08 §2.2 table row "Emoji / sticker / sound slots: same".
Status: `DEFAULT`

**136. AutoMod presets, normalisation, state.**
Decision (recommended): ship no preset lists; operator-managed lists in Admin → Features → AutoMod. Independently (DEFAULT work): NFKC normalisation + confusables folding via `unicode-normalization`, per-guild compiled-rule cache in moka, and a per-member sliding window store for duplicate/rate spam; keep the extra "Links" trigger.
Rationale: content lists are an editorial/legal stance; D-041 includes AutoMod.
Status: `OWNER`

**137. Webhook token hashing.**
Decision: token = 68 chars base64url (Discord-length), stored as SHA-256, no pepper; `POST /webhooks/{id}/{token}` execute path with `?wait` and `thread_id` in WP-8's endpoint work.
Rationale: doc 09 §2; doc 05 §4.
Status: `DEFAULT`

**138. Templates and community duplication.**
Decision: `guilds.rules_channel_id`/`public_updates_channel_id` are the source of truth (they are Discord guild fields); drop the duplicates from `guild_community` in a WP-8 migration. One template per guild, code = 16 base62 chars; `instantiate` lands with the create-server flow (LNG-SSET-070).
Rationale: Discord guild object; doc 04 §2.
Status: `DEFAULT`

**139. Postgres code unexecuted (0030–0036).**
Decision: same run as Q11.
Status: `NO-CODE`

## Entrance auth endpoints (`feat/ENT-auth-endpoints`)

**140. Wiring `AuthService`.**
Decision: `AppState` extension (Q104); `sr-server` constructs `AuthService` (Postgres store from Q141, `SmtpMailer`/`LogMailer`, snowflake `IdSource`, `AuthConfig`) and registers it; `router_with` is removed.
Rationale: Q104.
Status: `DEFAULT`

**141. No Postgres `AuthStore`.**
Decision: move the `AuthStore` trait from `sr-api/src/routes/auth/store.rs` to `sr_auth::store` and implement it on `sr_db::auth_repo::PgAuthRepo` (sessions/TOTP/recovery/email tokens already there; add lookups by login/email/id, `set_password_hash`, `mark_email_verified`, `set_age_confirmed`, and `register_with_invite` delegation). Keep `MemoryAuthStore` for tests.
Rationale: dependency direction (`sr-db` must not depend on `sr-api`); doc 02 §3.
Status: `DEFAULT`

**142. Person matching and banned-Person alerts.**
Decision: implement in `register_with_invite`: match `external_identities(discord)` then verified email across all accounts (incl. deleted) → reuse `person_id`; banned Person → generic `900006` and publish `DomainEvent::StaffAlert { kind: BannedPersonAttempt }` on `Topic::Platform` (gateway → `SR_STAFF_ALERT`; Staff Server `#alerts` message once the system user can post).
Rationale: doc 06 §3 steps 4–5; D-030; doc 05 §6.3.
Status: `DEFAULT`

**143. In-memory throttles; lockout by name.**
Decision: single-node in-memory is fine (doc 02 §5). Lockout key becomes `(login name, client IP)` with the 5-failure back-off, plus 30/IP and a 50/hour per-login-name ceiling; IP source per Q57. No proof-of-work.
Rationale: doc 06 §4 (per account and per IP); mitigates remote lockout of a victim.
Status: `DEFAULT`

**144. "Trust this device" ignored; missing flows.**
Decision: add `trusted_devices (id, account_id, token_hash, device_name, created_at, expires_at, revoked_at)` and a 30-day `sr_trusted` cookie (random 256-bit, SHA-256 stored); `/sr/auth/login` skips 2FA when it matches; listed/revocable in Devices. Passkeys, Discord OAuth, new-device alerts and the "2FA changed" email are their inventory items; plain-text + simple HTML mails are enough (no MJML).
Rationale: spec entry §4 ("Trust this device for 30 days"); doc 09 §2.
Status: `DEFAULT`

**145. SMTP without TLS.**
Decision: lettre feature `tokio1-rustls-tls`; `smtp.tls = "starttls"` default (`"tls"` for 465, `"none"` only for Mailpit); no native TLS, no ADR needed.
Rationale: D-150; doc 03 §2 (lettre).
Status: `DEFAULT`

**146. Choices not in the docs.**
Decision: all accepted and already recorded in doc 05 §5.1 (auto-login on register, field errors, `/sr/age-gate`, Origin allow-list for pre-login POSTs, 180-day cookie, reset revokes sessions).
Rationale: doc 05 §5.1.
Status: `DONE`

**147. No OpenAPI annotations on auth routes.**
Decision: covered by Q59 (annotate and merge); the `PasswordReset` page change is correct.
Rationale: Q59.
Status: `DEFAULT`

## Message endpoints (`feat/LNG-message-endpoints`)

**148. `MessageStore` trait location; no Postgres store.**
Decision: move the trait into `sr_chat::repo::MessageStore` (history paging, nonces, pins, reactions, attachment rows) and implement `PgMessageStore` in `sr-db/src/chat_repo.rs` with `#[ignore]` tests; `sr-api` keeps HTTP only; `MemoryBackend` moves behind `sr-chat`'s `memory` feature.
Rationale: doc 02 §3 (repositories in `sr-db`, domain in `sr-chat`).
Status: `DEFAULT`

**149. `AppState` slot for chat services; shared edits.**
Decision: `Ext<MessagesDeps>` via Q104; the shared edits (`pub mod routes`, deps, `serde_json` in `sr-events`) stand.
Rationale: Q104.
Status: `DEFAULT`

**150. Full-payload event variants; REST typing bypasses the gateway.**
Decision: retire `MessageCreated` (hub and tests switch to `MessageSent`); authors become full user objects via Q157's lookup; the gateway records typing from the `TypingStarted` bus event (drop direct `Gateway::typing_start` calls) so REST and gateway paths converge. DMs per Q50.
Rationale: one event path (doc 02 §4.1).
Status: `DEFAULT`

**151. Nonce semantics.**
Decision: Discord parity: dedupe (return the original with 200) only when `enforce_nonce: true`; without it a second message is created. Window 5 minutes, nonce ≤ 25 chars.
Rationale: Discord `enforce_nonce` documentation; AGENTS.md §3.3.
Status: `DEFAULT`

**152. Create-message gaps.**
Decision: implement `allowed_mentions` now (it gates pings; parse, roles, users, replied_user); accept and store `tts` as the flag only; `poll`, `sticker_ids`, multipart file posts, burst reactions, emoji usability checks and `MESSAGE_ACK` stay their inventory items (LNG-MSG-009/010/020, LNG-EXP).
Rationale: Discord `allowed_mentions` semantics; D-041 scope.
Status: `DEFAULT`

**153. Upload slots.**
Decision: accept the single-PUT design; sign `/cdn/attachments/...` URLs (HMAC with `server.secret_key`, 24 h TTL) when serving; hourly job `attachments.gc_unreferenced` deletes `pending` slots older than 24 h and their objects; 404 `10003` for foreign upload ids is correct.
Rationale: doc 02 §8 (object URLs signed, buckets never public); doc 02 §6 jobs.
Status: `DEFAULT`

**154. Pin system message and slowmode; bearer scopes.**
Decision: system messages (type ≠ 0/19) bypass slowmode and content limits; pin permission per Q95; bearer scope checks (`messages.read`) are enforced by the auth layer when OAuth lands (Phase 6), not per route.
Rationale: Discord does not slowmode system messages; doc 05 §4 OAuth2 phase.
Status: `DEFAULT`

## Guild REST endpoints (`feat/LNG-guild-endpoints`)

**155. Wiring is the composition root's job.**
Decision: `Ext<GuildBackend>` via Q104; `routes::all()` merges `auth::router()`, `messages::router()`, `guilds::router()` and `routes::openapi()` merges their documents; `sr-server` registers the backend.
Rationale: Q104; `routes/mod.rs` is currently empty.
Status: `DEFAULT`

**156. Route overlap risk.**
Decision: ownership table: guilds module owns `GET/PATCH/DELETE /channels/{id}` for every channel type (DM/group DM branch delegates to the DM service and returns the DM object) and `/users/@me/guilds*`; messages module owns `/channels/{id}/{messages,pins,typing,attachments}*`; a future users module owns `/users/*` except `/users/@me/guilds`; DM module owns `POST /users/@me/channels`. Add a test that builds `routes::all()` so duplicate routes fail in CI.
Rationale: `Router::merge` panics at startup; doc 05 §4 route families.
Status: `DEFAULT`

**157. Member/ban objects carry only `user.id`; media fields ignored.**
Decision: add `trait UserLookup { async fn users(&self, ids: &[UserId]) -> Vec<UserSummary> }` in `sr-chat`, implemented in `sr-db` over `accounts`, injected into guild and message DTO builders (also fixes message authors, Q150). `icon`/`banner`/`splash` land with FND-STOR-003.
Rationale: Discord member/ban/message objects embed user objects.
Status: `DEFAULT`

**158. Ban `delete_message_seconds` and missing endpoints.**
Decision: implement the purge as bulk `visibility = user_deleted` for the window (D-121 semantics, dispatch `MESSAGE_DELETE_BULK` per channel) and write the audit entry via `record_audit` (Q133); bulk ban, prune, member search/add and voice moderation remain LNG-GLD-005/006.
Rationale: D-121; doc 05 §3 (`X-Audit-Log-Reason` honoured).
Status: `DEFAULT`

**159. `CHANNEL_DELETE` leaks private channel names; role shift events.**
Decision: compute the viewer set before deletion and carry it in `ChannelDeleted { viewers: Vec<UserId> }`; the gateway filters by it. Emit `GUILD_ROLE_UPDATE` for every role whose position shifts on create/reorder.
Rationale: AGENTS.md §5 (client hiding is never the boundary); Discord sends role updates on reorder.
Status: `DEFAULT`

**160. Bare `ChannelCreated` beside `ChannelCreatedFull`.**
Decision: fold into one `ChannelCreated` carrying the full channel object; update `hub.rs` and gateway tests (WP-5 PR).
Rationale: Q48.
Status: `DEFAULT`

**161. `GET /guilds/{id}/channels` hides unviewable channels.**
Decision (recommended): keep hiding them (privacy over parity); bots with `ADMINISTRATOR` see all; document the difference in the porting guide (doc 05 §7 item 3). Member/ban list defaults (1 / 1000) are correct.
Rationale: AGENTS.md §5; D-125 spirit (private content is private). Parity exception, hence owner call.
Status: `OWNER`

**162. `UnknownBan`, `memory` feature, unexecuted repo.**
Decision: additions are correct and merged; the Postgres run is covered by Q11.
Rationale: `crates/sr-core/src/error.rs` (10026).
Status: `DONE`

## Lounge shell UI (`feat/LNG-shell-ui`)

**163. Mock data seam.**
Decision: point `apps/web/src/lounge/data.ts` at the gateway store hooks (`apps/web/src/store`) with thin shape adapters; `MockDataProvider` stays only behind `VITE_SR_MOCK=1` for Ladle/visual work.
Rationale: FND-API-008 store exists (`7e73adb`).
Status: `DEFAULT`

**164. TypeScript markdown stand-in.**
Decision: replace `lounge/markdown.ts` with `@sideroom/wasm` parsing + `packages/markdown-react` rendering; delete the stand-in once wired (no reimplementation may remain).
Rationale: AGENTS.md §3.2 ("never a reimplementation").
Status: `DEFAULT`

**165. `@me` in URLs.**
Decision: resolved (`pathParamsAllowedCharacters: ['@']`).
Status: `DONE`

**166. `/channels` redirects to the remembered location.**
Decision: resolved; tests assert `startsWith('/channels')`.
Status: `DONE`

**167. Missing primitives built under `lounge/components`.**
Decision: move `RailPill`, `ResizeHandle`, `useMediaQuery`, `ComingSoon` into `packages/ui` and export `observeRectWithFallback` from the ui index; `EmojiPicker` stays in the app (needs server data) but is built from ui primitives.
Rationale: AGENTS.md §3.2 (shared UI kit).
Status: `DEFAULT`

**168. Placeholders that look real.**
Decision: informational; each item is tracked in inventory 03-lounge-shell and stays unticked until built.
Status: `NO-CODE`

**169. Keyboard shortcuts.**
Decision: keep Discord's Alt+Up/Down, Ctrl+Alt+Up/Down and Esc bindings as-is; **drop Ctrl+Shift+M for focus-composer** (it is Discord's toggle-mute) and instead focus the composer when any printable key is pressed outside an input (Discord behaviour). User-rebindable keybinds come with User Settings → Keybinds.
Rationale: D-040 / spec lounge-chat §12 (Discord shortcuts).
Status: `DEFAULT`

**170. Scroll anchoring on history load.**
Decision: anchor by measured `scrollHeight` delta in a `useLayoutEffect` after prepend (robust for mixed-height rows), keep `scrollToIndex` as fallback; the 60 fps / 50k-message check becomes a Playwright perf test under FND-CI-004.
Rationale: spec lounge-chat §15 acceptance criterion.
Status: `DEFAULT`

**171. Drafts and UI memory are client-only.**
Decision: drafts persist per channel in `localStorage` (Discord keeps drafts local too), not module memory; remembered channel per guild, collapsed categories and panel width move to server-side `user_settings` when the settings API lands (LNG-RAIL-007); `localStorage` stays as the offline cache.
Rationale: Discord parity (drafts local; guild folders/collapsed categories in user settings).
Status: `DEFAULT`
