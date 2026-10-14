# Handoff: feat/p8-dev-be (COMPLETE for the scope below)

Worktree `C:/Users/braxt/Desktop/SIDEROOM_wt/p8-dev-be`, base `integration/phase7` (= main). Local commits only, nothing pushed. Contract for the web agent: [docs/specs/developer-platform.md](../../specs/developer-platform.md) section 3 (committed first; shapes are unchanged since, a few wording fixes only). Compatibility list: [docs/dev/sdk-compat.md](../sdk-compat.md).

## Done

Backend for DEV-APP and DEV-BOT, plus the DEV-SDK groundwork. Migrations `0500` to `0505` (inside the 0500-0539 range).

* **`sr-auth`**: `oauth.rs` (opaque 256-bit credentials with a domain-separated SHA-256 per kind, PKCE `S256` incl. the RFC 7636 vector, redirect URI rules, scope parsing). `BearerIdentity` gained `application_id`/`expires_at`.
* **`sr-apps`** (new crate): the service (`AppsService`), the storage port (`AppsRepository`), an in-memory repo for tests, pure rule sets (`commands` validation, `interactions` checks, `perms`, `manifest`). 50 unit tests.
* **`sr-db`**: `apps_repo.rs` (`PgAppsRepo`), 11 Postgres tests in `tests/apps_repo.rs`. `accounts.is_bot`; bot accounts are excluded from the five account-cap counts (registration, auth store, three admin stats).
* **`sr-api`**: `routes/developer/` (portal, OAuth2, bot API, interactions), the credential policy `auth/policy.rs` (bearer default-deny with an allow-list, bot deny-list) applied in the `Principal`/`UserOnly`/`BotOnly` extractors, the new `SessionOnly` extractor, the interaction-follow-up hook in `settings/webhooks.rs::execute_webhook`, `bot: true` on message/member/profile user objects (`UserSummary.bot`), `GET /users/@me` for bearers and bots, extra rate-limit buckets, token redaction in request logs. 21 route tests (`tests.rs`, `flow_tests.rs`) + policy and redaction tests.
* **`sr-gateway`**: IDENTIFY `shard` (only `[0, 1]`, else close `4010`), 1 test. Bot IDENTIFY/RESUME, privileged-intent gating (`4014`) and `MESSAGE_CONTENT` redaction were already in the hub; they now get real data.
* **`sr-server`**: `NoBots`/`NoBearers` are gone (`backends.rs` re-exports `AppsAuth`); `PgAuthenticator` accepts bot tokens (any token with dots), `PgGuildDirectory` gives a bot's READY `application: {id, flags}` and none of a person's state; hourly interaction GC; `DevDeps` wiring.
* **Harness (DEV-SDK-003 groundwork)**: `crates/sr-server/src/bot_pg_tests.rs`, one Postgres-backed test driving the real gateway and REST router as a bot (IDENTIFY failures, READY, GUILD_CREATE, MESSAGE_CONTENT redaction per session, mention exception, REST send, slash command round trip, token reset). The 17-row behaviour list is in `docs/dev/sdk-compat.md`.
* **Docs**: spec, compat list, doc 05 pointers, inventory ticks, open question 102 note. `packages/api-client/openapi.json` regenerated (tags `developer`, `oauth2`, `bot`); `src/generated/schema.d.ts` regenerated with the repo clone's `openapi-typescript` 7.13.0 (`node <repo>/packages/api-client/node_modules/openapi-typescript/bin/cli.js openapi.json -o src/generated/schema.d.ts`, because the worktree has no `node_modules` and I did not install anything).

Ticked in `docs/inventory/17-developer-platform.md`: DEV-APP-002, 003, 004, 005, 006, DEV-BOT-001, 003, 004. Annotated as partly done, not ticked: DEV-APP-001 (no `app_testers`), DEV-APP-007 (no user installs), DEV-BOT-002 (single shard only), DEV-BOT-006, DEV-SDK-003.

## Tests

New non-ignored: sr-apps 50, sr-auth 5, sr-api 24 (21 developer routes, 2 policy, 1 redaction), sr-gateway 1, sr-server 1. New ignored (Postgres): sr-db 11, sr-server 1. Whole workspace: `cargo test --workspace` passes; every ignored suite of `sr-db`, `sr-server` and `sr-api` passes against a dedicated database (`--ignored --test-threads=1`). `cargo fmt --check` and `cargo clippy --workspace --all-targets -D warnings` are clean. `cargo deny check`: bans, licenses and sources pass; **advisories fail on `rustls 0.23.45` (RUSTSEC-2026-0346 to 0349), which this branch did not introduce** (the lock file only gained `sr-apps` and two dev-dependencies of sr-server: `tower`, `http-body-util`, both already in the lock).

## Security decisions

