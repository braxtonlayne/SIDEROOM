# Handoff: feat/p15-rest-c (wound down early at the owner's request)

Base: integration/phase15 + feat/p15-answers. No migrations used (1000-1009 untouched). No scratch Postgres DB was created; no server or background process was started.

## Resume here

### Done and ticked in docs/dev/answers-followups-p15.md
- K-01 voice waveform: `sr_chat::voice_meta::declared_waveform_is_consistent` + `apply_voice_meta` in `crates/sr-api/src/routes/messages/rich_create.rs`; doc 05 line 81. sr-chat unit tests pass (324). The sr-api test `measured_voice_keeps_a_consistent_declared_waveform` is written but NOT run yet.
- K-02 holographic rule: `validate_colors` in `crates/sr-chat/src/role_rules.rs` (sr-chat tests pass); route cases added to `crates/sr-api/tests/guilds.rs` (not run, needs Postgres or the memory fixture); doc 05 updated; web constant test added in `apps/web/src/lib/roleGradient.test.ts` (passes).
- K-03 trimmer 48 kHz: `soundTrim.ts` `decodeAudioFile` via OfflineAudioContext; test in `settings/server/__tests__/soundTrim.test.ts` (passes, with vitestVersion test).
- K-09: `crates/sr-payments/tests/esplora_live.rs` and `docs/dev/payments-live-test.md`. Ran live against mempool.space signet: 3 pass (funded address is the signet block 1 coinbase address; first tx id is in the test).
- K-14: doc and inventory lines done, EXCEPT the ADM-GRAPH-003 note, deliberately skipped because the canvas renderer is still to be built.

### Half-done: K-08 (commit "wip: K-08 backend")
Done (compiles, `cargo check -p sr-server` clean, fmt clean, NO tests run):
- `UserSummary.patron` (sr-chat repo.rs); Pg query in `crates/sr-db/src/chat_repo.rs` `UserLookup::users` (active or 3-day grace, not hide_badge); `GuildRepository::user_guild_ids` (default empty, Pg and MemoryRepo impls).
- `sr_patron: bool` (always present) on `social::model::UserObject`, `messages::model::UserRef`, `guilds::dto::UserRef`.
- `MessagesDeps::refresh_member_objects` (messages/deps.rs) sends GUILD_MEMBER_UPDATE; called from `BusPaymentEvents::entitlement_user` and `patch_preferences` when `hide_badge` changes (`donations/mod.rs`, `member.rs`); `DonationsDeps::with_chat_slot` wired in `crates/sr-server/src/app.rs`.
Next steps:
1. Tests: sr-api user-object test (active, grace, hidden, expired), refresh dispatch test (MemoryRepo user_guild_ids + recording bus), a sr-db Postgres test for the patron SQL (scratch DB).
2. Move the remaining `Entitlements::limits` call sites onto the same seam (DON-ENT-006): `crates/sr-db/src/media_repo/downloads.rs:328`, `crates/sr-server/src/gateway_pg.rs:655`, `upload_limits.rs`, `watchparty.rs:156`, `sr-payments/service.rs:555`.
3. Web: `ring` prop on the packages/ui avatar (design token), use in member list, message author, profile popout, voice tile; preferences copy "Show my Patron badge and ring" (new i18n fragment `p15-rest-c-1.ts`); docs 05 user object row and doc 08 section 2.1 row; tick DON-UI-005, DON-ENT-006, K-08.
4. Regenerate OpenAPI (`cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json`, then the api-client generate and mocks scripts); `KNOWN_CLASHES` must stay empty.

### Not started
- K-10 system probes (`GET /admin/system` backups + disk; two web cards; ADM-PLT-006).
- TV-ENG-001/003/007, TV-ADM-003/004 (schedule slots, guide imports and favorites, imported_guide resolver, fuzzy import, weekly grid editor): needs migrations in 1000-1009, sr-tv, API, FE; read rest-a's handoff and docs/specs/cinema-live-tv.md.
- WP-BE-008, WP-FE-008, WP-FE-010 (in-chat live watch-party card: new message type plus edit path).
- ADM-GRAPH-003 canvas 2D renderer over sr-graph-wasm above ~500 nodes (G-24); then add no K-14 deferral note.

## Gates
Passed: cargo test -p sr-chat --lib; esplora_live (live); vitest for soundTrim, roleGradient, vitestVersion; cargo fmt --check; cargo check -p sr-api --tests and -p sr-server.
Not run: sr-api tests, sr-db tests, clippy -D warnings, web typecheck/lint/build/bundle budget, OpenAPI regeneration, route matrix. The worktree install was done (`pnpm install --offline --frozen-lockfile`); the wasm build was not.

## Owner questions
None new. Grace days for the patron flag are hardcoded to the default 3 in SQL (`PATRON_GRACE_MS`); a non-default `grace_days` config would not be reflected.
