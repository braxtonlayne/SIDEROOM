# Open question answers

> Historical record: this file keeps the full answer to every original open question. The live list of questions that still need a human is in [open-questions.md](open-questions.md). The Phase 2 round of answers (entries OQ-1 to OQ-34 and the handoff decisions P2-A to P2-J) is appended at the end under "Phase 2 answers (2026-10-13)". The Phase 8 to 11 handoff round (P8-*, P9-*, P10-*, P11-* and MOD-1 to MOD-11, with decisions D-185 to D-216) follows it under "Phase 8 to 11 handoff questions (answered 2026-10-15)".

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
Decision: correct until Phase 6; schema and token format are fixed by Q56. No action now. (Done in wave 8: `sr_apps::auth::AppsAuth` authenticates both against `bot_tokens` and `oauth2_tokens`; `NoBots`/`NoBearers` are gone.)
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

---

# Phase 2 answers (2026-10-13)

Answers to the 34 entries that were live in [open-questions.md](open-questions.md) after the `integration/phase2` merges (`e6e1f66`), plus the decisions the Phase 2 handoff notes asked for ([feat-p2-lounge-messages](dev/handoff/feat-p2-lounge-messages.md), [feat-p2-lounge-shell](dev/handoff/feat-p2-lounge-shell.md)). Entry numbers `OQ-n` are the numbers that file used; `P2-x` are the handoff decisions. The same rules apply as above: the decisions log wins; Discord parity by default (AGENTS.md §3.3); Rust first (D-150) with doc 03 §4 as the list of allowed non-Rust components; privacy per doc 09; "guild" in code, "server" in copy (doc 14). Adopted answers are recorded as D-157 to D-183 in [01-decisions-log.md](01-decisions-log.md); the implementation follow-ups are in [dev/answers-followups.md](dev/answers-followups.md).

## Summary

| Status | Count |
|---|---|
| DEFAULT | 29 |
| OWNER | 4 |
| NO-CODE | 8 |
| DONE | 3 |

OWNER (recommendation adopted as the default, owner confirms or overrides): OQ-12 (Lounge sans font), OQ-13 (token calibration), OQ-14 (door image), OQ-32 (age-restricted titles shown by default to confirmed adults).
NO-CODE: OQ-8, OQ-10, OQ-11, OQ-15, OQ-16, OQ-17, OQ-18, OQ-19 (and the accepted-as-is notes OQ-20, OQ-22, OQ-23, OQ-24, counted under DEFAULT because nothing is pending).
DONE: OQ-7 (local run), OQ-9, OQ-21.

## Owner confirmations (OQ-1 to OQ-4, OQ-25 to OQ-28)

**OQ-1 (Q46). Emergency operator invites.**
Decision: adopted as implemented. `sr-cli invite create` consumes no credit and writes no `invite_credits_ledger` row; the invite is flagged emergency (migration 0090) so redemption spends nothing; a `platform_audit_log` row `invite.emergency_create` is written with actor = system user 1 and target = the recorded issuer (the named account or the oldest Staff Server owner). No second audit trail and no credit requirement.
Rationale: D-024 governs the per-user credit economy, not operator actions; shell access on the host is the authorisation (D-131); the issuer stored on the invite keeps the invite graph complete (D-023); `platform_audit_log` is the operator trail doc 04 §7 prescribes.
Status: `DEFAULT` (D-157)

**OQ-2 (Q133). Guild audit-log retention.**
Decision: 45 days for `audit_log_entries`, from the platform setting `moderation.audit_retention_days` (`0` = forever), purged by the existing `audit.purge` job (`crates/sr-server/src/jobs.rs`). `platform_audit_log` and `staff_access_log` are kept forever. The setting is shown in Admin → Limits.
Rationale: Discord keeps 45 days; doc 09 §1 is explicit only for the staff log (forever) and silent for guild logs; an operator-configurable number needs no owner input.
Status: `DEFAULT` (D-158)

**OQ-3 (Q136). AutoMod preset word lists.**
Decision: SIDE ROOM ships no built-in word lists. `KeywordPreset` triggers resolve against operator-curated lists edited in Admin → Features → AutoMod (`PresetLists`); a preset with an empty operator list matches nothing and the rule editor says so. If the owner ever wants shipped lists, that is a separate, opt-in decision with its own licence review; nothing blocks on it.
Rationale: content lists are an editorial and legal stance; the empty default is the only one that commits the project to nothing. D-041 (AutoMod parity) is met by the engine, not by the lists.
Status: `DEFAULT` (D-159)

**OQ-4 (Q161). Hidden channels in the channel list.**
Decision: `GET /guilds/{id}/channels` keeps returning only channels the caller can `VIEW_CHANNEL`; owners, `ADMINISTRATOR` and bots with `ADMINISTRATOR` see everything. Add the difference to the porting checklist in doc 05 §7 (new item 5: "channel listings are permission-filtered; a bot needs `VIEW_CHANNEL` or `ADMINISTRATOR` to see a channel").
Rationale: AGENTS.md §5 (the server is the security boundary); D-125 (private content is private); the stricter behaviour cannot break a bot that has the permissions it needs, so parity loss is limited to enumeration.
Status: `DEFAULT` (D-160)

**OQ-25. Webhook URLs cannot be copied later.**
Decision: adopted: tokens stay SHA-256 hashes; `GET /webhooks/{id}` never returns the token; `POST /webhooks/{id}/reset-token` issues a fresh URL (shown once) and kills the old one, with an audit entry without the value. No `token_sealed` column. The web Integrations tab shows the URL once at creation and after a reset, with "Reset URL" where Discord has "Copy URL".
Rationale: doc 09 §2 ("tokens are stored as SHA-256 hashes") settles it; a sealed column would make a database dump plus the server key able to post as any webhook (D-125 spirit). The only loss is convenience.
Status: `DEFAULT` (D-164)

**OQ-26. Global default notification level outranks the server default.**
Decision: adopted: channel → category → the user's per-server choice → the user's global default (`/users/@me/notification-defaults`) → the server owner's `default_message_notifications`. Servers only; DMs keep their own scope.
Rationale: an explicit choice by the member beats a suggestion by the server owner; the global default is a SIDE ROOM addition (Discord has none), so no parity rule is broken; the server default still applies to everyone who never set one.
Status: `DEFAULT` (D-165)

**OQ-27. Prune counts messages only.**
Decision: add last-seen tracking and use it. `accounts.last_seen_at` is updated by the gateway on IDENTIFY/RESUME and at most once per 10 minutes per session while connected (also on REST login). Prune "inactive for N days" then means: joined more than N days ago, `last_seen_at` older than N days (or null), no message in the guild within N days, and only roles in `include_roles`. Until the column lands the messages-only rule stays and the prune dialog copy says "members who have not sent a message in N days".
Rationale: Discord prunes on last-online, so voice-only and read-only members are not kicked; a single coarse timestamp is cheap, not sensitive (it is never exposed in user objects; it only shapes a count that `KICK_MEMBERS` holders can already approximate), and keeps doc 09 §1 unchanged.
Status: `DEFAULT` (D-166)

**OQ-28. Email change is confirmed by link only.**
Decision: when the account has any second factor enabled, `POST /sr/auth/email/change` also requires a fresh second-factor proof: the request carries `mfa: {method, code}` (TOTP, recovery code or, when they exist, a passkey assertion), or a 2FA re-prompt satisfied within the last 15 minutes (same window as doc 07 §5). Password re-auth and the link to the new address stay; the old address keeps getting the notice after the swap, now with a "This wasn't me" link that invalidates the pending change for 24 hours while it is pending and revokes all sessions after it completed.
Rationale: Discord asks for the 2FA code before changing the email of a 2FA account; the email address is the recovery root, so a stolen session plus the password must not be enough to move it (doc 06 §4).
Status: `DEFAULT` (D-167)

## Decisions needed before building (OQ-5, OQ-6)

**OQ-5. Passkeys need an ADR.**
Decision: (b) first, (a) as the fallback, in that order, inside ENT-AUTH-010:
1. A time-boxed spike (two days) on pure-Rust relying-party code: `webauthn-rp` (pure-Rust RP crate) and the `passkey-types` family for CBOR/COSE parsing, verifying registration with `none` attestation and assertions for ES256, EdDSA and RS256 with RustCrypto (`p256`, `ed25519-dalek`, `rsa`), tested against the spec test vectors and Chrome's virtual authenticator in Playwright (Q83). SIDE ROOM never verifies attestation statements (it does not gate on authenticator models), which removes most of what pulls OpenSSL into `webauthn-rs`.
2. If no maintained crate passes, write ADR-0009 "webauthn-rs with openssl" accepting the C dependency (`openssl` with `vendored` so the musl release build stays static, Q109), and add the row to doc 03 §4. The owner approves at PR review, which is how every ADR is approved.
Until one of the two lands, passkeys stay deferred and the Entrance keeps hiding the passkey buttons behind the `methods` array (Q83). Fix doc 03 §2: the `webauthn-rs` row is "⚠️ C (openssl)" today.
Rationale: D-150 ("if a good Rust implementation exists, prefer it") requires the evaluation before the exception; doc 03 §4 requires the ADR for the exception; D-028 keeps passkeys in scope, so (c) is not an end state.
Status: `DEFAULT` (D-161)

**OQ-6. Default staff role permission bits.**
Decision: adopt the table in doc 07 §4 (migration 0130 `staff_role_templates`, `sr_admin::rules::default_staff_roles`) as the default: Support 10241, App Reviewer 769, Library Curator 85, Media Curator 203, Moderator 7169, Senior Staff 1818623, Owner bit 63, with the grant rules under it (only the Owner grants `READ_PRIVATE_CONTENT`, `APPOINT_STAFF`, `VIEW_ACCESS_LOG`, `OPERATOR`; a delegate grants only bits they hold; the Owner role is never assignable). Remove the "owner to confirm" sentence from doc 07 §4 and tick the bootstrap item once `bootstrap` creates the roles.
Rationale: the bits follow doc 07 §3's own grant descriptions; the roles are ordinary editable roles after bootstrap (doc 07 §4), so the owner can reshape them in the product without a migration; nothing in the table reaches past D-132.
Status: `DEFAULT` (D-162)

## Needs a database run (OQ-7)

**OQ-7. The real Postgres has never run any SQL.**
Decision: resolved locally: the settings-gaps and library-core branches ran every `#[ignore]` test against a local Postgres 17 (59 green on a fresh database, plus the 10 `library_repo` tests). What remains is CI: `rust.yml` already has a `postgres:17` service container but only runs `cargo nextest run --workspace`; add `SR_TEST_DATABASE_URL` and a second step `cargo nextest run --workspace --run-ignored only -j1`. `admin_repo::suspend_revokes_sessions_and_reactivation_respects_the_cap` must create its own scratch database (or set the cap relative to the current count) so it passes on a reused database.
Rationale: AGENTS.md §4 (PRs must pass tests); the service container exists, so the cost is one step.
Status: `DONE` locally, `DEFAULT` for CI (D-163)

## Library catalog decisions (OQ-29 to OQ-34)

**OQ-29. WebP is lossless only.**
Decision: (b), and no new ADR is needed: ADR-0005 already accepts libwebp ("Lossy WebP encoding via `webp` (libwebp bindings), because pure-Rust WebP encoders are lossless-only") and doc 03 §4 lists it. Encode pages with the `webp` crate at quality 85; for PNG sources also encode lossless and keep whichever is smaller (the ingest spec rule); JPEG/WebP/GIF sources are lossy only. The 720/1080/1600 variants follow the same rule. Lossless pages already stored are left as they are (they are valid WebP); the next re-ingest replaces them.
Rationale: ingest-naming-conventions §2.1 and LIB-ING-001 ask for q85; D-084 fixes the format; ADR-0005 fixes the library; `cargo deny` already allows BSD-3 (libwebp).
Status: `DEFAULT` (D-168)

**OQ-30. AVIF pages are refused.**
Decision: keep AVIF out for now. Archives with `.avif` pages are rejected at ingest with a message that names the files ("convert to JPEG, PNG or WebP"), instead of being silently skipped; the naming spec and LIB-ING-001 drop `avif` from the accepted list. Revisit through a short evaluation of `rav1d` (the Rust port of dav1d) once it exposes a stable still-image decode path; `dav1d` (C) is not on the doc 03 §4 list and would need an ADR nobody has asked for.
Rationale: D-084 names JPEG, PNG and WebP as the inbound formats; a skipped page corrupts a chapter silently, a refusal does not; D-150.
Status: `DEFAULT` (D-169)

**OQ-31. RAR/CBR and 7z/CB7 are not read.**
Decision: (b) and (c) without a new ADR: ADR-0005 and doc 03 §4 already accept `unrar` bindings for RAR. Add 7z/CB7 through `sevenz-rust2` (pure Rust) behind the same `ArchiveLimits` as ZIP (entry count, per-entry size, total budget, no nested archives, path traversal rejected, solid-block size capped). Add RAR/CBR through the `unrar` crate behind the same limits, gated by the platform setting `library.ingest.rar_enabled` (default on) so an operator can switch the one C parser off; extraction runs inside the ingest job, never in a request handler. Uploaders of other formats are asked to convert. The unrar licence forbids building a RAR *creator* from its source; reading is what every open-source reader does.
Rationale: ingest-naming-conventions §2.2 lists `.cbr/.rar/.cb7/.7z`; ADR-0005 is the accepted exception; limits reuse the hardened ZIP design (LIB-ING-002).
Status: `DEFAULT` (D-170)

**OQ-32. Age-restricted series need an opt-in even for adults.**
Decision (recommended): show age-restricted series by default to accounts with `age_confirmed_at`: the default `AgeFilter` for confirmed adults becomes `include`, with `hide` and `only` as the filter options; the browse filter chip is "Age-restricted: shown / hidden / only". Unconfirmed accounts keep `hide` and 404 everywhere, unchanged. A user setting `library.show_age_restricted` (settings blob, default on) and the platform setting `library.age_restricted_default_visible` (default true) let a member or the operator flip the default without code.
Rationale: D-032 is a one-time dialog whose answer is "remembered permanently"; asking confirmed adults to opt in on every list contradicts that. Discord parity: after the age gate, NSFW channels simply appear. The activity-privacy toggle "Hide age-restricted titles from my activity" (doc 09 §3) is separate and stays default on. This is the one entry here that touches taste and legal exposure, so the owner confirms it; the setting makes the override free.
Status: `OWNER` (recommendation adopted as D-171)

Owner override (2026-10-13): the owner replaced this recommendation with D-184. Everyone sees age-restricted library titles in browse, search, detail, feeds and follows, unconfirmed accounts included; nothing is hidden or 404'd. Only opening the pages of a restricted series needs age confirmation: the API answers 403 `AGE_CONFIRMATION_REQUIRED` (code 900011), the client shows the Lounge's "Are you 18 or older?" dialog, `POST /sr/age-gate` records the yes permanently (D-032), and the retry succeeds. `AgeFilter` defaults to `include`. An optional user setting `library.hide_age_restricted` (default false) hides restricted titles for members who want that. The platform setting `library.age_restricted_default_visible` and the user setting `library.show_age_restricted` are dropped.

**OQ-33. Publisher permissions.**
Decision: confirmed as implemented: `PUBLISH_READING` for create, edit, publish and page changes; `DELETE_CONTENT` for delete (series, edition, chapter, volume); merge needs both; unpublish accepts either. A Library Curator (`PUBLISH_READING`, no `DELETE_CONTENT`) can unpublish but not delete, which is the intended split: unpublish is reversible, delete is not.
Rationale: doc 07 §3 wording; the role table of OQ-6; D-121's spirit (destructive actions sit with higher-level staff).
Status: `DEFAULT` (D-172)

**OQ-34. Browse sorts.**
Decision: confirmed: the API accepts `updated`, `newest`, `title`, `followers` and returns 400 for anything else; the Library UI offers only those four ("Recently updated", "Newest", "A–Z", "Most followed"). `rating` and `trending` are added by LIB-SOC-001 and LIB-SOC-004 and are not aliased to `followers` in the meantime.
Rationale: an honest stand-in beats a mislabelled one; the spec's sort list (library-reader §3.2) is reached when the social items land.
Status: `DEFAULT` (D-173)

## Needs a human outside the code (OQ-8 to OQ-16)

**OQ-8 (Q8). Permission fixtures are hand-derived.**
Decision: unchanged; captured in Phase 6 by `crates/sr-compat-tests`. Removed from the live list and tracked as a follow-up.
Status: `NO-CODE`

**OQ-9 (Q26). Presigned URLs untested against Garage.**
Decision: resolved: `.github/workflows/storage.yml` runs `cargo test -p sr-storage --test s3_live -- --ignored` against Garage from `deploy/compose/dev.yml` ([docs/dev/storage-live-test.md](dev/storage-live-test.md)).
Status: `DONE`

**OQ-10 (Q20, Q21). Pinned ops versions and bootstrap flags.**
Decision: the tags in `deploy/compose/dev.yml` (`postgres:17`, `dxflrs/garage:v1.0.1`, `axllent/mailpit:v1.21`, `caddy:2`, `coturn/coturn:4.6`) are validated by `storage.yml` and `e2e.yml`, which start them in CI. `deploy/compose/README.md` does not exist yet: a follow-up creates it with the table of pinned tags and the `bootstrap.sh` flag check.
Status: `NO-CODE` (follow-up for the README)

**OQ-11 (Q19). pnpm is not installed on the agent machines.**
Decision: unchanged (`corepack enable && corepack prepare pnpm@10.12.1 --activate`); until then agents keep using `npx pnpm@10.12.1` with `npm_config_manage_package_manager_versions=false`, never npm, for lockfile changes. Stays in the live list as a human action.
Status: `NO-CODE`

**OQ-12 (Q33). Font files are missing.**
Decision (recommended): the Side Room families are fixed by doc 10 §2 (Newsreader, Hanken Grotesk, IBM Plex Mono, all OFL). For the Lounge sans, use **Inter** (OFL) as the gg sans stand-in: closest metrics to Discord's UI at 14–16 px, variable WOFF2, well-hinted. The owner picks or confirms the family and drops the WOFF2 files plus `OFL.txt` under `apps/web/public/fonts/<family>/`; `fonts.ts` already expects the paths. Downloading fonts is an owner action under the working agreements.
Status: `OWNER` (asset and taste)

**OQ-13 (Q34). Token values are not calibrated against the mockups.**
Decision: unchanged: the owner (or someone with the mockups open) does the 1440/1704/<900 px pass; adjustments land as token-only PRs. Recommendation: do it after OQ-12, since type metrics move the spacing.
Status: `OWNER` (visual judgement)

**OQ-14 (Q81). The door image is missing.**
Decision: unchanged: the owner supplies `apps/web/public/entrance/door.jpg` (recommended ≥ 1704 px wide, ≤ 400 KiB, no EXIF); the Admin → Entrance upload replaces the file later. Until then `Door.tsx` keeps its gradient placeholder.
Status: `OWNER` (asset)

**OQ-15 (Q85). Markdown corpus has not been reviewed against Discord.**
Decision: unchanged: a human with a Discord account reviews a stratified 100-case sample after Q31/Q89/Q90 are in; mismatches become hand-written expectations. Stays in the live list.
Status: `NO-CODE`

**OQ-16 (Q108). The CI workflows have not had a green run.**
Decision: unchanged: `release.yml`, `openapi.yml`, `storage.yml` and `e2e.yml` need a first green run on GitHub; FND-CI-004 stays unticked until `e2e.yml` passes. Pushing is the owner's call (working agreements). Stays in the live list.
Status: `NO-CODE`

## Informational (OQ-17 to OQ-24)