* Every credential is 256 random bits, shown once, stored as a domain-separated SHA-256, looked up through a unique index, then compared again in constant time (`oauth::digests_equal`). Bot tokens keep the Discord three-part shape (open question 56) and are also bound to the account they name.
* The client secret, bot token, authorization code, access/refresh tokens and interaction token are distinct kinds: a value minted for one never matches a digest stored for another.
* One central policy in the extractors, not per handler: bearers can reach `GET /users/@me` (`identify`), `GET /users/@me/guilds` (`guilds`), `GET /oauth2/@me` and the command-permission PUTs; everything else is `50001`. Bots are denied (`20001`) on `/admin`, `/developers`, the consent API, owner portal routes, `/sr/*` (except `csrf` and `voice/turn`) and the account routes. It is switched on by `ApiSettings::for_public_url` (production) and off in `ApiSettings::default()` because existing unit-test fixtures stand bot tokens in for users; a test per denial lives in `auth/policy.rs` and `developer/{tests,flow_tests}.rs`.
* Owner-only operations use `SessionOnly`; secret and token resets and deletion re-ask for the password (shared login lockout). Non-owners get the same 404 as a missing id.
* No privilege escalation: the bot's managed role gets `requested AND installer's own permissions`; a private (default) or unreleased application can only be installed by its owner, development apps only in guilds of at most 25 members (`900120`); the installer needs `MANAGE_GUILD`. The managed role goes just above `@everyone` and the roles that were there shift up.
* Command permissions can only be changed by a bearer of a server manager issued to that application with the dedicated scope; bot tokens and sessions are refused. Invoking a command re-checks the member's channel access, `USE_APPLICATION_COMMANDS`, the install, default permissions and overwrites.
* OAuth2: exact redirect URI match (https or loopback http), invalid client/redirect/scope is a 400 and never a redirect, `S256` only, code single use with replay revoking its tokens, refresh rotation with reuse detection revoking the grant, a client without a secret needs PKCE and cannot refresh, introspection and revocation only for the issuing client, tokens die when the user (or a bot's owner) is no longer `active` and when the application is suspended.
* Interactions: the token is compared in constant time, bound to its application, valid 15 minutes, the first response is a compare-and-set within 3 s; only messages that belong to an interaction can be edited through its token; component interactions must name a `custom_id` that exists on a message the application's bot wrote; user/channel/role option values are resolved and must belong to the server and be visible to the invoker.
* Bot accounts cannot log in (a well-formed Argon2id hash nothing matches, so timing looks like any account) and are not counted by the account cap.
* Secrets never reach logs: request logs mask `/interactions/*/{token}` and `/webhooks/*/{token}`, secret-bearing types have a redacted `Debug`, audit rows carry no secrets, responses with secrets send `Cache-Control: no-store`.

## Not done / differences (details in spec section 2)

* DEV-BOT-005: no HTTP interactions endpoint and no Ed25519 signatures. DEV-BOT-007 (bots in voice), DEV-ACT-*, teams, app testers, user installs, application emoji, role connections, `guilds.join`, `client_credentials` and sharding beyond `[0, 1]` are not built.
* A non-ephemeral interaction response needs the bot's own `SEND_MESSAGES` (Discord lets the token bypass it). A deferral shows "is thinking" to the invoker only. Interactions are server-only.
* Posted messages can only change `content` and `SUPPRESS_EMBEDS` (the message store cannot replace embeds/components); the API refuses such an edit with `50035` instead of ignoring it. Select menus and modal text inputs are not stored on messages (only buttons round trip). Message `interaction`/`application_id` are not stored.
* Resetting a token or suspending an application does not close gateway sessions that are already connected (new IDENTIFY and RESUME fail).
* `execute_webhook`'s hook for interaction follow-ups is covered through a stand-in route in the developer tests (the same `try_followup` call), not through the real settings router. The real route is only compile-checked.
* The tier 2 and 3 compatibility runs (discord.js, discord.py, serenity, twilight against a running server, recorded fixtures) are planned in `docs/dev/sdk-compat.md` and not in CI.
* `dm_permission`, `contexts` and `integration_types` are stored and returned but have no effect yet.

## Owner decisions needed

1. Should an unreleased application be installable by testers other than its owner (`app_testers`, doc 11 section 2.2)? Today only the owner, in guilds of 25 members or fewer.
2. Is the install permission cap (bot role permissions = requested AND installer's) too strict next to Discord? It is the "no escalation" rule; relaxing it means trusting `MANAGE_GUILD` holders with arbitrary bot permissions.
3. Should interaction responses be allowed without the bot's `SEND_MESSAGES` like Discord (token-based)? It would let a bot post where it cannot normally write.
4. Privileged intents in `development` are developer-toggled without review (the guild limit of 25 is the guard). Doc 05 can be read as wanting review for every use; say if you want it everywhere.
5. Bearer tokens last 7 days, refresh tokens 90 days, codes 5 minutes, at most 25 applications per account: confirm or change.
6. Application icons are served from the bot's avatar path (`/cdn/avatars/{bot id}/{hash}.png`); a dedicated CDN kind would need a change in the storage crate.
7. The rustls advisories above need a lock bump on the integration branch (not mine to do in a shared lock file).
