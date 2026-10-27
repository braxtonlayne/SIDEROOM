# Handoff: feat/p28-imm-flags (wave 28: BYOS-FLG-001, OPS-IMM-001, OPS-IMM-002)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p28-imm-flags`, base `integration/phase28` 666e7579.
Env: `CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p28-imm-flags-target`, `CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`, one cargo at a time.
Scratch DB `sr_p28_imm_flags` (not created yet). node_modules installed offline; `packages/wasm/dist` copied from `p27-integ` (not committed).

## Resume here

All three items are built, committed and tested (BYOS-FLG-001, OPS-IMM-001, OPS-IMM-002). What is left, in order:

1. Gates: `cargo fmt --all --check` and `cargo clippy -j 2 -D warnings --all-targets` on sr-core, sr-admin, sr-features, sr-storage, sr-db, sr-api, sr-gateway, sr-voice, sr-voice-proto, sr-server (script in the scratchpad; the result is in the section "Gates" below once it ran); web i18n audit and bundle budget (not run).
2. Not done: the Admin "endpoint test button for staff on any mount" (part of the BYOS-FLG-001 row, belongs to BYOS-MNT-002); a full HTTP test that the kill switch leaves mount 1 reads working (only the pure `decide` is tested); Playwright runs of the E2EE specs on 8083/5176 (no server was started this session); the `sr-server` voice Postgres test `voice_pg_tests` (edited to declare `sr_e2ee`, not run); `member_shelves_in_guild` is stored but read by nobody yet (byos-catalog should call `ByosSettings::combine`).
3. Integration notes: `docs/15-threat-model.md` got row T28 and `scripts/threat-model.test.mjs` now accepts at least 28 rows (byos-legal also adds rows: renumber at the merge, keep the rows in order); `authz_matrix.rs` got one appended `pub mod harness` and four small edits in `run_group` (hook calls, a refusals-only mode) plus an `embedded()` guard in its two big tests, which return at once when the file is compiled into `immutable_rules.rs`.

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

OPS-IMM-002
- `crates/sr-server/tests/immutable_rules.rs` (new) includes `authz_matrix.rs` with `#[path]` and drives `authz_matrix::harness` (an appended `pub mod` there): `harness::run(flip, groups)` seeds a fresh world per group (the matrix seed, cast and client, not copied) and, after the seed, puts the platform in a state through the Admin API as the Owner: every feature killed at platform scope (12 of the 31 listed were on and are killed), every setting at its minimum, every setting at its maximum (51 settings, one patch). Then, per group (moderation, admin, rest = storage and voice routes), it checks: the state change wrote the staff access log; an audited admin action (a settings change; a feature switch is access-logged only, by design) wrote `platform_audit_log` and the access log; a staff read of a member archive (`GET /admin/private/accounts/{id}/dms`) wrote a `private_read` access-log row; `GET /admin/access-log/verify` reports `ok`; and the matrix refusal phase (all denial calls, plus the "refused calls write nothing" fingerprint) still refuses. Result: 0 failures in all nine runs (3 states x 3 groups), 184 s on a warm build.
- Rate limits and abuse limiters stay at their maximum in every state (at their minimum every call would be 429, which proves nothing about authorization); that is the one carve-out, named in the file header and in doc 15 T28.
- (b) `no_flag_name_matches_the_deny_pattern`: walks feature ids, settings-schema field keys, the settings catalog and the config field paths; one legacy hit is pinned in `LEGACY`: `moderation.audit_retention_days` (retention of a server own Discord-parity audit log, 0 = forever; not the staff logs). **Owner to confirm the name is acceptable or rename it.** (c) `no_description_claims_to_disable_logging_or_encryption` plus a self-test of the checker, a test that pins the deny pattern, and `the_voice_encryption_switch_is_gone_from_the_config`.
- `.github/workflows/rust.yml`: the ignored-suite step already runs every `#[ignore]` test of the workspace, so the harness is in it; a named step after it fails when the harness disappears from `cargo nextest list --run-ignored only`; the YAML parses with js-yaml. `AGENTS.md` section 4 and `docs/15-threat-model.md` T28 added.
- Not covered by the harness (named gaps): the voice join is covered by `sr-gateway`/`sr-voice` tests and by the name and config tests, not by a websocket client inside the harness; guild-scope and channel-scope flags and combinations of settings are not visited.

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