**OQ-17 (Q102). Bot and OAuth bearer backends reject every token.** Correct until Phase 6 (Q56). Follow-up recorded under Phase 6. Status: `NO-CODE`
**OQ-18 (Q122). Browser zstd decoding is deferred.** Revisit trigger made concrete: when the p95 gateway payload exceeds 64 KiB or mobile users report data use; then `ruzstd` in `packages/wasm`. Status: `NO-CODE`
**OQ-19 (Q124). Member lists use REST, not op 102.** Arrives with the member-list inventory item (lazy ranges); no change. Status: `NO-CODE`
**OQ-20 (Q126). Fresh READY clears message windows; RESUME refetches nothing.** Accepted, parity. Status: `DEFAULT`
**OQ-21 (Q127). No `around` window.** Resolved: `fetchMessagesAround` and `loadAroundMessage` exist (`apps/web/src/store/{rest,store}.ts`); "load newer" from a jumped-to window is LNG-MSGUI-001's remaining half. Status: `DONE`
**OQ-22 (Q130). Search filters take ids, not names.** Accepted; negation (`-from:`) is a later item. Status: `DEFAULT`
**OQ-23 (Q69). `*-wasm` crates build natively as well.** Accepted; `sr-markdown-wasm` has its `wasm_bindgen_test`; the other two get one in the wasm follow-up. Status: `DEFAULT`
**OQ-24 (Q168). Placeholder UI looks real.** Tracked per item in inventory 03-lounge-shell; the open-questions entry is retired. Status: `DEFAULT`

## Decisions from the Phase 2 handoff notes

**P2-A. Rich editor (Lexical) vs textarea for the composer (LNG-CMP-001).**
Decision: the textarea stays through Phase 2. Mentions, channels, emoji and timestamps are readable text mapped to wire tokens at send (the current design), which meets every acceptance criterion in lounge-chat §6.4 except inline token rendering. LNG-CMP-001 is re-scoped to "rich composer on **Lexical**" and scheduled after the LNG-MSGUI gaps, so the autocomplete, slash-command and upload code is not rewritten twice. Lexical over Slate: maintained by Meta, React-first, better accessibility story, MIT.
Rationale: the spec allows either library; parity of function before parity of feel; no backend impact.
Status: `DEFAULT` (D-174)

**P2-B. Presigned PUT uploads: bucket CORS and the Content-Type header.**
Decision: uploads stay presigned PUTs to Garage (doc 02 §4.5). Two fixes: (1) the bucket gets a CORS rule (AllowedOrigins = `server.public_url`, methods `PUT, GET, HEAD`, headers `Content-Type`, expose `ETag`, max age 3600) applied by a new `sr-cli storage ensure-cors` command that sends S3 `PutBucketCors` with the SigV4 signer planned for multipart (Q23); `deploy/compose/garage/bootstrap.sh` and `deploy/INSTALL.md` call it. (2) The presigner (`object_store` `signed_url`) signs method and path only, so the header is not part of the signature; the client keeps sending `Content-Type`, the server stores it as `declared_content_type` and sniffs the real type on finalize (unchanged). No upload proxy through `sr-server`.
Rationale: doc 02 §4.5 and Q153 chose presigned PUT; a CORS rule is the missing deployment step, not a design change; D-150 keeps the tooling in Rust.
Status: `DEFAULT` (D-175)

**P2-C. Remote embed images and the missing image proxy (LNG-MSG-014).**
Decision: the client never loads a third-party URL into an `<img>`. Until the proxy exists, embeds render their text parts (title, description, provider, fields) and show "Load image" per embed, which fetches with `referrerPolicy="no-referrer"` only on click; the user setting "Load remote images automatically" is off and hidden until the proxy ships. The proxy (LNG-MSG-014) serves `/cdn/external/{hmac}/{url-b64}`: SSRF-safe fetch, 10 MiB and 10 s limits, re-encoded to WebP/JPEG with EXIF stripped, cached in Garage under `external/` for 30 days, operator allow/deny lists. Once it exists, the unfurler fills `proxy_url` on `embed.image`, `embed.thumbnail`, `embed.video` and author/footer icons, and the client loads `proxy_url` only. Avatars, stickers and attachments are first-party already.
Rationale: Discord proxies every embed image (`media.discordapp.net`), so direct loads are both a parity gap and a privacy leak of viewer IPs; AGENTS.md §3.5's "never phones home" extends to members' browsers; doc 09 §3.
Status: `DEFAULT` (D-176)

**P2-D. Spoiler attachments.**
Decision: Discord parity: a spoiler attachment is one whose filename starts with `SPOILER_`. The upload-slot `spoiler: true` field is kept as the ergonomic input; the server prefixes the stored filename (once) when it is set, so `AttachmentObject.filename` carries the marker and no extra field is needed. The client keeps reading the prefix; the `spoiler` column stays as the query-friendly mirror.
Rationale: Discord's attachment object has no spoiler flag; bots and clients ported from Discord expect the prefix (D-060).
Status: `DEFAULT` (D-177)

**P2-E. Pin permission ignores channel overwrites.**
Decision: the client computes **channel-level** permissions with the shared `sr-permissions-wasm` `compute` (roles, channel overwrites, timeout rule, Q6/Q7) and stores them per channel (`channel.permissions`), replacing the base-guild `GuildPermissions` for every per-channel capability: pin, delete others' messages, manage, send, attach, embed, mention everyone, bypass slowmode. Pin = `PIN_MESSAGES` (Q95); `MANAGE_MESSAGES` no longer implies it in the client once the one-time grant migration (0061) is the source of truth. The server already enforces per channel; this removes wrong greying-out.
Rationale: AGENTS.md §3.2 (the same permission computation on both sides, from one Rust crate); AGENTS.md §5.
Status: `DEFAULT` (D-178)

**P2-F. Where rail order and folders persist (LNG-RAIL-007/008).**
Decision: in the user settings blob (`GET/PATCH /users/@me/settings`, migration 0140) under Discord's own key names and shapes: `guild_positions: [guild_id…]` and `guild_folders: [{id, name, color, guild_ids}]`. The client writes them as a merge patch after a drag ends (debounced 1 s), applies `SR_USER_SETTINGS_UPDATE` from other sessions, and keeps `localStorage` as the offline cache and first paint. The same blob takes `collapsed_categories`, remembered channel per guild and panel width (Q171). No dedicated endpoint.
Rationale: Q171 already pointed here; the blob now exists (settings-gaps gap 3); Discord stores both in user settings, so ported clients find them where they expect.
Status: `DEFAULT` (D-179)

