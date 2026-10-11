# Open questions

Only questions that still need a human are listed here. Every other question was decided, and each answer is in [open-question-answers.md](open-question-answers.md). Numbers in parentheses (`Q46`) are the original numbers from that file. Each entry links to the code it affects. Paths are relative to the repo root and resolve once the originating branch is merged.

Remove an entry when it is decided, and record the decision in [01-decisions-log.md](01-decisions-log.md).

## Owner decisions

For each of these, the recommended answer was adopted as the default and is being implemented. The owner confirms it or overrides it.

1. **Emergency operator invites.** `invite create` from the CLI consumes no credit and writes a `platform_audit_log` row (`invite.emergency_create`, actor = system user 1). Confirm, or require credits or a different audit trail. (Q46) — [commands.rs](../crates/sr-cli/src/commands.rs)
2. **Guild audit-log retention.** Default 45 days (Discord), set by the platform setting `moderation.audit_retention_days` (`0` = forever). The platform and staff access logs stay forever. Confirm the default. (Q133) — [audit.rs](../crates/sr-chat/src/settings/audit.rs)
3. **AutoMod preset word lists.** Ship no built-in lists; the operator curates them in Admin → Features → AutoMod. Shipping a list is an editorial and legal call. (Q136) — [automod.rs](../crates/sr-chat/src/settings/automod.rs)
4. **Hidden channels in the channel list.** `GET /guilds/{id}/channels` hides channels the caller cannot view, which differs from Discord. Keep the stricter behaviour and document it in the porting guide (doc 05 §7). (Q161) — [admin_service.rs](../crates/sr-chat/src/admin_service.rs)

## Decisions needed before building

5. **Passkeys need an ADR.** Passkeys (ENT-AUTH-010) are deferred. Doc 05 §5.1 says `webauthn-rs` depends on the C `openssl` library, and doc 03 §4 and D-150 require an ADR in `docs/adr/` before a C dependency is added. Pick one:
   - (a) approve `webauthn-rs` with an ADR that accepts the C dependency;
   - (b) use a pure-Rust WebAuthn implementation (none has been evaluated yet);
   - (c) keep passkeys deferred (the current state).

   — [routes/auth](../crates/sr-api/src/routes/auth), [03-tech-stack-and-rust-policy.md](03-tech-stack-and-rust-policy.md)
6. **Default staff role permission bits are unspecified.** Doc 07 §4 lists the default Staff Server roles (Owner, Senior Staff, Moderator, Media Curator, Library Curator, App Reviewer, Support) but gives bits only for some of them, and only in part (Senior Staff is "…"). `bootstrap` says the default roles are not created yet. Specify the `roles.platform_permissions` bitfield for each role. — [07-permissions-and-staff.md](07-permissions-and-staff.md), [commands.rs](../crates/sr-cli/src/commands.rs)

## Needs a database run

7. **The real Postgres has never run any SQL.** Migrations 0001 to 0080 and the `sr-db` repositories compile and pass clippy, but no SQL has run against Postgres. Every `#[ignore]` test has been skipped, including the ENT-INV-010 concurrency test. (Q11, Q38, Q99, Q121, Q139) Run this before the next integration merge:

   ```
   just dev
   SR_TEST_DATABASE_URL=postgres://sideroom:sideroom_dev@127.0.0.1:5432/sideroom cargo test -p sr-db -- --ignored
   ```

   Then add the same as a CI job with a `postgres:17` service container. Affected tests: [auth_repo.rs](../crates/sr-db/tests/auth_repo.rs), [chat_repo.rs](../crates/sr-db/tests/chat_repo.rs), [social_repo.rs](../crates/sr-db/tests/social_repo.rs), [settings_repo.rs](../crates/sr-db/tests/settings_repo.rs), [guild_admin_repo.rs](../crates/sr-db/tests/guild_admin_repo.rs), [registration.rs](../crates/sr-db/tests/registration.rs). Migrations: [migrations](../crates/sr-db/migrations).

## Needs a human outside the code

