# Handoff: feat/p28-imm-flags (wave 28: BYOS-FLG-001, OPS-IMM-001, OPS-IMM-002)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p28-imm-flags`, base `integration/phase28` 666e7579.
Env: `CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p28-imm-flags-target`, `CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`, one cargo at a time.
Scratch DB `sr_p28_imm_flags` (not created yet). node_modules installed offline; `packages/wasm/dist` copied from `p27-integ` (not committed).

## Resume here

Slices 1 to 3 done and committed (BYOS-FLG-001, OPS-IMM-001). Next, in order:

1. **OPS-IMM-002**: `crates/sr-server/tests/immutable_rules.rs`, then wire it into `.github/workflows/rust.yml`'s ignored step (the step runs `cargo nextest run --workspace --exclude sr-storage --run-ignored only -j1`, which already picks up an `#[ignore]` test in `sr-server/tests/`, so the wiring may be a comment and a named step; check) and validate the YAML with js-yaml. Add the `docs/15-threat-model.md` row "flags cannot weaken the three" and the `AGENTS.md` section 4 line "run immutable_rules.rs before integrating".
2. Gates still to run before finishing: `cargo fmt --check` and `cargo clippy -j 2 -D warnings` on sr-core, sr-admin, sr-features, sr-storage, sr-db, sr-api, sr-gateway, sr-voice, sr-voice-proto, sr-server (sr-api builds take about 11 minutes); the sr-server voice tests; OpenAPI should be unchanged (no new routes; the byos gate is a layer); web: i18n audit, bundle budget.
3. Not done: the Admin "endpoint test button for staff on any mount" (part of the BYOS-FLG-001 row, belongs to BYOS-MNT-002); a full HTTP test that the kill switch leaves mount 1 reads working (only the pure `decide` is tested; do it inside the harness).

## Done