**P2-G. Atomic create-from-template (LNG-GLD-003).**
Decision: the server owns it. `POST /guilds/templates/{code}` (Discord's route, `{name, icon?}`) instantiates a template in one transaction using `sr_chat::settings::templates::instantiate` (Q138), and `POST /guilds` accepts `template: "<built-in id>"`. The eight built-in templates (Gaming, Friends, Study Group, School Club, Local Community, Artists & Creators, Watch Club, Reading Club) move from `apps/web/src/lounge/shell/serverTemplates.ts` to Rust as Discord-compatible template JSON in `crates/sr-chat/src/settings/builtin_templates/`, served by `GET /guilds/templates/builtin` so the wizard lists them from the server. The client's sequential POSTs are deleted once the endpoint exists; partial failure becomes impossible.
Rationale: D-041 (server templates), D-060 (Discord routes); one source of truth for the built-ins (AGENTS.md §3.2).
Status: `DEFAULT` (D-180)

**P2-H. READY should carry full `recipients` for DM channels.**
Decision: yes. `build_ready_extras` (`crates/sr-server/src/gateway_pg.rs`) emits `private_channels[].recipients` as full user objects through the `UserLookup` of Q157, and drops the private `recipient_ids`. The shell's boot-time `GET /users/@me/channels` (`shell/social.ts`) is removed once the gateway change ships; `CHANNEL_CREATE` for DMs carries the same shape.
Rationale: Discord's READY carries `recipients`; one boot round-trip less; D-060.
Status: `DEFAULT` (D-181)

**P2-I. Invite dialog defaults to single use.**
Decision: keep Q134's split. "Copy link" and anything that produces a URL to paste elsewhere creates the invite with `external: true` (single use, 7 days) because it is intended to leave the platform (D-022). "Invite friends" (a DM to a member) uses the server's defaults (7 days, unlimited uses) because it never leaves the platform; the dialog's expiry and use-count pickers edit either. The dialog copy says "This link can be used once" on the external one.
Rationale: D-022 is about links that leave the platform; in-platform recipients are already accountable members (D-020), so a per-recipient single-use code adds friction without a security gain.
Status: `DEFAULT` (D-182)

**P2-J. Friends use REST polling; no `RELATIONSHIP_*` gateway events.**
Decision: the backend already dispatches `RELATIONSHIP_ADD/UPDATE/REMOVE` as `DomainEvent::UserScoped` to each addressed user's topic (`crates/sr-api/src/routes/social/relationships.rs`, `events.rs`); the gap is the client. The reducer applies the three dispatches (and `CHANNEL_CREATE/DELETE` for DMs) to the social store; the Friends page stops re-fetching after each action and only loads on boot. Tick LNG-FRD-003 after the reducer test proves another session's request appears live.
Rationale: parity (Discord updates friends live); the events exist, so polling is duplicate work.
Status: `DEFAULT` (D-183)

## Owner confirmations (2026-10-13, later)

- **OQ-12 (Lounge sans font): confirmed.** Inter. The fonts (Newsreader, Hanken Grotesk, IBM Plex Mono, Inter; Latin subset WOFF2 from the Fontsource npm packages 5.3.0, OFL licences beside them) are installed under `apps/web/public/fonts/` and registered in `apps/web/src/styles/fonts.css`; the owner approved the download.
- **OQ-14 (door image): supplied.** `apps/web/public/entrance/door.jpg` is the owner-provided photograph, converted to JPEG (1536x1024, 184 KB, no EXIF). It is below the recommended 1704 px width; replace it with a wider master when one exists. `entrance.css` now shows it and hides the CSS stand-in.
- **OQ-13 (token calibration): agreed**, to be done now that the fonts have landed.

- **OQ-13 (token calibration): done for the built screens (2026-10-13).** The entrance landing and login pages were compared with `entrance-landing.jpg` and `entrance-login.jpg` at 1704 px. Colour tokens already matched the sampled mockup pixels within a few levels (page `#121210` vs `--sr-bg-0` `#111110`, panel `#161614` vs `#161613`, gold button `#c0b08d` vs `#c2b08f`), and layout metrics matched within a few pixels (header 159 px, hero 1030 px, headline 102.4 px, tagline 18 px), so no token values changed. Changes: Newsreader now ships the optical-size (opsz) variable files, so the headline uses the display cut; the door photograph is scaled and positioned to the mockup crop with a left fade instead of a hard panel edge; the login door panel has its own crop; wide screens (min-width 1200 px) get the mockup form sizes (76 px fields, 78 px buttons, sentence-case 14 px labels) scoped to `.ent-form`. Not checked: below 900 px, Cinema, Library and Lounge (no UI or no committed mockups yet), and the wordmark weight.

## Phase 8 to 11 handoff questions (answered 2026-10-15)

Every open owner question from the Phase 8 to 11 handoffs (`docs/dev/handoff/feat-p8-*.md`, `feat-p9-*.md`, `feat-p10-*.md`, `feat-p11-*.md`), the gaps list in `feat-p11-integration.md`, and the eleven questions in `docs/specs/moderation-compliance.md` section 14, de-duplicated. Rules applied as before: [01-decisions-log.md](01-decisions-log.md) wins; Discord parity by default (AGENTS.md section 3.3); privacy and safety per docs 09 and 15; Rust first per doc 03 (a C binding not in doc 03 section 4 needs an ADR); conservative security defaults; the production context is one Owner running a friend-group instance on one rented box (doc 00 section 2). Earlier handoffs (Phase 2 to 7) contain no unanswered explicit questions; their "Decisions" sections are statements that stand as built.

Status key for this round: `ADOPTED` = a product decision, recorded as D-185 onward; `DEFAULT` = engineering default, implement without the owner; `NEEDS-OWNER` = needs the human (legal, money, artwork, external accounts or keys), with the default that applies until they answer; `NO-CODE` = a human action or informational. The implementation work is listed in [dev/answers-followups-p12.md](dev/answers-followups-p12.md) (items `G-nn`).

| Status | Count |
|---|---|
| ADOPTED | 32 |
| DEFAULT | 58 |
| NEEDS-OWNER | 9 |
| NO-CODE | 4 |

103 questions in all (P10-ADM-7 is folded into P11-CLI-2). The NEEDS-OWNER items are P8-DON-4, P11-MSG-1, P11-SHL-3, P11-CLI-1, P11-USB-2, P11-LIB-2, MOD-4, MOD-5 and MOD-6; each states the default that applies until the owner answers.

### Developer platform (P8-DEV, P11-DEV)

**P8-DEV-1. Should an unreleased application be installable by testers other than its owner (`app_testers`)?**
Decision: yes. Doc 11 section 2.2 lists an App Testers tab, so this is parity, not an extension. `app_testers (application_id, account_id, state invited|accepted, invited_by, created_at)`, at most 25 testers per application; the owner invites by username and the tester accepts from a system DM (the same offer pattern as staff appointments). A `development` application may then be installed into a guild where the owner **or an accepted tester** holds `MANAGE_GUILD`, still only while the guild has at most 25 members (doc 11 section 2.1, error `900120`). Testers can use the app's commands anywhere it is installed; they get no portal access.
Status: `ADOPTED` (D-204). Follow-up: G-01.

**P8-DEV-2. Is the install permission cap (bot role permissions = requested AND the installer's own) too strict?**
Decision: keep it. A `MANAGE_GUILD` holder who lacks `ADMINISTRATOR` must not be able to mint an administrator bot; the cap is the "no escalation" rule of doc 15 T7 and the install dialog already explains which requested permissions were dropped. Difference from Discord recorded in the porting notes (doc 05 section 7).
Status: `ADOPTED` (D-205). Follow-up: G-02 (docs only).

**P8-DEV-3. Should interaction responses be allowed without the bot's `SEND_MESSAGES`, like Discord's token-based responses?**
Decision: no for channel-visible responses, yes for ephemeral ones. A non-ephemeral response is a message in the channel and obeys the channel's permissions (a bot muted in `#announcements` stays mute); an ephemeral response only reaches the invoker and is always allowed, so a bot can still explain itself. Deferred responses follow the same rule when they resolve. Difference from Discord recorded in doc 05 section 7.
Status: `ADOPTED` (D-206). Follow-up: G-03.

**P8-DEV-4. Privileged intents in `development` are developer-toggled without review; review everywhere?**
Decision: keep as built. In `development` the 25-member test-guild rule is the guard and the data at stake is the developer's own test servers; a release version that requests a privileged intent goes through version review (doc 11 section 2.1), which is where the review belongs. Doc 05 is clarified to say "reviewed at release".
Status: `DEFAULT`. Follow-up: G-02 (docs only).

**P8-DEV-5. Bearer tokens 7 days, refresh 90 days, codes 5 minutes, 25 applications per account: confirm?**
Decision: confirmed. 7-day access tokens match Discord; 90-day refresh with rotation and reuse detection is the conservative end; 5-minute codes are enough for a redirect round trip; 25 applications is generous for one person. All four are constants in `sr-apps` and can move into `platform_settings` if an operator ever asks.
Status: `DEFAULT`. No follow-up.

**P8-DEV-6. Application icons are served from the bot's avatar path; dedicated CDN kind?**
Decision: keep the bot avatar path. Every application has exactly one bot account, the avatar pipeline already does the sniffing, resizing and hashing, and Discord clients read `application.icon` as a hash under `/app-icons/`, which the client maps; a second kind would duplicate the storage code for no user-visible gain.
Status: `DEFAULT`. No follow-up.

**P8-DEV-7. The `rustls` advisories (RUSTSEC-2026-0346 to 0349) need a lock bump.**
Decision: resolved on the integration branch: `Cargo.lock` carries `rustls 0.23.46`. The next CI run of `cargo deny check advisories` confirms it; nothing else to decide.
Status: `NO-CODE`.

**P8-DEV-8. Should a signed-in session read and manage an application's commands in the portal (the web agent built a builder instead)?**
Decision: no. Discord's portal has no command manager either; commands are managed with the bot token through the API, and `sr-cli`/the SDK are the tooling. The builder stays and gains "copy as curl" only.
Status: `DEFAULT`. No follow-up.

**P11-DEV-1. The interactions-endpoint signing seed is stored unencrypted in the database.**
Decision: seal it. `applications.interactions_signing_seed` is encrypted with `server.secret_key` the way TOTP secrets and the VAPID private key are (doc 15 T11); the public key stays derivable, a migration re-encrypts existing rows, and `sr-db/tests/secret_columns.rs` lists the column as sealed, not `PLAINTEXT_OK`. Deriving from the server key alone was rejected because rotating `server.secret_key` would silently change every application's public key.
Status: `ADOPTED` (D-185). Follow-up: G-04.

**P11-DEV-2. Endpoint URLs must be https on public addresses only (no private-network opt-in). Confirm?**
Decision: confirmed. Interaction deliveries are outbound requests to URLs that members supply, so they go through `sr_net::SafeClient` with its private-range policy (doc 15 T8). Developers who run their app on a LAN use a tunnel, as with Discord. The operator opt-in that payments has (`SR_PAYMENTS_ALLOW_PRIVATE_HOSTS`) is not extended to app endpoints.
Status: `DEFAULT`. No follow-up.

**P11-DEV-3. No auto-disable of an endpoint after repeated failures (Discord disables).**
Decision: parity. After 25 consecutive failed deliveries (timeout, non-2xx, bad signature echo) within 10 minutes the endpoint is marked `disabled`, interactions fall back to gateway delivery, the application owner gets a system DM, and the portal card shows "Disabled after repeated failures" with a "Verify and re-enable" button that runs the PING check. Success resets the counter.
Status: `ADOPTED` (D-186). Follow-up: G-05.

**P11-DEV-4. Typed `name:value` option syntax plus a form, where Discord has chip-style inputs.**
Decision: keep until the Lexical composer. Chip inputs are inline tokens, which is exactly what LNG-CMP-001 (D-174) adds; building chips in the textarea now would be rewritten. The form stays as the accessible path afterwards.
Status: `DEFAULT`. No follow-up (part of LNG-CMP-001).

### Donations (P8-DON)

**P8-DON-1. Accept `MITNFA` for `hex_lit` and move to BDK later, or keep the in-house derivation?**
Decision: keep the in-house derivation on `secp256k1`, `bitcoin_hashes` and `bech32`. It is verified against the BIP-84 and BIP-380 vectors, pulls about 270 fewer crates (doc 15 T14) and needs no licence exception. ADR-0006 named `bdk_wallet`; it gets an addendum recording the substitution and the trigger for revisiting (BDK dropping `hex_lit`, or a need for descriptors beyond `wpkh`). `secp256k1-sys` compiles C the way `libwebp-sys` does and is covered by the same reasoning as ADR-0005 (a vetted reference implementation of a format, orchestrated from Rust).
Status: `ADOPTED` (D-207). Follow-up: G-06 (docs).

**P8-DON-2. Is the 60-minute price risk acceptable, or should the dialog require a refreshed quote after 30 minutes?**
Decision: accept the 60-minute lock. At the configured amounts ($5 to $50) the exposure is cents, and a stable BIP21 QR for the whole invoice life is worth more than a re-quote step that confuses payers mid-payment. Both numbers stay operator-editable in `DonationConfig`; an operator who sells many $20 invite credits can shorten the invoice life instead.
Status: `DEFAULT`. No follow-up.

**P8-DON-3. When to turn off `SR_DOWNLOADS_LEGACY_FALLBACK` and drop `cinema_download_grants`?**
Decision: now. Migration 0542 copied every unexpired grant; the default flips to `0` in this phase, the flag is removed one phase later, and a forward-only migration drops the table then. The runbook tells an operator upgrading from an earlier build to check Admin → Entitlements first.
Status: `DEFAULT`. Follow-up: G-07.

**P8-DON-4. Which Esplora endpoint and which price sources does the installation use?**
Decision: the owner's. The recommended default: a self-hosted `electrs` (or mempool backend) on the same box, so invoice addresses never leave it (the handoff's privacy note), and two public price APIs with the median rule (doc 08 section 1). Until the owner sets them the Admin → Donations page shows the sources as "not configured" and invoices cannot be created.
Status: `NEEDS-OWNER` (external services). No code follow-up.

**P8-DON-5. Should the Cinema and Library footers link the leaderboards before contributor boards exist?**
Decision: build the contributor boards first, then link. `contribution_credits` now exists (REV-CORE-006, Phase 11), so the three boards from doc 08 section 4 (movies, episodes, chapters) can be computed; the footer links (DON-LB-003) land in the same change.
Status: `DEFAULT`. Follow-up: G-08.

**P8-DON-6. The eleven build decisions in the donations handoff (descriptor forms, tolerance, anonymity, `boost_level`, config row, reorg revert, routes, descriptor storage, Esplora paging).**
Decision: confirmed as built. One note: decision 10 (the descriptor is readable by anyone with database access) is already covered by doc 15 T11 (disk encryption on the host); the spec sentence is enough.
Status: `DEFAULT`. No follow-up.

### Live TV (P8-TV)

**P8-TV-1. Programme durations come from `runtime_min`, not from the asset.**
Decision: use the asset. When a block's title resolves to a transcoded asset, its `duration_ms` is the block length and `runtime_min` is the fallback for pool filtering only; a stalled tail (block longer than media) must not happen for library content.
Status: `DEFAULT`. Follow-up: G-09.

**P8-TV-2. The guide's Remind me calls an assumed `PUT|DELETE /tv/reminders/{block_id}`.**
Decision: that is the route. A `tv_reminders (account_id, block_id, remind_at)` table; the 15-second scheduler of `sr-userfeat` fires them as a Lounge reminder notification plus Web Push (LNG-NTF-003), five minutes before the programme by default. Reminders for a block that is regenerated away are dropped with a notice.
Status: `DEFAULT`. Follow-up: G-10.

**P8-TV-3. A daypart from 00:00 to 00:00 means "all day" (unverified against the server).**
Decision: yes, by definition: an end at or before the start wraps midnight, so equal times are a full 24 hours. A test pins it and the admin editor says "All day" for that case.
Status: `DEFAULT`. Follow-up: G-11.

**P8-TV-4. Time zones come from a built-in table of 40 zones with hand-coded DST rules.**
Decision: replace with `jiff` (pure Rust, bundles the IANA database, handles historical rule changes). The table was a stop-gap because nothing could be downloaded; a crate added through Cargo is normal dependency work. `GET /tv/timezones` then lists every IANA zone.
Status: `DEFAULT`. Follow-up: G-12.

### Watch parties (P8-WP)

**P8-WP-1. Should there be a dedicated "Watch Party" channel type (WP-BE-009)?**
Decision: no new channel type. A persistent party bound to a voice channel already behaves as a watch-party channel (survives being empty, first joiner hosts); a channel type would touch the permission pipeline, the channel editor and every ported bot's channel-type table for a cosmetic gain. The channel editor gets a "Watch party room" preset that creates a voice channel with a persistent party, which is the user-facing feature the spec wanted.
Status: `ADOPTED` (D-208). Follow-up: G-13.

**P8-WP-2. Concurrent party limits are a constant 2 until server levels exist.**
Decision: server levels exist now (Phase 8 donations). `GuildLimits` is implemented over `sr_payments::Entitlements` with doc 08 section 2.2's numbers (2/4/8/16), invalidated on `SR_ENTITLEMENT_UPDATE`.
Status: `DEFAULT`. Follow-up: G-14.

**P8-WP-3. The drift ignore band is 100 ms instead of the spec's 250 ms.**
Decision: accepted; the simulation showed 250 ms misses the 300 ms acceptance criterion. Spec updated to 100 ms.
Status: `DEFAULT`. Follow-up: G-15 (docs).

**P8-WP-4. Clock sync over HTTP; gateway op 100 stays reserved.**
Decision: accepted. Fewer moving parts and testable without a socket; op 100 remains reserved and unused.
Status: `DEFAULT`. No follow-up.

**P8-WP-5. Live TV parties (WP-BE-010) and "Watch together" on a live channel.**
Decision: wire them: `sr-tv` supplies the `PlayGate` for `live` parties (no seek, pause or speed, as tested), the live guide's now panel gets "Watch together", and the Lounge message card too.
Status: `DEFAULT`. Follow-up: G-16.

### Phase 9 cross-cutting (P9)

**P9-I18N-1. The account `locale` has no UI; the web Language preference is separate, and mails follow `accounts.locale`.**
Decision: one setting. The Language picker writes `PATCH /users/@me {locale}` and mirrors it into the settings blob for first paint; mails, push text and system DMs read `accounts.locale`.
Status: `DEFAULT`. Follow-up: G-17.

**P9-PWA-1. Build the push backend, and teach the Share Target about files.**
Decision: the push backend exists since Phase 11 (LNG-NTF-003). Files: the manifest `share_target` becomes a POST form intercepted in `sw.js`, which stores the files in IndexedDB and opens `/share` with them queued as composer attachments for the chosen channel; the GET text path stays.
Status: `DEFAULT`. Follow-up: G-18.

**P9-PWA-2. The app badge counts mentions only; add a dot for plain unread?**
Decision: no. Discord's badge is mentions and DMs; a count-less dot would make the badge mean "anything happened", which is noise for a chat app.
Status: `DEFAULT`. No follow-up.

**P9-PWA-3. Real-device checks (iOS keyboard, Android share sheet, installed-PWA badge).**
Decision: a human with the devices does them after the next deploy; nothing to decide.
Status: `NO-CODE`.

**P9-SEC-1. The route matrix AUTHZ column is heuristic-seeded (160 rows `other`), not hand-verified per handler.**
Decision: accepted as the baseline. Every feature PR that touches a route hand-verifies its rows (the PR template gets a checkbox); a one-off pass over the 160 `other` rows is scheduled as a small item rather than blocking anything.
Status: `DEFAULT`. Follow-up: G-19.

**P9-SEC-2. Nine `/sr/auth/*` routes answered 500 instead of 401 without an auth service.**
Decision: fixed in Phase 10 (`feat-p10-admin-be`, extractor order swapped, `NEEDS_DEPS_FIRST` removed).
Status: `NO-CODE` (done).

**P9-SEC-3. The router-literal scan is textual; routes registered otherwise rely on `#undocumented`.**
Decision: accepted. Feature-nest and cdn routers are few and listed; the test fails loudly when a literal appears outside the list, which is the property we want.
Status: `DEFAULT`. No follow-up.

**P9-SEC-4. Outbound webhooks, RSS and metadata fetchers must use `sr_net::SafeClient`.**
Decision: a standing rule: every outbound HTTP request to an address that is not a fixed first-party constant goes through `sr_net::SafeClient` (unfurl, image proxy, RSS, payments, connections, push, interaction endpoints already do). `cargo deny` bans a direct `reqwest` dependency in every crate except `sr-net` and the two fixed-URL exceptions (Discord OAuth, Tenor).
Status: `ADOPTED` (D-203). Follow-up: G-20.

### Phase 10 admin (P10-ADM, P10-ADF)

**P10-ADM-1. Direct `POST /admin/staff` (appoint without acceptance) is still there, behind re-auth. Remove it, or restrict to the Owner?**
Decision (owner, 2026-10-15, overrides the earlier "remove it" default): keep it, restricted to the Owner (everyone else, including staff with `APPOINT_STAFF`, is refused) and still behind re-auth. Offers stay the default path for everyone; `sr-cli staff appoint` (an operator CLI action with a `platform_audit_log` row, `staff.appoint_cli`) covers bootstrap and emergencies.
Status: `ADOPTED` (D-197, amended). Follow-up: G-21.

**P10-ADM-2. Delegations give `APPOINT_STAFF` with a ceiling; the Senior Staff template also grants it outright. Both?**
Decision: both, as built. The template bit is the D-162 default for the Owner's most trusted role; a delegation is the scoped form for anyone else. The Admin → Staff page explains the difference in one line.
Status: `DEFAULT`. No code follow-up.

**P10-ADM-3. Badge names and bits (`early_member`, `contributor`, `bug_hunter`, `honorary` at `accounts.flags` bits 8 to 11).**
Decision: accepted. Staff cannot touch bits 0/1; `patron`, `staff`, `owner`, `app_developer` and `bot` stay derived.
Status: `DEFAULT`. No follow-up.

**P10-ADM-4. Re-auth is ON by default and locks staff without TOTP or a passkey out of sensitive actions, the Owner included. Keep?**
Decision: keep, and close the gap at the front: accepting a staff appointment offer requires an enrolled second factor (the offer DM says so, the Accept button answers "enrol first"), so nobody holds staff powers without one. `SR_ADMIN_REQUIRE_REAUTH=0` stays a development and e2e switch only.
Status: `ADOPTED` (D-198). Follow-up: G-22.

**P10-ADM-5. `revoked_by` of an appointment closed by the leave trigger is the member themselves.**
Decision: accepted; the feed alert and the guild audit (kick) are the record. The appointment row gets `revoked_reason` (`left`, `kicked`, `revoked`) so the Staff page can say which.
Status: `DEFAULT`. Follow-up: G-23.

**P10-ADM-6. Email alerts go to the Staff Server owner's account email only.**
Decision: accepted for now; `alerts.email_to` (a list, default empty = the Owner) can follow when a second operator exists.
Status: `DEFAULT`. No follow-up.

**P10-ADM-7. Announcement DMs go to everyone at once; no opt-out or schedule.** (Merged with P11-CLI-2.)
Decision: see P11-CLI-2 / D-192.

**P10-ADM-8. ADM-GRAPH-003 WebGL renderer: owner decision.**
Decision: no WebGL. The tree and list views cover the realistic graph (under 100 accounts); when the graph passes about 500 nodes a 2D canvas renderer on the existing `sr-graph-wasm` layout is enough and adds no dependency. ADM-GRAPH-003 is re-scoped to "canvas 2D".
Status: `DEFAULT`. Follow-up: G-24 (inventory).

**P10-ADF-1. Should the Staff Server profile badge be a user-profile badge or only the rail frame?**
Decision: a profile badge, as doc 07 section 4 says ("SIDE ROOM Staff badge on profiles, configurable"). `GET /users/{id}/profile` `badges` includes `staff` (and `owner` for the Owner) computed from the active appointment; the staff member can hide it with the user setting `staff.show_badge` (default on).
Status: `DEFAULT`. Follow-up: G-25.

**P10-ADF-2. Is "Merge series" meant for `PUBLISH_READING` only? The API also requires `DELETE_CONTENT`.**
Decision: already decided: merge needs both (D-172). The catalog UI hides Merge unless the viewer holds both bits.
Status: `DEFAULT`. Follow-up: G-26.

### Phase 10 Lounge servers and channels (P10-LBE, P10-LFE)

**P10-LBE-1. Should the gateway stop dispatching NSFW channel messages to accounts that have not confirmed 18+, or is the REST gate enough?**
Decision: the gateway must gate too. `MESSAGE_CREATE/UPDATE/DELETE`, reactions, typing, pins and thread events from an `nsfw` channel (or a thread of one) are not delivered to a session whose account lacks `age_confirmed_at`; the channel object itself is still delivered (everyone sees that the room exists, D-184's spirit). `GuildDirectory::age_confirmed` already exists for voice. The server is the boundary (AGENTS.md section 5); a REST-only gate leaks content through the socket.
Status: `ADOPTED` (D-199). Follow-up: G-27.

**P10-LBE-2. Is a 60-second offline grace for temporary members right, or immediate like Discord?**
Decision: keep 60 seconds. A PWA reload or a mobile network blink is a disconnect and reconnect, and Discord's instant removal would throw people out of a server for a page refresh. Recorded as a parity exception. A startup sweep removes temporary members whose last session ended before the restart.
Status: `ADOPTED` (D-200). Follow-up: G-28.

**P10-LBE-3. Are the 12 directory categories right, and should staff manage them instead of the built-in list?**
Decision: keep the built-in list (Discord's discovery categories are fixed too). A platform setting `directory.hidden_categories` lets the operator hide ones that make no sense for their instance; editing names waits for a request.
Status: `DEFAULT`. Follow-up: G-29.

**P10-LFE-1. Should a channel-follow API (announcement Follow) and a guild "updates channel" field be added?**
Decision: both exist now (`POST /channels/{id}/followers` from Phase 10 lounge-be; `public_updates_channel_id` in the community settings). The Follow button and the updates-channel badge wire to them.
Status: `DEFAULT`. Follow-up: G-30.

**P10-LFE-2. Is a real route for a thread full view wanted?**
Decision: yes, parity: `/channels/{guild}/{thread_id}` resolves a thread id to its parent channel with the thread pane open in full view, so threads can be linked and bookmarked like on Discord.
Status: `DEFAULT`. Follow-up: G-31.

### Phase 10 server settings (P10-SBE, P10-SFE)

**P10-SBE-1. Allow the `opus` crate (libopus) for soundboard transcoding via an ADR, or wait for a pure-Rust encoder?**
Decision: neither; drop the server-side transcode. Judged against doc 03: libopus is **not** in section 4's allowed list (section 2's "opus bindings" row was aspirational), so it would need a new ADR, and the ADR would buy nothing: browsers decode MP3, Ogg and WAV with `decodeAudioData`, soundboard playback goes through the user's mic path where the browser encodes Opus, and voice messages are encoded client-side already. Server-side Opus is only wanted for normalising storage, which 512 KiB files do not need. Spec section 7 loses "→ Opus"; `symphonia` remains the only audio decoder in scope and is only added when a feature needs server-side decoding. Revisit if a maintained pure-Rust Opus encoder appears.
Status: `ADOPTED` (D-201). Follow-up: G-32 (docs).

**P10-SBE-2. Should an approved membership application write an audit entry?**
Decision: yes. Guild audit action `200 APPLICATION_DECISION` (SIDE ROOM range 200 to 255, outside Discord's ids; ported bots ignore unknown actions) with `options {status, application_id}`; one entry per decision.
Status: `DEFAULT`. Follow-up: G-33.

**P10-SBE-3. Should the server tag also be shown on guild member lists?**
Decision: yes (Discord shows tags in the member list). `primary_guild` goes on the member and ban `UserRef` (`routes/guilds/dto.rs`) and the lazy member list payloads, then the chat renderer and member list draw it with the existing `TagPreview`.
Status: `DEFAULT`. Follow-up: G-34.

**P10-SFE-1. Should the Community wizard block when safety checks fail instead of fixing them on Finish?**
Decision: block, like Discord: the safety step shows the two switches and Next is disabled until both are on; Finish then writes what the user enabled, never silently.
Status: `DEFAULT`. Follow-up: G-35.

**P10-SFE-2. Store preset banners as a colour pair instead of generated PNGs?**
Decision: no. Discord stores images only; the PNG round-trips through every client and bot without a new field.
Status: `DEFAULT`. No follow-up.

**P10-SFE-3. Which backend items are wanted before the next settings pass?**
Decision, in order: (1) the Server Settings → Features page already renders guild feature modules since Phase 11 (nothing to add); (2) access mode and membership applications: the backend exists (Phase 10 sset-be), the UI is next; (3) DM and spam protection settings (LNG-SSET-052) and the moderator 2FA requirement (`mfa_level`, Discord parity) need backend fields; (4) `system_channel_flags` and widget settings are small parity fields that go with (3). The rest waits for a request.
Status: `DEFAULT`. Follow-up: G-36.

### Phase 10 voice (P10-VBE, P10-VFE)

**P10-VBE-1. Shard the SFU actor now or later?**
Decision: later. One actor at about 280 Mbit/s covers the production instance (doc 08 section 6.2: a free channel worst case is 78 Mbit/s) many times over; the first sharding number must come from the AX42-class host, not a shared Windows box. VOX-SFU-011's exit gate stays open and is measured on the real hardware before any code moves.
Status: `DEFAULT`. No follow-up now.

**P10-VBE-2. Turn `voice.video_bwe` on by default for video-heavy deployments?**
Decision: off by default. The 50 kbit/s padding per connection is a real cost on audio-only rooms; the loss-based fallback is the right default for a friend-group box. `docs/dev/server.md` tells a video-heavy operator when to flip it.
Status: `DEFAULT`. No follow-up.

**P10-VBE-3. Backfill `USE_EXTERNAL_SOUNDS` into existing guilds' `@everyone` roles?**
Decision: yes, by migration, together with `START_WATCH_PARTY` (Phase 8 added it to new servers only). Both are free features (D-044, D-110); a server owner who removed one of them on purpose has not had the chance yet, so the backfill cannot undo a choice.
Status: `ADOPTED` (D-202). Follow-up: G-37.

**P10-VBE-4. Persist stage instances and running calls, or keep them tied to the in-memory voice states?**
Decision: keep them in memory with the voice states. A restart ends the voice session anyway and members rejoin; persistence only pays off with a multi-node SFU, which is the same milestone as sharding (P10-VBE-1).
Status: `DEFAULT`. No follow-up.

**P10-VBE-5. Should `GUILD_CREATE` carry `stage_instances` and `soundboard_sounds` like Discord's?**
Decision: yes (D-060). The REST routes and op 31 stay for refresh.
Status: `DEFAULT`. Follow-up: G-38.

**P10-VFE-1. Should Decline on a ringing DM call tell the server?**
Decision: yes, and the route exists: `POST /channels/{id}/call/stop-ringing` (Phase 10 voice-be). The client calls it on Decline; `CALL_UPDATE` shrinks `ringing`.
Status: `DEFAULT`. Follow-up: G-39.

**P10-VFE-2. Is 30 seconds the ring time?**
Decision: the server is authoritative and rings for 45 seconds (`RING_TIMEOUT`); the client's overlay follows `CALL_UPDATE/DELETE` and its local fallback timer becomes 45 seconds.
Status: `DEFAULT`. Follow-up: G-39.

**P10-VFE-3. Confirm the camera and share presets (480p30 to 1080p60) and the free cap (720p30).**
Decision: confirmed; they are the doc 08 section 2.2 ladder (720p30 free; 720p60/1080p30 at Level 1; 1080p60 at Level 2+ and for Patrons). `CALL_CREATE` gains `video: bool` so the ring says "video call" (Phase 10 integration gap).
Status: `DEFAULT`. Follow-up: G-40.

### Phase 11 feature modules (P11-FM)

**P11-FM-1. A starboard post is removed when the count falls below the threshold; a moderator-deleted post is not reposted.**
Decision: accepted (matches the common bots people port from).
Status: `DEFAULT`. No follow-up.

**P11-FM-2. Module actions run as the server owner in the audit log. Acceptable, or add a system actor?**
Decision: a system actor. Audit entries written by a feature module name the system user (id 1) as `user_id` with `options.module = "<feature id>"`, the way Discord attributes AutoMod actions to AutoMod rather than to whoever enabled the rule. The audit viewer shows "SIDE ROOM · Starboard". The owner's name must not appear on actions they did not take.
Status: `ADOPTED` (D-187). Follow-up: G-41.

**P11-FM-3. Reaction-role "verify" mode grants once; un-reacting keeps the role.**
Decision: accepted (that is what "verify" means in the bots it replaces).
Status: `DEFAULT`. No follow-up.

**P11-FM-4. Is the system user joining servers as a visible member acceptable?**
Decision: no. The system user keeps its membership row (the message path needs it) but is excluded from member lists, member counts, the lazy list, prune, mentions autocomplete and `@everyone` fan-out; its messages carry Discord's `SYSTEM` tag and it cannot be kicked, banned or given roles. Members see posts "from SIDE ROOM", not a member called SIDE ROOM.
Status: `ADOPTED` (D-188). Follow-up: G-42.

**P11-FM-5. Scheduled-message and reminder routes allow bot tokens; should they deny?**
Decision: deny. They are user-scope features (a person's own reminders and scheduled posts); a bot that wants scheduling has its own clock. The bot deny-list in `auth/policy.rs` gains `/users/@me/reminders*`, `/users/@me/scheduled-messages*`, `/users/@me/bookmarks*` and the birthday routes.
Status: `ADOPTED` (D-189). Follow-up: G-43.

### Phase 11 Lounge messages, shell, profiles (P11-MSG, P11-SHL, P11-INT, P11-CLI)

**P11-MSG-1. Default sticker packs: procedurally drawn original artwork, or supplied artwork?**
Decision: supplied, and only when the owner has some. No procedural art ships. The `sticker_packs` table and the send-path change (message creation accepts a pack sticker id) are built now so a pack can be added as a reviewed layer-4 `sticker_pack` package (doc 11 section 4) without code; until then the sticker picker shows guild stickers only.
Status: `NEEDS-OWNER` (artwork). Default: no default packs. Follow-up: G-44.

**P11-MSG-2. Add Klipy now, or is Tenor enough?**
Decision: Tenor is enough. A second provider is a feature-flagged addition if Tenor's key policy ever bites. The Tenor API key is operator configuration (`[gif] api_key`), which the operator obtains from Google.
Status: `DEFAULT`. No follow-up.

**P11-MSG-3. Suppress push while the user has an active gateway session (Discord does)?**
Decision: yes. A push is not sent while any of the user's sessions is connected and not idle (`afk: false`); when every session is idle or gone, push goes out. DMs and direct mentions only, as now.
Status: `ADOPTED` (D-191). Follow-up: G-45.

**P11-SHL-1. Reading activity wire type: 6, or type 3 with a marker?**
Decision: 6. Discord's types are 0 to 5; a SIDE ROOM value outside that range is ignored by ported bots and never mistaken for Watching. Documented in doc 05 as the one activity-type extension.
Status: `ADOPTED` (D-193). Follow-up: G-46 (docs).

**P11-SHL-2 / P11-INT-2. Group DM invite links: confirm `POST /channels/{id}/invites` on a group DM.**
Decision: confirmed, parity with Discord's group DM invites: the group owner creates one (`{code, channel: {type: 3}}`, 24-hour expiry, unlimited uses, revocable), `/invite/{code}` joins the group after login (D-021), and the invite is always in-platform (never single use, D-022 does not apply).
Status: `DEFAULT`. Follow-up: G-47.

**P11-SHL-3. Decoration and effect pack catalogue: where do approved free packs live?**
Decision: a built-in catalogue in Rust (`sr_chat::profile::catalog`: slug, kind, asset path, label key) served by `GET /sr/profile-catalog` and validated on `PATCH /users/@me`; assets live under `apps/web/public/profile/` with their licence file. The four CSS effects (`sparkle`, `aurora`, `snow`, `embers`) are the catalogue today; avatar decorations need artwork, which is the owner's to supply or to approve as a layer-4 package later.
Status: `NEEDS-OWNER` (artwork). Default: effects only, no decorations. Follow-up: G-48.

**P11-SHL-4. The PWA update toast and the Lounge notices-bar entry can both show.**
Decision: the bar wins inside the Lounge (the toast is suppressed there); the toast stays for the Entrance, Cinema and Library, which have no bar.
Status: `DEFAULT`. Follow-up: G-49.

**P11-INT-1. Group DM icon: the server stores `icon` verbatim and accepts any string.**
Decision: parity with guild icons: `PATCH /users/@me/channels/{id} {icon}` takes a data URI through the media pipeline (`MediaKind::ChannelIcon`), stores the hash and rejects anything else.
Status: `DEFAULT`. Follow-up: G-50.

**P11-INT-3. The client never sends `afk: true`; the gateway drops `platform`, `party` and `expires_at_ms` from activities.**
Decision: the client sends op 3 with `afk: true` after 10 minutes without input (Discord's idle rule) and `false` on the next input; the activity whitelist keeps `platform`, `party.id` and `expires_at_ms` (SIDE ROOM extensions, documented in doc 05).
Status: `DEFAULT`. Follow-up: G-51.

**P11-INT-4. Per-server profile has no pronouns, theme or effect overrides.**
Decision: add them (D-044 makes the Nitro-only per-server cosmetics free): `guild_members.pronouns`, `theme_primary`, `theme_accent`, `profile_effect`, `avatar_decoration`, editable through `PATCH /guilds/{id}/members/@me` and returned in `guild_member_profile`.
Status: `DEFAULT`. Follow-up: G-52.

**P11-INT-5. Badge descriptions are client labels; the server sends no tooltip text.**
Decision: the server sends `badges: [{id, description, icon}]` from one Rust catalogue (`sr_admin::BADGES` extended); the client translates known ids and falls back to the server text.
Status: `DEFAULT`. Follow-up: G-53.

**P11-INT-6. Connection stats keys are shown as sent.**
Decision: stats keys are a closed set (`followers`, `subscribers`, `karma`, `stars`, `repos`) with i18n labels; unknown keys render as sent.
Status: `DEFAULT`. Follow-up: G-53.

**P11-CLI-1. Server-tag badge glyphs 0 to 15 are plain Unicode symbols; replace with artwork?**
Decision: keep the Unicode set until artwork exists; a 16-glyph SVG sheet drops into `lib/serverTag.ts` without a data change.
Status: `NEEDS-OWNER` (artwork). Default: Unicode glyphs. No code follow-up.

**P11-CLI-2 (and P10-ADM-7). Should critical announcements be undismissable, and should announcement DMs have an opt-out or a schedule?**
Decision: critical announcements (`critical: true`, meant for outages and security notices) cannot be dismissed and are always DM'd; ordinary announcements are dismissable in the bar and are DM'd only when the author ticks "also send as DM" and the recipient has not turned off `notifications.platform_dm` (default on). No scheduling: the author posts when it should go out.
Status: `ADOPTED` (D-192). Follow-up: G-54.

### Phase 11 user settings and account (P11-USB, P11-USF)

**P11-USB-1. Should deletion also be blocked for accounts holding the Owner or a staff role?**
Decision: yes. An account with an active staff appointment cannot delete itself; it must leave the Staff Server first (which revokes staff status at once, D-130) and the error says so. The Owner cannot delete at all until `sr-cli staff transfer-owner` has run. This closes a path where a departing staff member could erase their own trail.
Status: `ADOPTED` (D-194). Follow-up: G-55.

**P11-USB-2. Reuse the `[discord]` OAuth app for account linking, or a separate one?**
Decision: the owner registers the second redirect URI on the existing Discord application; a separate app is only needed if Discord's per-app limits are hit. Until the URI is registered the Discord connection button is hidden (`available: false`).
Status: `NEEDS-OWNER` (external account). Default: reuse the one app.

**P11-USF-1. Should Copy ID stay always visible, or be gated on Developer Mode like Discord?**
Decision: gate it on Developer Mode (parity; the earlier "always shown" choice is overridden). Copy Link stays always visible.
Status: `ADOPTED` (D-195). Follow-up: G-56.

**P11-USF-2. Auto-enable Streamer Mode when OBS is detected?**
Decision: not possible in a browser; the manual switch is the feature. Revisit with native clients (D-140).
Status: `NO-CODE`.

**P11-USF-3. Which invite and credit routes should the backend add so the Invites section (LNG-USET-025 to 029) can be built?**
Decision: three member routes: `GET /sr/invites` (remaining credits, frozen flag and reason, open codes with created/expires, ledger of redeemed ones with the invitee's username), `POST /sr/invites` (one code; refused with `900013` when no credit, frozen or at the account cap, D-014/D-024/D-026), `DELETE /sr/invites/{code}` (revoke while unredeemed). Session only, bots denied, rate limited 10 per hour.
Status: `DEFAULT`. Follow-up: G-57.

### Phase 11 Library and review (P11-LIB, P11-REV)

**P11-LIB-1. Reading-privacy setting for the Friends and "People you know" trending and rankings scopes.**
Decision: no new setting; the D-120 one applies. The scoped aggregates count only members whose "Share my watching & reading activity" (Library section toggle) is on and whose history is not paused, and never age-restricted titles for members with "Hide age-restricted titles from my activity" on. Those toggles move from the client prefs blob to server-readable user settings (`activity.share_cinema`, `activity.share_library`, `activity.share_tv`, `activity.hide_age_restricted`, `activity.paused`) so the gateway presence, the aggregates and the profile tab all read the same values.
Status: `ADOPTED` (D-190). Follow-up: G-58.

**P11-LIB-2. PDF rasterization (pdfium) approval (also `docs/open-questions.md` item 2).**
Decision: pdfium is already on the doc 03 section 4 list (ADR-0005), so no new ADR; what needs the owner is bundling a prebuilt pdfium binary beside the ingest worker, which is a download. Recommended default: do not bundle. PDFs are read in the browser with `pdf.js` (doc 03 section 3, the planned reader), the cover thumbnail is rendered client-side by `pdf.js` at upload time and submitted as the cover image, and the server never rasterizes. Image-mode PDFs (pre-rendered page images) stay unbuilt unless the owner approves the bundle.
Status: `NEEDS-OWNER` (download). Default: client-side `pdf.js`, no server rasterizer. Follow-up: G-59.

**P11-REV-1. Should the daily upload limit be an operator setting instead of the constant 50 GB?**
Decision: yes: platform setting `submissions.daily_upload_bytes` (default 50 GiB, `0` = unlimited), declared in the feature registry so Admin → Limits renders it; staff stay exempt (D-082).
Status: `ADOPTED` (D-196). Follow-up: G-60.

**P11-REV-2. Should abandoned or rejected Cinema submissions purge uploaded media too?**
Decision: yes; doc 09 section 1 already says rejected submission files are purged after 14 days (metadata kept), and abandoned drafts follow the same rule. The Cinema kinds implement `SubmissionKind::purge` (originals and any transcode output of the unpublished item) and `credit_refs` (replacements earn nothing, D-074).
Status: `DEFAULT`. Follow-up: G-61.

**P11-REV-3. REV-CORE-001: the tables are `content_requests` / `content_request_votes`, not `requests` / `request_votes`.**
Decision: the built names stand (clearer in a schema that also has data requests and staff offers); the inventory line is updated and the box ticked.
Status: `DEFAULT`. Follow-up: G-62 (inventory).

### Moderation and compliance spec, section 14 (MOD)

**MOD-1. Who triages reports in a server, and may server moderators see the reporter?**
Decision: a report on guild content goes to that guild's moderators (`MANAGE_MESSAGES`, or the configured report channel) and only they see it; platform staff see it when it is escalated, when the category is `illegal_content` or `child_safety` (always escalated), or when the target is a DM, a user outside a guild, a server, or a library or cinema item (spec section 3.4). Reporter identity: hidden from server moderators unless the reporter ticks "moderators may see my name" (off by default); always visible to platform staff and access-logged (D-125).
Status: `ADOPTED` (D-209). Follow-up: G-63 (spec wording only now; code in Phase 7).

**MOD-2. Dedicated `MANAGE_CASES` and `MANAGE_TAKEDOWNS` platform bits, or reuse?**
Decision: add the two bits now, before Phase 7 code exists: `MANAGE_CASES` (bit 22: open, assign, act on and close cases; place and release holds) and `MANAGE_TAKEDOWNS` (bit 23: the takedown register and the data-request queue). Reusing `MANAGE_ACCOUNTS` would hand the Support role case access it should not have. Defaults: Moderator gains `MANAGE_CASES`; Senior Staff gains both; the Owner has everything. Doc 07 section 3 and the D-162 table are updated by a migration of `staff_role_templates`.
Status: `ADOPTED` (D-210). Follow-up: G-64.

**MOD-3. Which `reason_category` values are serious by default for D-031, and is the list right?**
Decision: the list stands. Serious by default: `illegal_content`, `child_safety`, `fraud`, `platform_abuse`; `harassment` and `ban_evasion` default to standard and the ban dialog lets staff raise them; `spam`, `nsfw_policy`, `other` default to standard. The stored `category` remains what the invite graph reads, so the defaults are only presets.
Status: `ADOPTED` (D-211). Follow-up: G-65.

**MOD-4. Copyright: public takedown form and formal counter-notice, or staff-entered only? Repeat-infringer threshold?**
Decision: the owner's, since the process is jurisdiction policy. Recommended default: staff-entered notices only (spec section 5), counter-notices recorded by staff, repeat-infringer threshold `moderation.strike_review_threshold` = 3 upheld takedowns in 12 months opening a review case (never an automatic suspension). The contact address for notices is published by the operator (doc 12 section 9).
Status: `NEEDS-OWNER` (legal). Default as stated.

**MOD-5. Underage accounts: ban, or reset-and-restrict? Is there a minimum registration age?**
Decision: the owner's (legal). Recommended default: a minimum registration age of 13, stated in the invite landing and enforced only by self-declaration (no verification); an account found to be under 18 gets its age confirmation reset and restricted spaces closed (reset-and-restrict, not a ban); an account found to be under 13 is suspended with `reason_category = other` and a note, and the inviter is told only that the invitee did not meet the entry rules. A platform setting `accounts.minimum_age` (default 13) carries the number.
Status: `NEEDS-OWNER` (legal). Default as stated.

**MOD-6. Data-subject deadlines and jurisdiction; is keeping the Person record after erasure acceptable, and should the deletion copy say so?**
Decision: the jurisdiction is the owner's to name. Defaults that do not depend on it: the queue shows a due date from `compliance.data_request_deadline_days` (default 30, the GDPR number, which is the strictest common one); the deletion screen and the data-export README state plainly that the Person record, invite graph, invoices and staff-side moderation history are kept (D-124) and why. Whether that retention is lawful where the instance runs is the owner's call.
Status: `NEEDS-OWNER` (legal). Default as stated. Follow-up: G-66 (copy).

**MOD-7. How long are closed cases, evidence and notices kept, and should holds expire or be reviewed?**
Decision: kept forever (consistent with D-124); holds never expire on their own but are flagged for review after `moderation.hold_review_days` (default 90) on the holds dashboard, and a release needs a reason. Nothing silently unfreezes.
Status: `ADOPTED` (D-212).

**MOD-8. May server moderators place holds, or only platform staff?**
Decision: platform staff only (`MANAGE_CASES`). Server moderators delete, time out, kick and ban inside their server; a hold is a platform evidence action that overrides the author's deletion rights (D-122), which a server moderator must not be able to invoke.
Status: `ADOPTED` (D-213).

**MOD-9. May a platform reviewer see a reported DM message, given D-125?**
Decision: yes, within the snapshot only: the reported message and the ten before and after it, taken at report time by the reporter's own action (a participant exposing a conversation they are part of). Opening the snapshot is access-logged with the case id as the reason; reading anything beyond the snapshot needs `READ_PRIVATE_CONTENT` and the normal D-125 flow.
Status: `ADOPTED` (D-214).

**MOD-10. Are appeals wanted at launch, and who reviews them when staff are few?**
Decision: yes, minimal: one appeal per decision, from the ban screen, reviewed by a different staff member than the decider; when nobody else holds the bit, the Owner reviews; no second appeal.
Status: `ADOPTED` (D-215).

**MOD-11. Should the web client get a flag-read endpoint so `moderation.reports` can be turned on per environment without a rebuild?**
Decision: yes: `GET /sr/features/flags` (any signed-in human, bots denied) returns the platform-scope flags whose schema marks them `client_visible`; the client reads it at boot and on `SR_FEATURE_UPDATE`, replacing the build-time `VITE_FEATURE_MODERATION_REPORTS` seed. Guild-scope flags keep `GET /guilds/{id}/modules`.
Status: `ADOPTED` (D-216). Follow-up: G-67.

### Still with the owner from earlier rounds

`docs/open-questions.md` keeps the UnRAR licence confirmation (D-170) as before; its PDF item is answered above as P11-LIB-2.

## Phase 12 handoff questions (answered 2026-10-15)

Every open owner question, "still open" item and "decision" left for confirmation in the Phase 12 handoffs (`docs/dev/handoff/feat-p12-admin-ui.md`, `feat-p12-feat-ui.md`, `feat-p12-web-followups.md`, `feat-p12-firstpaint.md`, `feat-p12-admin-mod-be.md`, `feat-p12-devplat-be.md`, each on its own branch), plus the one item of [open-questions.md](open-questions.md) that was not legal, money, artwork or an external account (PDF rasterization, item 2, now closed). Same rules as the earlier rounds: [01-decisions-log.md](01-decisions-log.md) wins; Discord parity by default; privacy and safety per docs 09 and 15; Rust first per doc 03; conservative security defaults; one Owner running a friend-group instance.

Four of these were decided by the owner in chat today and are recorded here as given (direct appointment kept for the Owner, slash commands offered everywhere, the birthday card's home, `pdfjs-dist`). Status key as before: `ADOPTED` = a product decision, recorded as D-217 onward (or, for the direct-appointment answer, the already-rewritten D-197); `DEFAULT` = engineering default, implement without the owner; `NEEDS-OWNER` = needs the human, with the default that applies meanwhile; `NO-CODE` = informational or already resolved on another branch. Follow-ups are G-68 onward in [dev/answers-followups-p12.md](dev/answers-followups-p12.md).

| Status | Count |
|---|---|
| ADOPTED | 6 |
| DEFAULT | 29 |
| NEEDS-OWNER | 0 |
| NO-CODE | 2 |

37 questions in all (the `#review-queue` card-action item appears in two handoffs and is answered once, as P12-AUI-5). Nothing in this round needs the owner: no legal, money, artwork or external-account question was raised. Of the previous round's nine NEEDS-OWNER items, P11-LIB-2 is closed by the `pdfjs-dist` decision (P12-WFU-1); the other eight stand unchanged in [open-questions.md](open-questions.md).

### Admin web UI (P12-AUI, branch `feat/p12-admin-ui`)

**P12-AUI-1. Direct appoint is shown to the Owner only. Remove it entirely or keep it?**
Decision (owner, 2026-10-15): keep it, Owner only, behind re-auth. `POST /admin/staff` refuses everyone except the Owner (staff with `APPOINT_STAFF` included) and still needs the 15-minute second-factor window; "Send appointment offer" stays the primary action for everyone. This is the same owner decision the `feat/p12-admin-mod-be` branch already recorded by rewriting D-197 and the P10-ADM-1 entry above; the admin-ui branch's Owner-only "Appoint directly" button is the matching UI and nothing is removed. How the button is presented is P12-AMB-2.
Status: `ADOPTED` (D-197 as amended on `feat/p12-admin-mod-be`; no new decision number). No further follow-up beyond G-80.

**P12-AUI-2. The delegation ceiling excludes Owner and the Owner-only permissions (`READ_PRIVATE_CONTENT`, `APPOINT_STAFF`, `VIEW_ACCESS_LOG`, `OPERATOR`) in the UI; the server also refuses them.**
Decision: confirmed, exactly D-162's grant rule ("only the Owner may grant" those four) applied to delegations. The client hides what the server refuses so nobody builds a ceiling that cannot be saved; the server stays the boundary.
Status: `DEFAULT`. No follow-up.

**P12-AUI-3. The announcement composer sends `reason` only when typed.**
Decision: confirmed. `reason` is the audit note, optional on the route, and an empty string must not become an audit row that says nothing.
Status: `DEFAULT`. No follow-up.

**P12-AUI-4. Voice rooms and egress dashboard tiles: `/admin/stats` had no such fields.**
Decision: resolved on `feat/p12-admin-mod-be`, which added `voice_rooms` and `egress_bytes` (null without the embedded SFU) to `GET /admin/stats`; the tiles light up at integration. Nothing to decide.
Status: `NO-CODE`.

**P12-AUI-5. `#review-queue` Approve/Reject card buttons: the card-action registry knows only `sr:ack`, `sr:clear-suspicion` and `sr:retry-job`, and nothing posts review cards.** (Also raised by `feat-p12-admin-mod-be.md`.)
Decision: build the feed as doc 07 section 4 describes it ("bot-fed feed of submissions with Approve/Deny buttons"). When a submission enters review, the review service posts a system card to `#review-queue` (title, kind, submitter, size, a link to the review page) with buttons `sr:review-approve:<submission id>` and `sr:review-reject:<submission id>`. Approve runs the same service call as `POST /admin/review/...` under the same permission bit for that kind (`REVIEW_READING`, `REVIEW_MEDIA`, `REVIEW_APPS`, `REVIEW_PACKAGES`) and the same re-auth rule; Reject needs a reason (D-082 rejection copy), so the button opens the review page with the reject dialog focused rather than rejecting blind. The card is edited to its outcome ("Approved by X", "Rejected by X: reason") and the buttons are removed, as the staff-offer cards already do. One decision per card: a second press answers "already decided".
Status: `DEFAULT`. Follow-up: G-68.

**P12-AUI-6. Announcement edit: there is no `PUT` route, so "Reuse" prefills the composer.**
Decision: add `PATCH /admin/announcements/{id}` for an announcement that has not ended (`title`, `message`, `ends_at`; `severity`, `critical` and `send_dm` are fixed at publish). An edit never re-sends the system DM (a DM is a one-time delivery; the announcement bar shows the current text) and writes `platform_audit_log` `announcement.edit` with the diff. The composer's "Reuse" stays for ended announcements; active ones get "Edit".
Status: `DEFAULT`. Follow-up: G-69.

### Feature-module web UIs (P12-FUI, branch `feat/p12-feat-ui`)

**P12-FUI-1. The slash commands `/rank`, `/suggest` and `/ticket` are always offered in servers; the Community dialog says so when no module is on.**
Decision (owner, 2026-10-15): confirmed. The commands are offered in every server; when the matching module is off, the dialog explains that and (for people who manage the server) links to Server Settings → Features. The alternative, a per-server command list that changes as modules toggle, would need a gateway refresh of the command set for a marginal gain.
Status: `ADOPTED` (D-218). No follow-up.

**P12-FUI-2. The birthday card lives on User Settings → Profiles → Servers, not the main profile tab.**
Decision (owner, 2026-10-15): confirmed. The birthday is announced per server (the module's announcement channel and role are server settings), so it sits with the per-server profile; the main profile tab stays what Discord shows there.
Status: `ADOPTED` (D-219). No follow-up.

**P12-FUI-3. English slash-command descriptions stay literal, like the existing ones, because the command table is not yet in the catalogue.**
Decision: confirmed for now. When the built-in command table is moved into the i18n catalogue (a P9-I18N follow-up, not this round), the three descriptions move with the rest; nothing is special about them.
Status: `DEFAULT`. No separate follow-up.

**P12-FUI-4. Level roles are visible only to people who can read the module settings (Manage Server); there is no member route.**
Decision: expose them to members. The rank response that every member already reads (the leveling module's own rank route, whatever its final path) gains `level_roles: [{level, role_id}]`, filtered to roles the member can see in the guild; no second route. Members should know what they are climbing towards, and the data is not sensitive (role names are in `GUILD_CREATE`).
Status: `DEFAULT`. Follow-up: G-70.

**P12-FUI-5. Suggestion and ticket staff controls are shown to Manage Server only; the module's `staff_role` members are authorised by the API but the client cannot know the role without the settings route. A `can_respond` flag on `GET /guilds/{id}/modules` would fix it.**
Decision: yes. Each entry of `GET /guilds/{id}/modules` gains `member_capabilities`, a small map computed server-side for the caller (`can_respond` for suggestions and tickets now; other modules add keys as they need them). The server computes it from the same rule the API enforces (`MANAGE_GUILD` or a member of `staff_role`), so the client never guesses and the settings route stays Manage Server only.
Status: `DEFAULT`. Follow-up: G-71.

**P12-FUI-6. Suggestion vote counts are inferred from the card message's reactions, minus the system user's seed vote. A `votes` field would be sturdier.**
Decision: add `votes: {up, down}` to the suggestion DTO, computed by the module from the card's reactions with the system user's seed excluded (the module owns the card and already reads the reactions to update status); the client stops counting reactions. A member's own vote is reported as `my_vote: up | down | null` so the buttons can show state.
Status: `DEFAULT`. Follow-up: G-72.

**P12-FUI-7. The ticket "open" route is still not tested end to end on the backend (since p11).**
Decision: write the test before anything else touches tickets: open → thread created with the opener and `staff_role` added → close → thread archived, in the `sr-server` Postgres suite with `TestWorld`.
Status: `DEFAULT`. Follow-up: G-73.

**P12-FUI-8. Slowmode presets applied on demand (FTR-SLOW-001): the backend has only weekly schedules, so there is no UI.**
Decision: add the on-demand action. `POST /guilds/{id}/sr/slowmode/apply {preset_id, channel_ids, minutes?}` (needs `MANAGE_CHANNELS` in every listed channel) sets each channel's `rate_limit_per_user` from the preset, writes the usual `CHANNEL_UPDATE` audit entries attributed per D-187, and, when `minutes` is given, schedules a one-shot revert to the previous value. Web: "Apply slowmode preset" in the channel context menu and on the module card. The weekly schedules stay as built.
Status: `DEFAULT`. Follow-up: G-74.

### Web follow-ups (P12-WFU, branch `feat/p12-web-followups`)

**P12-WFU-1. Approve `pdfjs-dist` as an npm dependency (a download).** (Closes open-questions.md item 2 and the P11-LIB-2 default.)
Decision (owner, 2026-10-15): approved and added. `pdfjs-dist` 6.4.299 is a dependency of `apps/web`, loaded lazily as its own chunk with the worker as a separate asset, so it never enters a first-load bundle. PDFs are read in the browser; the server validates them (`pdf.rs`) and never rasterizes. `pdfium-render` stays listed under ADR-0005 but is not used and is not bundled; LIB-ING-004's image mode is closed, not deferred. Doc 03 section 3 already names `pdf.js` for the reader; section 4 gains a note that pdfium is approved but unused.
Status: `ADOPTED` (D-217). Follow-up: G-88 (docs).

**P12-WFU-2. Should the pdf.js worker run as a real worker (`worker-src 'self' blob:` already allows it)?**
Decision: yes, a real `Worker` from the `?url` asset, as the branch now does. Parsing on the main thread would freeze the reader on large files, and the CSP needs no change.
Status: `DEFAULT`. No follow-up.

**P12-WFU-3. G-17 backend: `PATCH /users/@me` must accept `locale` (BCP 47 tags with catalogues). Today the field is ignored.**
Decision: accept it. The server validates against the list of catalogues it ships (`en`, the pseudo-locales excluded) and stores `accounts.locale`; the user object returns it (Discord's `locale` field), `USER_UPDATE` carries it, and outbound email uses it. Unknown tags are a 400 field error.
Status: `DEFAULT`. Follow-up: G-75.

**P12-WFU-4. The Updates badge reads `public_updates_channel_id` from `GET /guilds/{id}/sr/community`; it would be cheaper on the guild object.**
Decision: put it on the guild object, which is Discord's shape: `public_updates_channel_id`, `rules_channel_id` and `safety_alerts_channel_id` are fields of Discord's guild object, so `GET /guilds/{id}` and `GUILD_CREATE` carry them (null when the community features are off) and `GUILD_UPDATE` follows changes. The `sr/community` route keeps them too.
Status: `DEFAULT`. Follow-up: G-76.

**P12-WFU-5. A `POST /share` only works while the service worker is active; the server has no `/share` POST route, so a fresh visit gets the shell or a 405.**
Decision: add the fallback. `sr-server` answers `POST /share` with `303 See Other` to `/share` (the files are dropped, since nothing can hold them without the worker) and the page says "Open SIDE ROOM once, then share again". Cheap, and it avoids a proxy error page from an installed-app share on a cleared browser.
Status: `DEFAULT`. Follow-up: G-77.

**P12-WFU-6. G-59 remainder: no cover-submission endpoint for member submissions, no image mode, no reading progress for PDFs.**
Decision: cover endpoint yes, image mode no, progress yes. `POST /library/submissions/{id}/cover` (submitter only, while the submission is `draft` or `pending`) takes one JPEG or PNG up to 2 MiB, sniffed and re-encoded by the existing cover pipeline; the web submits `renderPdfCover(file)`'s blob after upload. Image mode is closed with D-217 (no server rasterizer). PDF reading progress uses the existing chapter-progress route with the page index as the position, like image chapters.
Status: `DEFAULT`. Follow-up: G-78.

**P12-WFU-7. G-35: the community wizard's safety switches write the guild immediately when toggled, so Finish never changes verification level or content filter.**
Decision: confirmed. Each switch is an explicit, audited `GUILD_UPDATE`; a silent change on Finish was the bug the item fixed.
Status: `DEFAULT`. No follow-up.

**P12-WFU-8. G-56: the earlier tests that expected Copy ID without Developer Mode were changed to turn it on.**
Decision: that is the correct consequence of D-195.
Status: `NO-CODE`.

**P12-WFU-9. G-17: the pseudo-locales never go to the account; the "Language & region" formatting select is untouched.**
Decision: confirmed. Pseudo-locales are a development aid and must never leak into email or another device; formatting (number and date locale) stays a separate device preference, as Discord separates language from region.
Status: `DEFAULT`. No follow-up.

### First paint (P12-FP, branch `feat/p12-firstpaint`)

The handoff raises no owner question. Two notes it leaves are taken up as follow-ups so they are not lost.

**P12-FP-1. Entrance components still contain some hard-coded English strings (for example LoginLink).**
Decision: move them into the `entrance` fragment, which the first-paint scan test already guards; no new mechanism.
Status: `DEFAULT`. Follow-up: G-86.

**P12-FP-2. On a cold direct load of a Lounge URL, the catalogue and the lazy route chunk are fetched in series behind the gate.**
Decision: start the route chunk import in parallel with the catalogue (kick off the `lazyNamed` import when the gate mounts, render when both resolve). Only if the measured gain on the throttled profile is at least 200 ms; otherwise record the measurement and close the item.
Status: `DEFAULT`. Follow-up: G-87.

### Admin and moderation backend (P12-AMB, branch `feat/p12-admin-mod-be`)

**P12-AMB-1. Should `staff.show_badge` and `notifications.platform_dm` get Settings switches (and a user-settings schema) now, or wait for the settings registry?**
Decision: add the two switches now; do not wait. Two nested keys in the settings blob, written as a merge patch like `cinema.share_watching`, need no registry: "Show the SIDE ROOM Staff badge on my profile" under Settings → Profiles (visible only while an appointment is active) and "Platform announcements by direct message" under Settings → Notifications. When a user-settings schema registry lands, both move into it with the rest of the blob keys. A setting that exists server-side but cannot be changed is a trap.
Status: `DEFAULT`. Follow-up: G-79.

**P12-AMB-2. Should the Staff screen keep both "Send offer" and the Owner-only direct form visible, or hide the direct form behind an "Emergency" fold?**
Decision: fold it. "Send appointment offer" is the one visible action; below the offers card the Owner sees a collapsed disclosure "Appoint directly (emergency)" with one line of explanation (the appointee is added without accepting; use when the offer flow cannot be used; always logged) and the form inside. The fold is remembered closed. This keeps the Owner's default path the same as everyone else's while honouring the owner decision that the route stays.
Status: `DEFAULT`. Follow-up: G-80.

**P12-AMB-3. G-67: no gateway dispatch on a flag change (`SR_FEATURE_UPDATE` would need a new event); clients read the flags at boot.**
Decision: add the event, as D-216 already names it: `SR_FEATURE_UPDATE {scope: "platform", flags: {…}}` to every session when a `client_visible` platform flag changes (guild-scope module toggles keep their existing dispatch). The payload is the full visible-flag map, so clients replace rather than merge. Without it a flag flip only reaches members on their next reload.
Status: `DEFAULT`. Follow-up: G-81.

**P12-AMB-4. G-66: `compliance.data_request_deadline_days` is not declared (no request queue yet), and Settings → Account removal still shows "not available yet" (LNG-USET-006 open) although `POST /sr/auth/account/delete` exists.**
Decision: the deadline setting waits for the request queue, as G-66 says. The deletion UI does not wait: Settings → Account → Delete account calls the existing route behind re-auth (password plus second factor when enrolled), shows the D-124 retention text the branch already wrote, and renders the route's `staff` / `owner` field errors as the "leave the Staff Server first" / "transfer ownership first" notices. Whatever cooling-off the route implements is shown as is; this item adds no new deletion semantics.
Status: `DEFAULT`. Follow-up: G-82.

**P12-AMB-5. G-60 reads `platform_settings` directly through an `AllowanceSource` port, not "through `sr-features`".**
Decision: accepted. The Limits catalogue is the platform-settings store, `sr-review` has no `sr-features` dependency and should not gain one for a single integer; the port is the right seam. G-60's wording is corrected in the checklist.
Status: `DEFAULT`. No follow-up.

**P12-AMB-6. `#review-queue` Approve/Reject card action.**
Decision: see P12-AUI-5.

**P12-AMB-7. G-54: the admin composer on this branch lacks `critical` and `send_dm`; the admin-ui branch must add them.**
Decision: integration work for the admin-ui composer: the severity picker's `critical` value sets `critical: true` (always DM'd, not dismissable, the warning text says so and the confirm button reads "Publish and message everyone"); the existing system-DM toggle becomes `send_dm` and is forced on and disabled for `critical`.
Status: `DEFAULT`. Follow-up: G-83.

### Developer platform backend (P12-DPB, branch `feat/p12-devplat-be`)

**P12-DPB-1. The cap of 25 testers counts open invitations too; the answers said "up to 25 accepted".**
Decision: count invitations. A pending invitation holds a slot, which stops invitation spam (25 pending invitations is the most any application can have out) and matches how Discord's App Testers list counts entries. The developer frees a slot by withdrawing a pending invitation (`DELETE /applications/{id}/testers/{account_id}` already removes either state). D-204's wording is amended.
Status: `ADOPTED` (D-220, amending D-204). No code follow-up.

**P12-DPB-2. An accepted tester may install a private `development` application; the checklist did not say.**
Decision: allowed. Testers exist to try the application before release, and the private flag ("only the owner can install") is about strangers, not people the owner invited by name; the 25-member guild rule and `MANAGE_GUILD` still apply. Limiting testers to public apps would make the tab useless for the common case of a bot written for one friend group.
Status: `ADOPTED` (D-220). No code follow-up.

**P12-DPB-3. Declining an invitation deletes the row; there is no "declined" state.**
Decision: keep it deleted (less personal data kept for no purpose, doc 09), with one guard: the same account cannot be re-invited to the same application within 24 hours of declining, enforced by a short-lived `app_tester_declines (application_id, account_id, declined_at)` row that the housekeeping job removes after 24 hours. Without the guard a decline can be answered by an immediate re-invite, which is a nag vector through system DMs.
Status: `DEFAULT`. Follow-up: G-84.

**P12-DPB-4. The endpoint failure window starts at the first failure of a streak; a success resets the streak.**
Decision: confirmed; that is what D-186 means by "25 consecutive failures within 10 minutes".
Status: `DEFAULT`. No follow-up.

**P12-DPB-5. Drop the plaintext column `applications.interactions_signing_seed` once every deployment has run `apps.seal_seeds`.**
Decision: next phase, migration `0803_drop_plaintext_signing_seed.sql` (renumbered from `0800` on 2026-10-15, P12-LGB-4), guarded: it raises (and the server refuses to start) if any row still has a non-NULL plaintext seed, with the message "run the server once on the previous release to seal seeds". The release notes carry the upgrade order. `secret_columns.rs` drops the `PLAINTEXT_OK` legacy entry in the same PR.
Status: `DEFAULT`. Follow-up: G-85.

**P12-DPB-6. The Testers list lives on the Installation tab; DEV-PORT-003 stays unticked because Emojis, Webhooks, Rich Presence and Activities are not built.**
Decision: accepted. Installation is where install eligibility is decided, so testers belong there; a separate App Testers tab appears when the portal reaches Discord's full tab set, and DEV-PORT-003 stays open until then.
Status: `DEFAULT`. No follow-up.

### Still with the owner from earlier rounds

Unchanged: the UnRAR licence confirmation (D-170) and the eight remaining NEEDS-OWNER items of the Phase 8 to 11 round (P8-DON-4, P11-MSG-1, P11-SHL-3, P11-CLI-1, P11-USB-2, MOD-4, MOD-5, MOD-6); P11-LIB-2 is closed by D-217. [open-questions.md](open-questions.md) lists them with their defaults.

## Phase 12 backend handoff questions (answered 2026-10-15)

The "decisions to confirm", "not done" judgement calls and listed gaps of the two Lounge backend handoffs of Phase 12 (`docs/dev/handoff/feat-p12-lounge-ch-be.md` on `feat/p12-lounge-ch-be`, `docs/dev/handoff/feat-p12-lounge-gw-be.md` on `feat/p12-lounge-gw-be`), plus a check of [open-questions.md](open-questions.md): every item still listed there is legal, money, artwork or an external account (or a human action), so nothing new is taken from it. Same rules as the earlier rounds: [01-decisions-log.md](01-decisions-log.md) wins; Discord parity by default; privacy and safety per docs 09 and 15; Rust first per doc 03; conservative security defaults; one Owner running a friend-group instance on one node (doc 02 section 5). Status key as before: `ADOPTED` = a product decision, recorded as D-221 onward; `DEFAULT` = engineering default, implement without the owner; `NEEDS-OWNER` = needs the human, with the default that applies meanwhile; `NO-CODE` = informational or already resolved. Follow-ups are G-89 onward in [dev/answers-followups-p12.md](dev/answers-followups-p12.md).

| Status | Count |
|---|---|
| ADOPTED | 4 |
| DEFAULT | 8 |
| NEEDS-OWNER | 0 |
| NO-CODE | 1 |

13 questions in all. Nothing in this round needs the owner. The two artwork items the handoffs mention again (default sticker packs, avatar decorations) are P11-MSG-1 and P11-SHL-3, already in [open-questions.md](open-questions.md) with their defaults, and are not counted here.

### Lounge chat backend (P12-LCB, branch `feat/p12-lounge-ch-be`)

**P12-LCB-1. Group DM invites reuse `POST /channels/{id}/invites` and make `InviteObject.guild` optional (`type` 1 for groups), a breaking change for typed clients that assumed a guild.**
Decision: confirmed; it is Discord's shape, not a departure from it. Discord's invite object has `type` (0 guild, 1 group DM, 2 friend) and an optional partial `guild`, so a client that assumed `guild` was assuming something Discord never promised, and ported bots already handle it. The OpenAPI schema says so explicitly (`type` required, `guild` nullable, `channel` carries the group's `id`, `type: 3`, `name` and `icon`), the generated TypeScript client follows, and the web switches on `type` rather than on the presence of `guild`. Two rules are added so a link to a private conversation stays a conversation among people who want each other there: joining by link is refused (404, the same answer as an unknown code, so there is no oracle) when the joiner and the group's owner block each other, and refused with the existing "group is full" error when the group already has Discord's 10 recipients. Twenty-four hours and unlimited uses stay; the link is always in-platform, so D-022's single-use default for links that leave the platform does not apply.
Status: `ADOPTED` (D-221). Follow-up: G-90.

**P12-LCB-2. Event auto-start: every scheduled event starts at its start time, voice and stage events included; Discord waits for someone to start those.**
Decision: match Discord for voice and stage events, keep the automatic start for external ones. An external event (`entity_type` 3) has nothing to join, so it goes `ACTIVE` at its start time and `COMPLETED` at its end time by the clock, as Discord does. A voice or stage event (`entity_type` 2 or 1) stays `SCHEDULED` at its start time until either someone with `MANAGE_EVENTS` starts it by `PATCH {status: 2}` or any member joins the linked channel at or after the start time (the gateway's new `VoiceJoined` domain event is the hook); an "Active" event over an empty room misleads everyone who looks at the Events list, and the join-to-start rule means a friend group never has to remember a Start button. An event that is still `SCHEDULED` 24 hours after its start time is marked `CANCELED` by the clock (not deleted: the audit entry and the interested list stay), with `GUILD_SCHEDULED_EVENT_UPDATE` as for any status change. A voice or stage event that is `ACTIVE` ends when its end time passes or, with no end time, when the linked channel has been empty for 15 minutes; a stage event also ends when its stage instance is closed. Nothing changes for `PATCH` by hand.
Status: `ADOPTED` (D-222). Follow-up: G-91.

**P12-LCB-3. The server 2FA requirement (`mfa_level`) gates kick, ban, unban, bulk ban, prune, roles, channels, overwrites and server edit or delete only, not message moderation, webhooks, invites or AutoMod; bots are exempt.**
Decision: widen the scope to Discord's elevated-permission list and keep the bot exemption with Discord's condition. Discord's requirement covers every action behind `KICK_MEMBERS`, `BAN_MEMBERS`, `ADMINISTRATOR`, `MANAGE_CHANNELS`, `MANAGE_GUILD`, `MANAGE_MESSAGES`, `MANAGE_ROLES`, `MANAGE_WEBHOOKS`, `MANAGE_THREADS`, `MANAGE_GUILD_EXPRESSIONS` and `MODERATE_MEMBERS`, so here the `MfaGate` is also applied to deleting someone else's message, bulk delete, pinning, timeouts and the other `MODERATE_MEMBERS` edits, webhook create, edit and delete, thread edit, archive and delete by a non-author, emoji, sticker and soundboard management, AutoMod rules, the community and safety settings, and invite deletion. Invite creation is not gated (`CREATE_INSTANT_INVITE` is not elevated on Discord either), and neither is anything a member does to their own content. The gate answers the existing 403 `900004` when the session has a second factor but has not proved it in the last 15 minutes (the re-auth dialog handles it), and a new 403 `900123 MFA_ENROLLMENT_REQUIRED` when the account has no TOTP and no passkey at all, so the client can send the person to Settings → My Account instead of asking for a code they cannot produce. Bots are exempt as on Discord, on Discord's condition: the application's owner account must have a second factor enrolled; a bot whose owner has none gets the same `900123` in a server with `mfa_level` 1. The Owner's own requirement to hold a second factor before raising the level stays.
Status: `ADOPTED` (D-223). Follow-ups: G-94 (backend), G-97 (web).

**P12-LCB-4. The scheduled-event clock task runs on every node, so a multi-node deployment may announce a start twice.**
Decision: no bug today (v1 is one `sr-server` node with the `InProcessBus`, doc 02 section 5), but the clock should not be the thing that breaks first when a second node appears. Move the tick into the job queue that doc 02 section 6 already names for "scheduled-event reminders": a `scheduled_events.tick` job leased with `FOR UPDATE SKIP LOCKED` every 30 seconds and re-enqueued by its runner, so exactly one node advances events. The status update stays idempotent, as the handoff says.
Status: `DEFAULT`. Follow-up: G-92.

**P12-LCB-5. `dm_spam_filter`, `explicit_media_filter_dms`, `hide_suspicious_links` and `warn_new_account_days` are stored and audited but nothing reads them.**
Decision: enforce them with what exists, without a scanner. (a) `dm_spam_filter`: a direct message from a member who is not the recipient's friend lands in Message Requests (the `is_message_request` plumbing of D-181) when every server the two share has the filter on; the server setting can only tighten the recipient's own DM privacy setting, never loosen it. (b) `warn_new_account_days`: the same routing applies when the sender's account is younger than the largest `warn_new_account_days` of the servers the two share, and the request carries `sr_request_reason: "new_account"` (otherwise `"server_filter"`) so the client can show "This account is N days old" on the request card; account age comes from the snowflake, so no new user field. (c) `explicit_media_filter_dms`: no image classifier ships (none is pure Rust, none is accurate enough to act on, and LNG-SSET-054 only ever promised an operator-configurable scanner hook, default off). While the setting is on in any shared server, attachments in a message-request conversation are delivered with the spoiler flag set, so nothing explicit is shown before the recipient chooses; the scanner hook itself is not built until an operator asks for one. (d) `hide_suspicious_links`: in a server with the setting on, a link whose visible text names a different host than its target is rendered as its raw target with a warning, Discord's behaviour; the setting is exposed to members on the community block of the guild object so the web can apply it. Nothing here deletes a message or blocks a sender; these are routing and display rules.
Status: `ADOPTED` (D-224). Follow-up: G-95.

**P12-LCB-6. Scheduled events have no cover image, no recurrence, no `entity_id` for stage instances and no notification to interested members when an event starts.**
Decision: cover image yes, start notification yes, stage link yes, recurrence no. `image` (Discord's field: one JPEG, PNG or GIF up to 2 MiB as a data URI on create and patch, `MediaKind::EventCover`, CDN kind `guild-events`) is part of the Discord object every client renders. When an event becomes `ACTIVE`, every member who marked Interested gets the ordinary notification path (gateway dispatch to their sessions, Web Push under the D-191 rule) with the event name and a link; Discord does the same. Starting a stage event opens the stage instance on its channel with the event's name as topic and records its id as `entity_id`; ending the event closes the instance, and closing the instance ends the event (D-222). Recurrence (`recurrence_rule`) is skipped: it is a recent, complex Discord addition that a friend group can live without, and it would multiply the clock and notification rules; revisit when someone asks.
Status: `DEFAULT`. Follow-up: G-93.

**P12-LCB-7. Join-to-create: one room per member, at most 50 rooms per server, no lock or limit chat commands (the creator uses the normal channel editor through their overwrite).**
Decision: accepted as built. The overwrite is the Discord-native way to give a creator control, the two caps keep a prank from filling the channel list, and chat commands can arrive later as module slash commands if anyone misses them.
Status: `DEFAULT`. No follow-up.

### Lounge gateway and settings backend (P12-LGB, branch `feat/p12-lounge-gw-be`)

**P12-LGB-1. `activity.hide_age_restricted` defaults to true on the server (the web default and D-190); cinema tests that expect adult activity set it to false.**
Decision: confirmed. Hiding age-restricted titles from shared activity by default is the privacy-preserving reading of D-120 and D-190, and the web already assumed it; a default that differs between server and client would show a friend a title the member thought was hidden.
Status: `DEFAULT`. No follow-up.

**P12-LGB-2. `activity.share_tv` is stored and exposed by `activity_shares(uid, 'tv')`, but TV has no shareable server-side activity, so nothing reads it.**
Decision: keep it. The toggle is part of the D-190 set and the Activity Privacy section writes all five keys at once; removing one and re-adding it when Live TV presence lands would churn the settings blob for nothing. When TV watching becomes an activity (`SR_ACTIVITY_UPDATE` from the TV player), it reads this key like the other sections do; no toggle appears in the UI for a section that has nothing to share until then.
Status: `DEFAULT`. No follow-up.

**P12-LGB-3. Library rankings are still one anonymous global aggregate; there are no scoped (friends, "people you know") rankings to filter by the activity settings.**
Decision: build the scoped rankings so D-190 is true for the Library as it is for Cinema: `GET /library/rankings?scope=friends|known` next to the global one, counting only members who share their library activity (`activity_shares(uid, 'library')`), excluding age-restricted titles for members hiding them, with the same minimum-count floor the Cinema scopes use so a scope of two friends does not reveal what one of them reads. The global aggregate stays anonymous and unfiltered.
Status: `DEFAULT`. Follow-up: G-96.

**P12-LGB-4. Migration numbers: `0796_activity_settings.sql` on this branch collides with `0796_group_dm_invites.sql` on `feat/p12-lounge-ch-be`; which one is renumbered, and to what?**
Decision: the gateway branch renumbers. `feat/p12-lounge-ch-be` was handed `0796` by name for G-47 and its handoff lists it; `feat/p12-lounge-gw-be` picked `0796` for G-58 because no number was named and offered to move. `0796_activity_settings.sql` becomes `0797_activity_settings.sql` (header comment included); `sqlx::migrate!` refuses duplicate versions, so the two branches cannot both merge until this is done. The same check found a second collision: `0800_scheduled_events.sql` on the chat branch takes the number G-85 reserved for the plaintext signing-seed drop, which is not written yet, so G-85 moves to `0803_drop_plaintext_signing_seed.sql`. Numbers in use after this round: 0790 to 0797 and 0800 to 0803 for Phase 12 branches, 0810 to 0819 for the running TV and watch-party agent, 0820 to 0829 for this round's follow-ups. No further renumbering.
Status: `DEFAULT`. Follow-up: G-89.

**P12-LGB-5. API-shape notes: message flag `SYSTEM_TAG = 1 << 30` is a SIDE ROOM extension set when the message object is built and never stored; push suppression treats a session that never sent op 3 as active, and `dnd` and `invisible` sessions as active too.**
Decision: both confirmed. Bit 30 is outside Discord's assigned message flags, is derived from the author (so nothing can forge it by storing a flag) and is what D-188 asked for; doc 05 names it. For push, a session that has not said it is idle is being used, and `dnd` and `invisible` are statuses the person chose while present; treating any of them as "away" would push a notification to a phone next to the open laptop, which is what D-191 exists to stop.
Status: `DEFAULT`. Follow-up: G-98 (doc rows only).

**P12-LGB-6. Implementation notes: `Member.extras` and `GuildMembership.extras` are boxed; the system user id is the constant `sr_chat::SYSTEM_USER_ID = 1` for the guards while the SQL filters use the `FLAG_SYSTEM` account flag; G-52's validation is slug and range based until the G-48 catalogue is plugged into `admin_rules.rs::apply_extras`.**
Decision: fine as built; the catalogue hook-up is integration work between the two branches (G-48 landed on `feat/p12-lounge-ch-be`), so whoever merges second plugs `sr_chat::profile::catalog` into `apply_extras` and drops the slug check.
Status: `NO-CODE`.

### Still with the owner from earlier rounds

Unchanged: the UnRAR licence confirmation (D-170) and the eight NEEDS-OWNER items of the Phase 8 to 11 round listed in [open-questions.md](open-questions.md); this round adds none.

## Phase 12 and 13 remaining questions (answered 2026-10-15)

Every owner question, "still open" judgement call and deviation left in the handoffs that the two Phase 12 rounds did not reach, plus the Phase 13 handoffs written so far: on `main`, `docs/dev/handoff/feat-p12-tv-wp-be.md`, `feat-p12-g68-be.md` and `feat-p12-integration.md` (the other five Phase 12 handoffs were answered as P12-AUI to P12-LGB and raise nothing new); on their branches, `feat-p13-web-fe.md`, `feat-p13-mfa-protect.md`, `feat-p13-events.md` and `feat-p13-library.md`. Also the two items of [open-questions.md](open-questions.md) "Human actions" 4 that are decisions rather than actions (the Content-Security-Policy origins and the entrance muted-text colour); everything else listed there is legal, money, artwork, an external account or a human action and stays. Same rules as the earlier rounds: [01-decisions-log.md](01-decisions-log.md) wins; Discord parity by default; privacy and safety per docs 09 and 15; Rust first per doc 03; conservative security defaults; one Owner running a friend-group instance on one node (doc 02 section 5).

Two of these were decided by the owner in chat today and are recorded as given: `foliate-js` is approved for the EPUB reader (P13-LIB-1) and "Share to Lounge" posts a card into a channel the person picks (P13-LIB-2). Status key as before: `ADOPTED` = a product decision, recorded as D-225 onward; `DEFAULT` = engineering default, implement without the owner; `NEEDS-OWNER` = needs the human, with the default that applies meanwhile; `NO-CODE` = informational or already resolved. Follow-ups are H-01 onward in [dev/answers-followups-p13.md](dev/answers-followups-p13.md).

| Status | Count |
|---|---|
| ADOPTED | 9 |
| DEFAULT | 15 |
| NEEDS-OWNER | 0 |
| NO-CODE | 13 |

37 questions in all. Nothing in this round needs the owner: no legal, money, artwork or external-account question was raised, and the two owner decisions given today are recorded, not re-decided. The eight NEEDS-OWNER items of the Phase 8 to 11 round and the UnRAR confirmation stand unchanged in [open-questions.md](open-questions.md).

### Live TV, watch parties, donations backend (P12-TVW, handoff `feat-p12-tv-wp-be.md`)

**P12-TVW-1. Should TV reminders have a user setting for the lead time and a "my reminders" list? Today: per-request lead only, and the popover cannot show an already-set reminder after a reload.**
Decision: both, small. A settings-blob key `tv.reminder_lead_minutes` (0 to 120, default 5) is the lead used when `PUT /tv/reminders/{block_id}` is called without `?lead_minutes=`; the Live TV settings card renders it. `GET /tv/reminders` (session only, bots denied like the other two) lists the caller's pending reminders as `[{block_id, channel_id, title, starts_at, remind_at, lead_minutes}]`; the guide popover reads it so "Remind me" shows as set after a reload and offers "Remove reminder", and the Live TV page gets a small "My reminders" list over the same route. No new delivery path: the reminder stays the system-user DM that P8-TV-2 and the handoff describe.
Status: `DEFAULT`. Follow-up: H-01.

**P12-TVW-2. Should a Watch party room be able to start empty (no title), so the room can exist before anyone picks something? That needs a nullable media on `watch_parties`.**
Decision: yes. A voice channel exists before anyone talks in it, and a room preset that demands a title at creation (with the creator leaving the party again so the next joiner hosts) is a workaround for a schema rule, not a feature. A persistent party bound to a voice channel may have no media: the preset no longer asks for a title, the party row is created with `title_id`, `episode_id` and `asset_id` null and state `idle`, and `WATCH_PARTY_UPDATE` and `GET /watch-parties/{id}` carry `media: null`. The first person to join an idle room becomes host and the client opens the title picker (the existing change-media dialog); choosing media moves the party to the usual paused state. When the last participant leaves, a persistent room keeps whatever media it had (as today). Ad-hoc parties (not bound to a channel) still need media at creation, enforced by a check constraint, so nothing changes for `POST /watch-parties`. The play gate, limits and the age rule run when media is chosen, as they do on change-media today.
Status: `ADOPTED` (D-227). Follow-up: H-02.

**P12-TVW-3. Live party media identifies the channel by `title_id` and `asset_id` rather than a new column, so clients must read `media.live` first; a restricted programme that starts after the party began is gated at tune time, not at party level.**
Decision: accepted as built. A column that is null for every non-live party buys nothing over the `live` flag, and ported clients that ignore `live` would not know what to do with a channel id in a new column either. Tune-time gating is the right place for a programme that becomes restricted mid-party: the party state machine does not know the schedule, the tune route does, and the person who is refused sees the same age-gate answer as outside a party. The watch-party spec says both things in one sentence.
Status: `DEFAULT`. Follow-up: H-26 (doc rows only).

**P12-TVW-4. G-07: the `SR_DOWNLOADS_LEGACY_FALLBACK` flag and the `cinema_download_grants` table are still there; removing them is "next phase's job".**
Decision: this is that phase. The flag has defaulted to off since Phase 12 and the entitlement path is the only one exercised; remove the flag and the fallback code, and drop the table with a guarded migration (it raises if any grant row is newer than the entitlements backfill, so an instance that never ran Phase 12 is told to do so first). The `deploy/RUNBOOK.md` note changes from "check Admin → Entitlements first" to the upgrade order.
Status: `DEFAULT`. Follow-up: H-03.

**P12-TVW-5. WP-FE-008's "message card" entry point: the in-chat party card does not exist, so the box stays open.**
Decision: the entry point now exists elsewhere. `feat/p13-web-fe` built the Cinema and Library rich cards in chat (LNG-MSGUI-018) with a "Watch together" button, which is the message-card entry point WP-FE-008 names; once that branch merges the box is ticked with a note. A separate "party in progress, join" card posted into a channel is not built: a running party already appears as the host's `party.id` activity, which the member list and profile show with a Join button, and the channel-bound room is visible in the channel list.
Status: `NO-CODE`. Follow-up: H-26 (inventory note).

**P12-TVW-6. `0781` was the agent's pick for the leaderboard migration and must not collide with other branches.**
Decision: resolved at integration; the merged tree lists 0781 once (`feat-p12-integration.md`). This round assigns `0870` to `0879`, so nothing new can collide with the Phase 12 and 13 numbers already taken (0810 and 0811 on `main`, 0820 on `feat/p13-events`, 0825 on `feat/p13-mfa-protect`, 0830 to 0832 on `feat/p13-library`).
Status: `NO-CODE`.

### Phase 12 follow-ups backend (P12-G68, handoff `feat-p12-g68-be.md`)

**P12-G68-1. Should Approve from the `#review-queue` card need a fresh second factor? `POST /admin/review/.../approve` asks for none today, so the card asks for none either.**
Decision: no fresh factor, on the card or the route; they must agree, and the route is right. A staff session exists only for an account with an enrolled second factor (D-198), the admin re-auth window already covers the pages that reach account data and private content (D-125, doc 07 section 5), and approving a submission publishes a reviewed item to members, which is reversible (unpublish, `DELETE_CONTENT`) and leaves a `review.approve` staff-log row naming the presser. A second factor on every approve would train staff to type codes reflexively, which is the opposite of what re-auth is for. Reject keeps going through the review page because a reason is required (D-082), not for security.
Status: `DEFAULT`. No follow-up.

**P12-G68-2. Is `900122` the right number for the tester re-invite cooldown, and should it be 409 or 403?**
Decision: both right. Integration gave `MFA_ENROLLMENT_REQUIRED` the next number (`900123`), so `900122 TESTER_RECENTLY_DECLINED` stands. 409 is the right status: the caller is allowed to invite and the request is well formed; it is the state of the pair (a decline less than 24 hours old) that refuses it, which is what 409 says, and 403 would make a client treat a cooldown as a permission problem. Doc 05 lists the code with its status.
Status: `NO-CODE`.

**P12-G68-3. Should the review card show thumbnails and a Request changes button (REV-RV-005 asks for them)? Today: title, kind, submitter, item count, Approve, Reject and a link.**
Decision: yes to both, small. One thumbnail: the submission's cover (Library) or poster (Cinema) as the card embed's image through a signed CDN URL minted for the Staff Server message, or no image when the submission has none; not a page strip, which would put unreviewed content into a chat channel. A third button `sr:review-changes:<submission id>` that, like Reject, changes nothing on the card and answers a link to `/admin/review?submission=<id>&changes=1`, which opens the review page's request-changes dialog (`sr-review` already has the `changes_requested` state and a reason is required there too). The card rewrites to its outcome for this transition as for the others.
Status: `DEFAULT`. Follow-up: H-04.

**P12-G68-4. Deviations to check: `POST /library/submissions/{id}/cover` keeps a literal path unlike the other `/submissions/{id}/library/*` member routes and takes a JSON data URI; `member_capabilities` is a sibling map on `GET /guilds/{id}/modules`; the announcement patch accepts `message` and `ends_at` as aliases; `SR_FEATURE_UPDATE` is published from every write path; the card shows an item count, not bytes.**
Decision: all accepted as merged. The cover route is published in the OpenAPI document and the client; renaming it now costs a breaking change for a cosmetic alignment, and the data-URI body is Discord's own convention for images. The sibling map keeps the `modules: {id: bool}` contract that ported code reads. Aliases cost nothing and the canonical names (`body`, `ends_at_ms`) are the ones documented. Publishing from the service is the correct seam (a flag written by the CLI must reach clients too). The item count is what exists; a byte size per submission can join the card when the review model records one.
Status: `NO-CODE`.

### Phase 12 integration (P12-INT, handoff `feat-p12-integration.md`)

**P12-INT-1. Not verified at integration: Playwright (shared ports), the whole vitest suite after the i18n key move, `cargo deny` after the last merge.**
Decision: these are what the first green CI run is for ([open-questions.md](open-questions.md), human action 3): `e2e.yml` runs Playwright, `ci.yml` runs the whole vitest suite and `cargo deny` on the merged tree. No agent should try to run Playwright on the shared machine. Nothing to decide.
Status: `NO-CODE`.

### Web front-end (P13-WFE, branch `feat/p13-web-fe`)

**P13-WFE-1. Should the welcome dialog also cover accounts created before this release? Currently no: only accounts younger than 14 days.**
Decision: no. The dialog's three steps (theme, picture, second-factor nudge) are first-run choices; a member who has used the place for a year has made them, or has decided not to, and a modal on their next visit is noise with a "we changed something" feel that nothing else justifies. The second-factor nudge for older accounts is the one step with value, and it has a better home: the notices bar already carries security notices (the update notice, D-192's announcements), so a one-time dismissable "Protect your account with a second factor" notice for accounts without TOTP or a passkey covers it without a modal; that is a line in the existing bar, not a dialog. The 14-day window stays and is measured from the platform snowflake epoch, which the branch does correctly (see P13-MFA-5 for the other epoch).
Status: `DEFAULT`. Follow-up: H-21 (the notice line).

**P13-WFE-2. Should oversize files be blocked client-side once the effective per-channel limit is exposed by the API? Today the tray only warns, from `GET /donations/me`, because server boosts can raise the limit inside a server.**
Decision: yes, block, and expose the limit where the client already looks. Discord refuses an oversize file in the tray before a byte is sent; uploading 300 MiB to be told 413 is the worst version of the same answer. The effective limit is a per-caller number on the objects the client holds: `sr_max_upload_bytes` on the guild object (`GET /guilds/{id}`, `GUILD_CREATE`, `GUILD_UPDATE`; the larger of the member's own perk and the server's entitlement level) and on DM and group channels in `GET /users/@me/channels` and READY `private_channels` (the perk alone). The tray blocks a file over the limit with the existing Patron hint and the amount, keeps the 413 handling for the race where an entitlement lapses mid-upload, and stops reading `/donations/me` for this. `SR_ENTITLEMENT_UPDATE` already reaches the sessions whose limit changed; the client refetches the guild on it.
Status: `ADOPTED` (D-233). Follow-ups: H-05 (backend), H-22 (web).

**P13-WFE-3. Backend contract 1: user-level safety settings (LNG-USET-012, 013, 015): `sensitive_media {friends, others, servers: show|blur|block}`, a DM spam filter, per-server DM overrides, and `GET|DELETE /users/@me/age-confirmation`. Today only the per-server `explicit_media_filter_dms` and `dm_spam_filter` exist.**
Decision: build them on `PATCH /users/@me/privacy` with Discord's shapes, and be explicit about what they act on, because nothing scans (D-224). (a) `sensitive_media: {friends, others, servers}` with `show | blur | block`, defaults `show | blur | show` (Discord's defaults). Without a classifier the signal is what the platform knows: for `others`, every image and video attachment in a DM from someone who is not the viewer's friend; for `servers`, every image and video attachment in an age-restricted channel; `friends` has no signal today and the setting says so under the switch ("applies when a media scanner is configured"), so it is stored and honoured the day the LNG-SSET-054 scanner hook exists. `blur` is delivered to that viewer as the spoiler flag; `block` is delivered as the attachment with `sr_filtered: true` and no `url` or `proxy_url`, so the viewer sees a "Blocked by your sensitive media settings" chip and can relax the setting, and nothing is deleted for anyone else. Both are per-viewer presentation, computed where the message object is built for a session (the age gate of D-199 already does per-session work there). (b) `dm_spam_filter: "off" | "non_friends" | "all"` (Discord's three), default `non_friends`: `non_friends` sends a first DM from a non-friend to Message Requests, which is the existing D-181 and D-224 routing and is why the key is a level and not a boolean; `all` sends every first DM there, friends included; `off` only applies where no shared server forces the filter (D-224: a server setting can tighten, never loosen). (c) `server_dm_overrides: {<guild_id>: bool}` is Discord's per-server "allow direct messages from server members" next to the existing default; a `false` for a server means a member known only through that server cannot open a DM (`50007`, as Discord), and the server member list's DM button greys out. (d) `GET /users/@me/age-confirmation` returns `{confirmed_at, method}` and `DELETE` clears `age_confirmed_at` (behind re-auth, since it is what unlocks age-restricted content; `SR_USER_UPDATE` follows so the gateway cache of D-199 drops), which is the "re-confirm" flow LNG-USET-015 wants. All four live on `user_privacy` (columns, not the blob: the server enforces them).
Status: `ADOPTED` (D-230). Follow-ups: H-06 (backend), H-23 (web).

**P13-WFE-4. Backend contract 2: burst reactions (LNG-EXP-012): `PUT .../reactions/{emoji}/@me?type=1`, `burst` and `count_details` on the reaction object, an allowance, gateway events carrying `burst`.**
Decision: not built; a parity shim only. Super reactions are a Nitro upsell with a per-week allowance and animation packs; making Discord's paid features free here (the server tag, doc 05 section 4) is a rule about not charging for what we build, not an obligation to build every paid cosmetic, and this one adds a second counter to every reaction row, a new field on four gateway events, an allowance table and animation assets (artwork) for a burst of sparkles a friend group will not miss. What ported bots need is the object shape: the reaction object carries `burst: false`, `me_burst: false`, `count_details: {normal: <count>, burst: 0}` and `burst_colors: []`, `?type=1` on add and remove is accepted and treated as `0`, and `GET .../reactions/{emoji}?type=1` answers an empty list. LNG-EXP-012 is re-scoped to the shim and ticked when it lands; the picker variant and animations are closed, not deferred.
Status: `ADOPTED` (D-231). Follow-up: H-07.

**P13-WFE-5. Backend contract 3: the emoji popout's "Join server" (LNG-EXP-011) needs `GET /emojis/{id}` for non-members with the source server, and a signed image URL for emoji of servers the viewer is not in; today cross-server emoji stay `:name:` text.**
Decision: yes to both, with one privacy line. An emoji in a message is content the author chose to show everyone who can read the message, so its image is drawable by every signed-in member wherever it appears, as on Discord: `GET /sr/emojis/{emoji_id}/url?size=&format=` mints the same one-hour signed variant URL as the guild route for any session (the emoji id alone does not say which server it came from). The source server is a different matter: `GET /sr/emojis/{emoji_id}` answers `{id, name, animated, guild: {id, name, icon, listing}}` with `guild` filled only when the viewer is a member or the server is listed in the Server Directory (`listing` carries the directory entry id, which the popout turns into "Join server" through the directory's own join route); otherwise `guild: null` and the popout shows name and image only. A private server's name is not revealed by one of its emoji travelling into a DM. The web's `customEmoji` resolver takes over every custom emoji node, not only the message's own server, and the `:name:` fallback stays for an emoji that no longer exists.
Status: `ADOPTED` (D-232). Follow-ups: H-08 (backend), H-24 (web).

**P13-WFE-6. Backend contract 4: operator copy (ENT-UI-010, ENT-UI-025): public `GET /sr/entrance/copy` for `no-invitation`, `house-notes` and landing strings, and `PUT /admin/entrance/copy/{key}` with defaults served by the client.**
Decision: build it, with a fixed key list and no new table. Keys: `no-invitation`, `house-notes`, `landing-note` (the one optional operator paragraph on the landing page; the landing's other strings stay in the catalogue, since translated copy and operator copy do not mix well). `GET /sr/entrance/copy` is public, cacheable (`ETag`, one minute) and rate limited like the invite check, and answers `{key: markdown}` for keys the operator set; the client renders its shipped defaults for the rest, so a fresh install needs nothing. `PUT /admin/entrance/copy/{key}` (`MANAGE_FEATURES`, re-auth, audited as `entrance.copy`) stores markdown of at most 16 KiB in `platform_settings` under `entrance.copy.<key>`, validated by `sr-markdown` with the entrance profile (no mentions, no custom emoji, links allowed), and `DELETE` restores the default. Admin → Entrance renders the three editors with a preview.
Status: `DEFAULT`. Follow-ups: H-09 (backend), H-25 (web).

**P13-WFE-7. OpenAPI naming collision: `PUT /users/@me/connections/link` references schema `LinkBody`, which resolves to the Cinema staff `LinkBody`; the web client uses the real fields.**
Decision: a bug, fix it in the generator's favour: the connections struct gets `#[schema(as = ConnectionLinkBody)]`, the OpenAPI document and client are regenerated, and the OpenAPI test gains a check that no two Rust types register the same schema name, so the next collision fails `cargo test` instead of reaching the client silently.
Status: `DEFAULT`. Follow-up: H-10.

**P13-WFE-8. Decisions taken on the branch: custom emoji resolve only through the message's own server; the upload size check only warns and reads `/donations/me` lazily; the welcome dialog uses the platform epoch; `dmToasts`, `welcomeDone` and `ttsRate` are new synced prefs with the key-order rule in `sanitizePrefs`.**
Decision: confirmed, except where P13-WFE-2 and P13-WFE-5 change them (the check blocks once the limit is on the guild object, and the resolver covers every server once the url route exists). The three prefs are client presentation and belong in the synced prefs blob, not in server-enforced settings.
Status: `NO-CODE`.

### Server 2FA scope and Protection settings (P13-MFA, branch `feat/p13-mfa-protect`)

**P13-MFA-1. Gating runs before the permission check on the older guild routes (kick, ban, roles, channels, server edit), so a member without the permission and without a second factor sees `900123`; the `settings/*` routes check the permission first.**
Decision: align the older routes to permission first, everywhere. Discord answers the missing permission (`50013`) before the two-factor requirement, and so should we: a member who may not kick is told they may not kick, and `900123` is reserved for people who could act once enrolled, which is what its enrolment prompt (G-97) assumes. The order also keeps the gate from reading `mfa_level` and the enrolment state for callers who were never going to pass. Same `MfaGate`, called after `require_permission`.
Status: `DEFAULT`. Follow-up: H-11.

**P13-MFA-2. Member nickname edits (`MANAGE_NICKNAMES`, `CHANGE_NICKNAME`) are not gated; role assignment and timeouts on the same route are.**
Decision: confirmed. Neither nickname bit is on Discord's elevated list and D-223 names the list; `PATCH /guilds/{id}/members/{id}` is gated by the fields it carries (`roles`, `communication_disabled_until`), which is the right granularity.
Status: `NO-CODE`.

**P13-MFA-3. `900123` for a session with no second factor at all replaces the old `900004` there.**
Decision: confirmed; that is the D-223 text, and the web splits on it (enrolment modal versus re-auth dialog).
Status: `NO-CODE`.

**P13-MFA-4. The sender's `CHANNEL_CREATE` does not carry `sr_request_reason`, and the recipient only learns of a pending request through `GET /users/@me/channels` and READY.**
Decision: the first half is by design, the second is a gap. The sender is never told that their message landed in Message Requests: not in the channel object, not in `CHANNEL_CREATE`, not by a different `MESSAGE_CREATE`; Discord does not tell them either, and telling them would turn the recipient's safety settings into a signal to the person they are protecting against. The recipient, though, must learn live: a DM that opens into their requests dispatches `CHANNEL_CREATE` to the recipient's sessions with `is_message_request: true` and `sr_request_reason`, and the first `MESSAGE_CREATE` follows as for any DM. The client files both under Requests: no sound, no unread badge on the DM list, no Web Push (the Requests counter is the only indicator, as D-181's Message Requests already behave), and the request card shows the reason text the branch wrote. Accepting the request moves the conversation to the DM list with the ordinary notification rules from then on.
Status: `DEFAULT`. Follow-up: H-12.

**P13-MFA-5. `lounge/liveMap.ts` `snowflakeTime` uses Discord's epoch (1420070400000) while the server uses `SIDEROOM_EPOCH_MS` (1767225600000); `requestReason.ts` uses the server's. An existing bug, not touched.**
Decision: a bug, confirmed (every time derived from a snowflake in the Lounge is about eleven years early), and it is being fixed on another branch in this phase; not assigned here. The one rule to record: the platform epoch is `SIDEROOM_EPOCH_MS`, exported once from the Rust snowflake crate through WASM or a generated constant, and no TypeScript file holds its own copy of either epoch. Doc 05 section 3 says so.
Status: `NO-CODE` (fixed elsewhere). Follow-up: H-26 (doc row).

**P13-MFA-6. No automated test for the thread gate (the thread router is not mounted in the `lounge` or `developer` suites); and the server 2FA list went into doc 07 section 2 (guild permission bits) rather than "section 4" as G-98 said.**
Decision: section 2 is the right home (section 4 is the Staff Server; the checklist was wrong). The thread gate gets its test by mounting the thread router in the `lounge` suite, which also covers the thread edit, archive and delete rules that have had no route-level test since Phase 10.
Status: `DEFAULT`. Follow-up: H-13.

### Scheduled events (P13-EVT, branch `feat/p13-events`)

**P13-EVT-1. Should the creator of an event be notified without marking interest? Currently no: start notifications go to members marked interested.**
Decision: the creator is marked Interested when the event is created, as Discord does, and can un-mark themselves like anyone else; the notification then needs no special path. `user_count` includes them, `GUILD_SCHEDULED_EVENT_USER_ADD` is dispatched for the creator on create, and an event created by a bot marks nobody.
Status: `ADOPTED` (D-228). Follow-up: H-14.

**P13-EVT-2. Voice occupancy and stage instances live in each gateway node's memory; the tick runs on one node and sees only that node's voice states, so a multi-node deployment could end an event after 15 "empty" minutes while people are connected through another node. Also `reap_stale` (jobs stuck `running` after a crash) is never called in production.**
Decision: no shared voice-state store now (P10-VBE-2's answer stands: persistence pays off with the multi-node SFU, the same milestone as sharding), but the rule must fail safe rather than end someone's event. `FeatureActions::voice_occupants` learns to say "unknown": it answers `Occupancy::Known(users)` on a node that holds the channel's voice state and `Occupancy::Unknown` when the deployment has more than one gateway node (the bus implementation knows; the `InProcessBus` is always `Known`), and the empty-room rule only ends an event on `Known` and empty. On one node nothing changes; on several the end time and manual End still work and a room is never ended for an emptiness nobody verified. `reap_stale` is called at startup and once an hour by the job runner with a 30-minute threshold, so a job left `running` by a crash is retried instead of lost; the events tick does not depend on it, as the branch says, but other jobs do.
Status: `DEFAULT`. Follow-up: H-15.

**P13-EVT-3. Stage events started before this change (status `ACTIVE`, no `entity_id`) are not tied to a stage instance; starting a stage event whose channel already has a stage answers 400 `GUILD_SCHEDULED_EVENT_STAGE_BUSY` (from a voice join the event just stays scheduled); the spec says `scheduled_events.image_key` but the table is `guild_scheduled_events`; one migration `0820` holds all three columns and 0821 to 0824 are unused.**
Decision: all accepted. Old active stage events still end at their end time or after 15 empty minutes (the voice rule applies to stage channels too), so nothing is stuck. 400 for a busy stage matches Discord's 150006 class of answer, and staying scheduled on a join is the conservative choice (a join should not hijack another stage). The checklist and doc 04 use the real table name. Unused numbers stay unused; nothing is ever renumbered into them.
Status: `NO-CODE`. Follow-up: H-26 (name correction).

**P13-EVT-4. The web treats only `type === 1` as a group invite; the Events dialog's Cover toggle shows the picker to creators and `MANAGE_EVENTS` holders only.**
Decision: confirmed. An older server that omits `type` is a guild invite (D-221 makes `type` required going forward); the cover picker follows the edit permission the route enforces.
Status: `NO-CODE`.

### Library (P13-LIB, branch `feat/p13-library`)

**P13-LIB-1. May `foliate-js` (npm, a download) be added for the EPUB reader (LIB-RDR-010)?**
Decision: approved by the owner (chat, 2026-10-15), recorded here, not re-decided. `foliate-js` is the engine doc 03 section 3 and the reader spec already name; it is MIT, pure browser code, and loads as its own lazy chunk under the library bundle budget. The reader keeps the server out of it: the EPUB is served as the kept file (`sr-library` validates with the `epub` crate at ingest), and progress is the EPUB CFI, stored as a string next to the page index on the chapter-progress route so Continue and the Lounge Reading activity work for books as for chapters. The dependency is pinned, added to the web licence inventory, and `cargo deny` is unaffected (nothing Rust changes).
Status: `ADOPTED` (D-225, owner decision). Follow-ups: H-16 (backend), H-19 (web).

**P13-LIB-2. Should "Share to Lounge" post a card into a chosen channel (needs a channel picker and a poster port), or is the copied link enough? Today it copies a link, as the Cinema title page does.**
Decision: the owner decided (chat, 2026-10-15): it posts a card into a channel the person picks. No poster port is needed: the card is the `<library:series_id>` (or `<cinema:title_id>`) mention of the chat spec, which already renders as the rich card with Read or Watch and "Watch together", posted as an ordinary message by the person through `POST /channels/{id}/messages` from their own session, with an optional comment above it. The picker lists the channels (across the person's servers, plus DMs and groups) where they hold `SEND_MESSAGES` and `USE_LIBRARY_EMBEDS` (doc 07 bit 58), remembers the last choice, and "Copy link" stays as a second item for everything else. The same dialog replaces the Cinema title page's copy-link, so both sections behave the same. The message is the person's own: attributed to them, editable and deletable by them, subject to the channel's permissions and slowmode like any message.
Status: `ADOPTED` (D-226, owner decision). Follow-up: H-20.

**P13-LIB-3. Should reading presence get an "everyone who shares a server" level like the Cinema's `share_watching`? The branch chose friends only.**
Decision: yes, for symmetry: one rule for both sections is easier to explain than two. `library.share_reading` in the settings blob with `off | friends | everyone`, default `friends`, next to `cinema.share_watching` and under the same master switches (`activity.share_library` off or `activity.paused` sends nothing, D-190); `everyone` means friends and people who share a server, never strangers (doc 09 section 3); age-restricted series stay masked beyond friends as the Cinema masks titles. The Activity Privacy section shows the two levels side by side.
Status: `ADOPTED` (D-229). Follow-up: H-17.

**P13-LIB-4. No sweeper for old export objects under `library/exports/` (an hourly cleanup of objects older than 7 days is suggested); reading presence rows are not swept by a job (reads ignore expired rows).**
Decision: both go into the existing hourly `library.gc` job. Export objects older than 7 days since last use are deleted (they are rebuilt on demand and cached by fingerprint, so nothing is lost); expired reading-presence rows are deleted in the same pass, because a row saying who read what, kept after it stopped being shown, is activity data that doc 09 says to keep no longer than needed, and the Cinema sweeps its own.
Status: `DEFAULT`. Follow-up: H-18.

**P13-LIB-5. Web Push for new chapters goes to every follower with the bell on; there is no separate per-user "push for library" switch; non-UTF-8 ZIP entry names decode as lossy UTF-8 in the browser pre-check but as CP437 on the server.**
Decision: accepted. The bell is the per-series control and the browser subscription the per-device one, which is Discord's two-level model for server notifications too; a third switch would be one more place to look for why a push did or did not arrive. The entry-name difference only affects labels in the pre-check, never the pages the server extracts.
Status: `NO-CODE`.

**P13-LIB-6. Implementation notes: exports are built in the request (600 pages, 200 MiB); the known set is computed per query; rankings are cached in memory (global one hour, scoped ten minutes per member) instead of a scheduled job; `LibraryEvents` gained a defaulted `emit_activity`.**
Decision: fine as built. In-request export within those caps is simpler than a job plus a poll for a Patron-only feature with a 24-hour allowance; an in-process cache matches doc 03 (`moka`, no Redis) and one node.
Status: `NO-CODE`.

### Items from open-questions.md that were decisions, not actions (OQ-XC)

**OQ-XC-3. Decide the voice and activities origins for the Content-Security-Policy (`apps/web/src/security/csp.ts` and `Caddyfile.prod` must change together; `connect-src` and `frame-src` are `'self'` today).**
Decision: no separate voice origin, and the activities origin only when activities are on. Voice signalling rides the gateway WebSocket and the embedded SFU is reached by WebRTC, which CSP does not govern (TURN is not a fetch), so `connect-src 'self'` is complete for v1; `extraConnect` stays for an operator who runs the SFU on another host later, fed from `voice.public_url` in the server config. Activities run on `https://*.activities.<host>` (doc 05 section 1, doc 12 step 1) and are never trusted (doc 15 section 3), so `frame-src` gains that pattern only when `activities.enabled` is true, generated from `public_url`; a build with activities off keeps `frame-src 'self'`. The Caddyfile template reads the same two settings, so the two files cannot drift, and the pinned CSP test covers both states.
Status: `DEFAULT`. Follow-up: H-27.

**OQ-XC-6. Entrance muted text uses a lighter local colour than `--sr-text-3` to pass WCAG AA contrast; confirm the look, or raise `--sr-text-3` itself (a token calibration decision).**
Decision: keep the local colour; do not touch `--sr-text-3`. The owner confirmed the token calibration (OQ-25 to OQ-28), the token is used across the Lounge where its contrast is fine against the Lounge backgrounds, and the entrance has its own background. The entrance keeps a scoped `--sr-entrance-text-muted` token in `entrance.css` with the comment that names the measured ratio; doc 10 lists it as the one scoped text token.
Status: `DEFAULT`. Follow-up: H-26 (doc row and the token name).

### Still with the owner from earlier rounds

Unchanged: the UnRAR licence confirmation (D-170) and the eight NEEDS-OWNER items of the Phase 8 to 11 round (P8-DON-4, P11-MSG-1, P11-SHL-3, P11-CLI-1, P11-USB-2, MOD-4, MOD-5, MOD-6); this round adds none, and [open-questions.md](open-questions.md) lists them with their defaults.

## Phase 13 developer platform and settings questions (answered 2026-10-15)

The owner questions, "still open" judgement calls and deviations in the four Phase 13 handoffs written after the previous round: on their branches, `docs/dev/handoff/feat-p13-devplat.md` (SDKs, webhooks, App Directory, portal tabs), `feat-p13-settings-admin.md` (safety settings, admin people and servers, entrance copy, upload limit), `feat-p13-web-ui2.md` (the web half of the same contracts) and the one item of `feat-p13-library.md` that P13-LIB-1 to P13-LIB-6 did not reach. Same rules as the earlier rounds: [01-decisions-log.md](01-decisions-log.md) wins; Discord parity by default; privacy and safety per docs 09 and 15; Rust first per doc 03 (D-203 included); conservative security defaults; one Owner running a friend-group instance on one node (doc 02 section 5). Status key as before: `ADOPTED` = a product decision, recorded as D-234 onward; `DEFAULT` = engineering default, implement without the owner; `NEEDS-OWNER` = needs the human, with the default that applies meanwhile; `NO-CODE` = informational or already resolved. Follow-ups are H-28 onward in [dev/answers-followups-p13.md](dev/answers-followups-p13.md); the one migration of this round is `0874`.

| Status | Count |
|---|---|
| ADOPTED | 7 |
| DEFAULT | 7 |
| NEEDS-OWNER | 0 |
| NO-CODE | 4 |

### Developer platform (P13-DEV, branch `feat/p13-devplat`)

**P13-DEV-1. `sr-sdk` depends on `reqwest` directly and was added to the `wrappers` list in `deny.toml` instead of being written over a `SafeClient`-style restricted client (D-203). The SDK talks to the server its operator names, not to user-supplied addresses; the transport is a trait, so swapping is easy. Confirm, or ask for the restriction.**
Decision: confirmed, with the rule written down rather than implied by a comment. D-203 exists so that nothing inside the server fetches an address a user typed; `sr-sdk` never runs inside the server, it is compiled into a bot an operator runs against a base URL they configured, and the only addresses it ever contacts are that base URL and what the server hands back for the gateway (`url` from `GET /gateway/bot`, `resume_gateway_url` from READY). The second group is the one gap: a server a bot connects to is trusted by its operator, but a resume URL is still an address the SDK did not configure, so the SDK follows a gateway or resume URL only when its host equals the configured base host (or a host the operator listed in `Client::builder().allow_gateway_host`), and falls back to the configured gateway URL otherwise; it never fetches an `avatar_url`, `icon` or embed URL from a message (there is no helper for that, and the docs say so). `SafeClient` stays for the server; `deny.toml`'s comment cites D-234 instead of "p13 handoff, owner question", and doc 03 lists `sr-sdk` as the one library crate with a plain `reqwest` dependency.
Status: `ADOPTED` (D-234). Follow-up: H-28.

**P13-DEV-2. Webhook author identity: `author.id` on a webhook message is the webhook id (Discord parity) while `messages.author_id` stays the creator; the creator's permissions applied at send time; if the creator is deleted or loses access, existing webhook messages stay. Also the avatar override is only shown through the image proxy, and nothing is returned without a configured signer.**
Decision: all three accepted as built, and recorded because moderation tooling must know the split. Discord renders the webhook as the author and keeps no creator on the message at all; keeping `messages.author_id` as the creator is the better data model here because the permission check that allowed the post (and D-223's second-factor rule) belongs to a person, not to a token. What follows from it: the member drawer's message history, the moderation case view and a data export list webhook messages under the creator with a "via webhook `<name>`" marker, not as their own words; `MESSAGE_DELETE` and the audit row for a deletion name the webhook message as such; the creator's departure or deletion leaves the rows and the rendered name (the side table has no foreign key, by design). The avatar override through the image proxy is doc 09's rule for every remote image (a viewer never contacts a host a webhook caller chose), and no signer means no avatar, not a raw URL. One gap closes with it: `message_webhook_authors` rows must go when their message is purged (P13-DEV-6).
Status: `ADOPTED` (D-235). Follow-ups: H-29 (purge), H-30 (the "via webhook" marker in staff views and export).

**P13-DEV-3. App Directory categories are the applications' tags (up to 5 each), no new category field; only `released`, non-suspended apps with `bot_public` are listed; no review of the listing text beyond the release review.**
Decision: accepted. Discord's directory categories are also a fixed tag vocabulary on the application, and a second taxonomy for a directory a friend group browses would be maintained by nobody. The listing rule is right: `released` is the state the review granted, `bot_public` is the developer's own choice to be found, and suspension is the one lever staff need (`REVIEW_APPS` suspends, which removes the listing at once; no separate "hide from directory" flag). Edits to the description, tags or icon after release are shown without re-review, as on Discord; what the release review checked (scopes, permissions, intents, the interactions endpoint) cannot change without a new version going through it. Searching the description is fine (`LIKE` with escaped wildcards, as built); ranking is by name, nothing is boosted.
Status: `ADOPTED` (D-236). No follow-up.

**P13-DEV-4. Application emoji and Rich Presence assets are managed only from the owner's session, not by bots (Discord lets bots manage application emoji). Also: application emoji cannot be used yet (no `<:name:id>` rendering, clients do not draw them).**
Decision: parity for emoji, owner-session for assets, and make the emoji usable, because a tab that stores images nothing can draw is a trap. Discord's `/applications/{id}/emojis` routes accept the application's own bot token, and that is safe here: the bot can only touch the emoji of the application it belongs to, the upload is sniffed, shrunk and capped as it is for the session, and an emoji is not a permission. So the emoji routes accept a bot principal whose application id is `{id}` (owner session or that bot, nobody else); the Rich Presence asset routes stay owner-session for writes, with the asset list readable by any session (that is Discord's split too: assets are uploaded in the portal and read publicly for presence cards). Rendering: a message or interaction response sent by the application's bot may use `<:name:id>` of its own application emoji (anyone else's message leaves it as text, as Discord does), `GET /sr/emojis/{id}/url` resolves application emoji for any session under D-232, and the popout says "Emoji of `<application name>`" with no join action. The Emojis tab's "cannot be used yet" note goes.
Status: `ADOPTED` (D-237). Follow-up: H-31.

**P13-DEV-5. Attachment options need `ATTACH_FILES` in the channel (the slot request checks it); Discord does too.**
Decision: correct, nothing to change. The file is a staged upload in that channel by the invoker; the option is only a way to hand it to a bot, and the bot's reply with the file goes through its own permission check.
Status: `NO-CODE`.

**P13-DEV-6. Follow-ups listed: `message_webhook_authors` rows are not removed when old messages are purged (they hold a name and a URL); uploads staged for an attachment option and never used stay as ordinary unattached upload slots.**
Decision: purge the rows; leave the slots. A purged message must leave nothing behind that says who posted what under which name, and the URL a caller supplied is exactly the kind of row doc 09 says to drop with its message, so `purge_deleted_messages` deletes the side-table rows for the doomed ids in the same statement batch, and the account-deletion path that removes a person's messages does the same (the message ids are known there). The unused attachment-option slots already have the lifetime of an abandoned composer upload (expiry and sweep), which is the right one.
Status: `DEFAULT`. Follow-up: H-29.

**P13-DEV-7. Still open with reasons: DEV-BOT-007 bots in voice (needs WebRTC and an MLS client in the SDK), the DEV-PORT-003 Webhooks tab (Discord's event-subscription URL, no spec), the Activities tab and DEV-ACT-001 to 004 (proxy host, instances, embedded SDK), DEV-PORT-004 (pages are hand-written, not generated from OpenAPI), DEV-SDK-003 (needs discord.js, discord.py, serenity and twilight in CI), the SDKs have no presence update, voice, `REQUEST_GUILD_MEMBERS` or HTTP-interactions server.**
Decision: the reasons hold; two boxes change shape. The Webhooks tab is closed, not deferred: Discord's webhook events (`APPLICATION_AUTHORIZED`, `ENTITLEMENT_*`) are a monetisation and install-tracking feature with no counterpart in doc 11, and channel webhooks live in Server Settings where they belong; DEV-PORT-003 is re-scoped to "Emojis, Rich Presence, Activities" and stays open on the Activities tab alone, which lands with DEV-ACT-001. DEV-PORT-004 stays open and is the OpenAPI document's job: the portal reference pages are generated from `openapi.json` at build time once the api-docs pipeline exists (a web follow-up for that phase, not this one). DEV-SDK-003 stays open; adding discord.js, discord.py, serenity and twilight to CI is a download the owner approves in the usual way, and the recommended first slice is one library per ecosystem (discord.js and serenity) in a weekly, not per-PR, workflow. The SDK gaps are ordered: `REQUEST_GUILD_MEMBERS` and presence update first (both are gateway ops the server already serves), the HTTP-interactions server (D-185's signing) second, voice last with DEV-BOT-007. Bots in voice stay where the handoff put them.
Status: `NO-CODE`. Follow-up: H-32 (SDK gateway ops; inventory re-scope note).

### Settings and admin backend (P13-SA, branch `feat/p13-settings-admin`)

**P13-SA-1. Should `sensitive_media` be applied in the REST message object only first, or REST and gateway together? Per-viewer attachment filtering needs a viewer-aware pipeline on both.**
Decision: together, or not at all; a REST-only first step ships a setting that fails at the moment it matters. The message a person has not seen yet arrives over the gateway, so a blur that applies only to history shows the image on arrival and hides it on reload, which is worse than the honest "stored, not applied yet" the card says today. The pipeline is smaller than it sounds because D-230 gives it exactly two signals: an image or video attachment in a DM from a non-friend (`others`) and one in an age-restricted channel (`servers`); `friends` has no signal until a scanner exists. So the per-viewer step runs only for `MESSAGE_CREATE` and `MESSAGE_UPDATE` payloads that carry such an attachment in such a channel, and for the same messages on the REST reads, as one function in `sr-chat` (`sensitive::apply(viewer_prefs, relation, channel, &mut message)`) that both the REST decorator and the gateway's per-session serialisation call; the viewer's `sensitive_media` and friend relation are read from the caches the gateway already keeps per session and refreshed on `SR_USER_UPDATE` and `RELATIONSHIP_*`. `blur` sets the spoiler flag for that viewer, `block` removes `url` and `proxy_url` and sets `sr_filtered: true`, nothing is stored. Bots are never filtered (they have no settings), and the sender always sees their own attachment. Server settings tightening the filter (D-224) ride the same function.
Status: `DEFAULT`. Follow-up: H-33.

**P13-SA-2. Where does the server entitlement level behind `sr_max_upload_bytes` come from? The code has no server-level source, so the value is the member's perk (the same function as the 413 check); `GUILD_CREATE` and `GUILD_UPDATE` lack the field because broadcast payloads are not per viewer.**
Decision: the source exists and the upload path never asked it. `sr-payments` models server levels (doc 08 section 2.2: pooled contributions, `level_for_pool`, grace) and exposes `Entitlements::limits(user_id, guild_id)`, which is `effective_limits(config, patron, guild.level)` and already yields `upload_bytes` as `max(free, level perk, Patron perk)`; the voice governor reads it with the guild. The chat uploads' `UploadEntitlements::max_upload_bytes(user)` has no guild argument, so neither the 413 check nor `sr_max_upload_bytes` has ever applied the level column of the perk table ("Upload size for all members in this server"). Fix the trait: `max_upload_bytes(user, guild: Option<GuildId>)`, the adapter calls `limits(user, guild)`, the slot request and the 413 check pass the channel's guild (`None` for DMs and groups), and `GET /guilds/{id}` reports that number for the caller. On the gateway D-233 is amended rather than bent: no `sr_max_upload_bytes` on `GUILD_CREATE`, `GUILD_UPDATE` or READY `private_channels`; the client reads it from `GET /guilds/{id}` and `GET /users/@me/channels` (which it loads anyway) and refetches the guild on `SR_ENTITLEMENT_UPDATE`, whose `guild` shape goes to every member on the guild topic when the level changes and whose `user` shape reaches the person whose Patron state changed. The web pre-check (D-233's point) loses nothing: the limit is known by the time the composer is open.
Status: `ADOPTED` (D-238, amends D-233). Follow-up: H-34.

**P13-SA-3. Entrance copy permission: `MANAGE_FEATURES` today; should it be a dedicated bit?**
Decision: a dedicated bit. Doc 07's bits are cut by area (`MANAGE_TV`, `MANAGE_COLLECTIONS`, `MANAGE_DIRECTORY`), and the entrance pages are an area: public, unauthenticated, and the only pages a stranger sees, which makes them a phishing surface (a link in the house notes) that should not come bundled with "toggle features and edit limits". `MANAGE_ENTRANCE` is bit 24: the three copy keys, the hero image (P13-WUI-1) and nothing else; the fresh-second-factor requirement stays for the same reason; `OWNER` implies it as always. Existing `MANAGE_FEATURES` holders do not inherit it (staff roles are edited by the Owner, who has everything); doc 07's table and the admin section gate change together.
Status: `ADOPTED` (D-239). Follow-up: H-35.

**P13-SA-4. Staff server deletion and ownership transfer act as the owner and are audited; should they also DM the owner?**
Decision: yes, both, through the system user, with the generic wording the other staff actions use. A server that vanishes or changes hands without a word is the one thing a friend group would call hostile, and Discord's silence here is a scale decision, not a product one. The previous owner gets "Staff transferred ownership of `<server>` to `<new owner>`" or "Staff deleted `<server>`", with the standing line to contact staff; the new owner gets "You are now the owner of `<server>`". The staff reason stays in the staff log and is never quoted (it may name a report or a third person). No DM for the Staff Server (refused anyway) or for a server whose owner account is deactivated.
Status: `DEFAULT`. Follow-up: H-36.

**P13-SA-5. LNG-SSET-027 join method and signals: nothing stores how a member joined (invite code, vanity, Directory, application) and "unusual activity" has no definition. Needs a join-source column set by every join path and an owner decision on signals.**
Decision: store the join source, define two signals, and keep platform-level suspicion out of server moderators' hands. Migration `0874` adds `guild_members.join_source` (`invite`, `vanity`, `directory`, `application`, `creator`, `staff`, `bot`; null for rows older than the migration, shown as "unknown"), `join_invite_code` and `join_ref` (the listing or application id); every join path sets them, including D-204 tester installs and staff transfers. Members with `MANAGE_GUILD` see `sr_join {source, invite_code, inviter_id}` on the member object and in the member table, as Discord's members page shows the invite; everyone else sees nothing. Signals are facts the server can verify, not scores: `new_account` (snowflake age under the server's `warn_new_account_days`, default 7, the same threshold D-224 uses for Message Requests) and `rejoined` (a `member_remove`, kick or ban-then-unban audit row for this person in this server inside the audit retention window). The platform's Person suspicion log, invite freezes and link state are staff data and never appear on a server's member table; a server moderator who suspects a platform-level problem reports it. "Unusual activity" as a filter means "has any signal".
Status: `ADOPTED` (D-240). Follow-up: H-37 (migration `0874`).

**P13-SA-6. LNG-FRM-004 forum post attachments: upload slots are issued per existing channel, and the post thread does not exist before creation. Needs a design (upload against the parent channel, then adopt).**
Decision: that design, which is Discord's. Discord's `POST /channels/{forum}/threads` takes the starter message and its files in one request; here the slots are requested against the forum channel (the member needs `ATTACH_FILES` there, as for any channel), the thread-start body's `message.attachments` lists the upload ids, and `start_forum_post` adopts them into the starter message in the new thread through the same `load_upload` path (invoker's own slots, this forum as the channel), re-parenting the attachment rows to the starter message id. A slot requested against a forum and never used expires like any other. The composer's tray already exists; the New Post form gets it.
Status: `DEFAULT`. Follow-up: H-38.

**P13-SA-7. ADM-CNT-005 (ingest sources: watched folders, storage imports, naming tester) has no admin routes; it needs the ingest watcher design from `docs/specs/ingest-naming-conventions.md`.**
Decision: noted; the review agent is building it on its own branch from that spec, and nothing here pre-empts it. The one rule to carry over: a watched folder's path is operator configuration (`OPERATOR` bit, not `MANAGE_FEATURES`), because it names a filesystem path on the node.
Status: `NO-CODE`.

**P13-SA-8. ADM-PLT-002: API rate limits are still static config (`ApiSettings`), not registry settings; making them live needs a reload hook in the limiter. Also ADM-COM-001: a staff preview of a pending Directory listing and a category manager are not built.**
Decision: make the limits live with bounds, build the preview, and close the category manager. The per-class HTTP limits (global per IP, per session, per bot, the message-send class and the upload class) become registry settings under `limits.rate.*` with a declared floor and ceiling each (a floor so a typo cannot lock everyone out of the instance, a ceiling so "unlimited" is not a value); the limiter holds its config behind a `tokio::sync::watch` receiver that the registry's write path updates, so a change applies within a second without a restart, and the Limits page edits them like every other registered setting. Gateway and voice limits stay static (they are sized to the node). For the Directory: `MANAGE_DIRECTORY` staff can open a pending listing's server page in preview (the existing directory page with the listing's draft data and a "pending" banner, no join); the category list stays a constant and hiding stays the `directory.hidden_categories` feature, so there is no category manager to build.
Status: `DEFAULT`. Follow-ups: H-39 (rate limits), H-40 (preview).

### Web (P13-WUI, branch `feat/p13-web-ui2`)

**P13-WUI-1. ADM-PLT-003: hero image editing has no backend route; the box stays open.**
Decision: build the route; the picture itself is the owner's. `PUT|DELETE /admin/entrance/hero` (`MANAGE_ENTRANCE`, fresh second factor, data URI, sniffed to PNG, JPEG or WebP, longest side shrunk to 1600 pixels, 4 MiB after shrink, stored at `entrance/hero` in object storage, audit row) and `GET /sr/entrance/hero` (public, ETag, `max-age` 3600, 404 when unset). The landing page shows the operator's image in the door slot when the route answers and the shipped door art otherwise, with the alt text from a fourth copy key `hero-alt` (plain text, 200 characters). First paint is unchanged: the image is fetched after the copy, and the shipped art is still inlined. Whether to replace the door art at all is the owner's artwork decision and stays in [open-questions.md](open-questions.md).
Status: `DEFAULT`. Follow-up: H-41.

**P13-WUI-2. LNG-EXP-012 (burst picker variant and animations) is closed by D-231; the box is left unticked so the owner can tick it as "won't build".**
Decision: record it as won't-build now; D-231 already closed it and an unticked box reads as work. The inventory line becomes `[x] ... won't build (D-231: shim only)`, which is how the earlier closed items are marked.
Status: `NO-CODE`. Follow-up: H-42 (inventory line).

### Library (P13-LIB, branch `feat/p13-library`, remaining item)

**P13-LIB-7. Single-page replacement in the admin chapter editor is a follow-up (the editor can reorder, flag spreads and remove pages, and re-convert a whole chapter from an archive).**
Decision: build it small. `PUT /admin/library/chapters/{id}/pages/{index}` (`PUBLISH_READING`, data URI or a staged upload id, the same sniff and limits as an archive page) replaces that one page through the converter (dimensions, crop data, variants), keeps the page's position and spread flag, writes an audit row and bumps the page-list fingerprint so D-225's offline cache and the CBZ export (LIB-RDR-016) pick it up. Replacing one scanned page should not need the whole archive again.
Status: `DEFAULT`. Follow-up: H-43.

### Still with the owner from earlier rounds

Unchanged: the UnRAR licence confirmation (D-170) and the eight NEEDS-OWNER items of the Phase 8 to 11 round (P8-DON-4, P11-MSG-1, P11-SHL-3, P11-CLI-1, P11-USB-2, MOD-4, MOD-5, MOD-6); this round adds none. Two items touch the owner without being decisions: the CI download for DEV-SDK-003 (P13-DEV-7) and the entrance door artwork (P13-WUI-1), both listed in [open-questions.md](open-questions.md) under human actions.