8. **Permission fixtures are hand-derived.** Capture real fixtures from a test guild in the Phase 6 conformance suite. (Q8) — [crates/sr-core/tests](../crates/sr-core/tests)
9. **Presigned URLs are untested against Garage.** Run the `#[ignore]` presigned-URL test against `just dev`. Only the builder and the in-memory fake are covered now. (Q26) — [storage.rs](../crates/sr-storage/src/storage.rs)
10. **Pinned ops versions and bootstrap flags.** Run `just dev`. Pin Garage and Mailpit to the latest tags that start, and check `bootstrap.sh` flags against `garage --help`. Parse `garage.toml` and run `cargo deny check` as part of that run. Record the versions in `deploy/compose/README.md`. (Q20, Q21) — [dev.yml](../deploy/compose/dev.yml), [bootstrap.sh](../deploy/compose/garage/bootstrap.sh)
11. **pnpm is not installed on the agent machines.** Install it with `corepack enable && corepack prepare pnpm@10.12.1 --activate`. Agents must not fall back to npm for lockfile changes. (Q19) — [pnpm-workspace.yaml](../pnpm-workspace.yaml)
12. **Font files are missing.** Add Newsreader, Hanken Grotesk, IBM Plex Mono and a Lounge sans as self-hosted WOFF2 under `apps/web/public/fonts/<family>/`, each with its `OFL.txt`. (Q33) — [fonts.ts](../packages/ui/src/theme/fonts.ts)
13. **Token values are not calibrated against the mockups.** Do a visual pass at 1440 and 1704 px against [docs/assets/mockups](assets/mockups/README.md) and under 900 px. Land adjustments as token-only PRs. (Q34) — [packages/ui/src/theme](../packages/ui/src/theme)
14. **The door image is missing.** Supply `apps/web/public/entrance/door.jpg`. The Admin → Entrance upload comes later. (Q81) — [Door.tsx](../apps/web/src/components/Door.tsx)
15. **Markdown corpus has not been reviewed against Discord.** Review a stratified sample of 100 cases against Discord, after the parser quirk decisions (Q31, Q89, Q90) are in. Mismatches become hand-written expectations. (Q85) — [tests/corpus](../crates/sr-markdown/tests/corpus)
16. **The CI workflows have never run.** `release.yml` and `openapi.yml` need a first green run in CI. FND-CI-004 stays unticked until the Playwright config exists. (Q108) — [release.yml](../.github/workflows/release.yml), [openapi.yml](../.github/workflows/openapi.yml)

## Informational (NO-CODE, accepted; revisit at the named trigger)

17. **Bot and OAuth bearer backends reject every token** until Phase 6 adds the schema (see Q56 for the token format). (Q102) — [backends.rs](../crates/sr-server/src/backends.rs)
18. **Browser zstd decoding is deferred.** Revisit when payload volume justifies it. (Q122) — [client.ts](../apps/web/src/gateway/client.ts)
19. **Member lists use REST, not op 102.** Lazy member ranges arrive with the member-list inventory item. (Q124) — [rest.ts](../apps/web/src/store/rest.ts)
20. **A fresh READY clears cached message windows, and RESUME refetches nothing.** Accepted as Discord parity. (Q126) — [reducer.ts](../apps/web/src/store/reducer.ts)
21. **Message windows have no `around` (jump-to-message) window.** Comes with LNG-MSG jump-to-message. (Q127) — [reducer.ts](../apps/web/src/store/reducer.ts)
22. **Search filters take ids, not names.** Accepted. Negation (`-from:`) comes later. (Q130) — [query.rs](../crates/sr-search/src/query.rs)
23. **`*-wasm` crates build natively as well as for wasm.** Accepted. Their wasm-only paths need `wasm_bindgen_test`s when Q68 lands. (Q69) — [crates](../crates)
24. **Placeholder UI looks real.** Add Server, Discover, Inbox and similar entries open a "coming soon" modal, and some header buttons are inert. Each item stays unticked in [03-lounge-shell.md](inventory/03-lounge-shell.md) until it is built. (Q168) — [inventory](inventory)