BYOS-FLG-001
- `crates/sr-storage/src/mount_policy.rs`: `WhoMayMount`, `EndpointAllow` (host or `*.suffix`), `MemberStanding`; 6 unit tests.
- `crates/sr-admin/src/settings.rs` (group `storage`): `byos.endpoint_allow` (text), `byos.require_client_remux` (bool, wave 29 reads it), `byos.who_may_mount` (text: all, patrons or staff; empty means all). `byos.presign_ttl_*` already existed. byos-browser adds `byos.max_mounts_per_user`, `byos.max_mounts_per_guild`, `byos.upload_parts_per_hour`: NOT added here (no duplicates).
- `crates/sr-features/src/byos.rs`: feature `platform.byos` (Platform + Guild scopes, default on, client visible); guild settings `guild_mounts` and `member_shelves_in_guild` (both default on; `ByosSettings::combine` ANDs the platform value with the server's). Registered in `sr-server/src/app.rs::feature_registry`.
- `crates/sr-api/src/routes/storage/flags.rs`: `byos_gate` route layer (one `route_layer` in `router.rs`): platform off means every storage route answers 403 `SR_FEATURE_DISABLED` except mount 1 reads (GET objects, POST presign); `guild_mounts` off refuses `POST /guilds/{id}/storage/mounts`. `MountPolicy` (reads the two settings) is enforced in `mounts.rs` create and patch: `endpoint_for` checks the allow-list before DNS, `create_user` checks `who_may_mount` (patron and staff standing from `sr_db::scopes_repo::PgScopeFacts` via `MountsDeps::with_standing`; fails closed without a source).
- Web: `settings/byos.tsx` (`useByosOn`, `ByosOffNotice`) in the user and server Storage sections; Limits group label `storage`; fragment `i18n/messages/en/p28-imm-flags-1.ts`.
- `member_shelves_in_guild` is stored and combinable but nothing reads it yet: the shelves in Cinema and Library belong to byos-catalog (call `ByosSettings::combine` there). Flag for the integrator.

OPS-IMM-001
- `sr-core/config.rs`: the `voice.e2ee` field is replaced by a private `removed_e2ee: Option<bool>` (serde name `e2ee`); `e2ee = false` (also `SR_VOICE__E2EE=false`) is a config error, `true` is accepted and ignored. Tests updated.
- `sr-voice-proto`: `VoiceCommand::E2eeKeyed { channel, user, keyed }`.
- `sr-gateway`: `VoiceConfig.e2ee` removed; `e2ee_enabled()` now means "voice is configured"; `VOICE_SERVER_UPDATE.sr_e2ee` is always `true`; on op 101 `key_package` (valid base64) and on `ready` the gateway sends `E2eeKeyed` to the SFU (before relaying the key package to the peers). The D-263 join refusal (`e2ee_required`) and the 10 s hello timeout stay (stricter than the roadmap sentence "joins but is never heard"; the owner may relax it). Tests: two "e2e off" tests removed, new `the_sfu_is_told_a_member_is_keyed_only_after_its_key_package_or_ready`.
- `sr-voice`: participants carry `e2ee_keyed`; unkeyed senders are not forwarded (audio, camera, screen), not announced as published and not shown speaking; dropped frames are counted in `sr_voice_unkeyed_frames_total` (`Snapshot.unkeyed_frames`, `Deltas.unkeyed`). A keyed notice that arrives before the member joins the SFU is held 60 s (`pending_keyed`, capped at 8192) like `pending_flags`. Tests: `loopback.rs::an_unkeyed_sender_is_not_forwarded_until_its_key_package`, `e2e.rs::a_client_is_not_heard_until_the_gateway_keys_it` (the existing e2e helpers key their clients first).
- Web: `voice/e2ee/flag.ts` and `serverFlag.ts` deleted; `support.ts` gained `e2eeDeclared(mode)` and `e2eOptOut()` (the e2e observer only, `VITE_SR_E2E=1`); the controller always starts E2EE (no `sr_e2ee` check); Settings > Advanced always says "end-to-end encrypted" (`e2eeOff` strings removed from `p15-rest-b-1.ts`); tests updated.
- e2e: `playwright.e2ee.config.ts` deleted; its fake-media browser settings moved into the main `playwright.config.ts` projects; `e2e/e2ee/voice-e2ee.spec.ts` moved to `e2e/tests/voice-e2ee.spec.ts` (180 s budget via `describe.configure`); `.github/workflows/e2e.yml` E2EE steps run `--project=X tests/voice-e2ee.spec.ts`; the `SR_VOICE__E2EE` env lines are removed there and in `voice-soak.yml`; both parse with js-yaml. NOT run: Playwright (no server was started, nothing ran in a browser).
- Docs: doc 05, doc 09, doc 12 (operator note rewritten), doc 15 T16, voice spec status, sr-sdk README, sr-e2ee lib doc.

## Tests run

- `cargo test -p sr-features byos` (3), `-p sr-admin --lib settings` (9), `-p sr-storage --lib mount_policy` (6), `-p sr-api --lib storage` (18, includes the allow-list and who_may_mount HTTP tests), `-p sr-core -p sr-gateway -p sr-voice-proto --lib` (43, 185, 17), `-p sr-voice --lib --test loopback --test e2e --test video --test load_smoke`: green.
- web: vitest `src/voice` and `p15Advanced` (17 files, 223 tests), `src/settings/__tests__/byosFlag.test.tsx`, `src/admin/__tests__/admin.test.tsx` and `p21abuse.test.tsx`: green; `tsc --noEmit`, eslint and prettier on touched files: clean. A wider run of `src/admin` had 2 failures while the laptop was suspended (the run took 2.8 hours of wall clock); it was not repeated, the touched files pass.

## Decisions

- The two guild choices are settings of the one feature `platform.byos`, not separate feature ids; a server's value is ANDed with the platform's.
- `byos.who_may_mount`: empty means `all`; an unrecognised stored value fails closed to `staff`. A failed settings read fails closed to `staff` as well.
- The SFU gate is protocol-level (key package seen), not cryptographic; recorded in doc 15.
- No row was added to `docs/01-decisions-log.md` (numbering is shared this wave); the owner may want to record "E2EE is the only voice mode" as a decision (D-249 and D-263 still say "when `voice.e2ee` is on").

## Open owner questions

- None blocking. See the D-263 note above.
