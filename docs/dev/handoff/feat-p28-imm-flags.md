# Handoff: feat/p28-imm-flags (wave 28: BYOS-FLG-001, OPS-IMM-001, OPS-IMM-002)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p28-imm-flags`, base `integration/phase28` 666e7579.
Env: `CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p28-imm-flags-target`, `CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`, one cargo at a time.
Scratch DB `sr_p28_imm_flags` (not created yet).

## Resume here

Slice 1 (2026-10-27 morning): BYOS-FLG-001 settings + feature + pure policy done; route gating NOT done yet.

Next steps, in order:
1. BYOS-FLG-001 route side: `routes/storage/flags.rs` gate middleware (platform kill switch, 403 SrFeatureDisabled, reads of mount 1 pass; guild mount routes follow `ByosSettings::combine`), wired with one `route_layer` in `sr-api/src/router.rs`; `byos.who_may_mount` and `byos.endpoint_allow` enforced in `routes/storage/mounts.rs` create/patch; `MemberStanding` impl in `sr-db/src/scopes_repo.rs`; Postgres/unit tests.
2. Web: i18n fragment `p28-imm-flags-1.ts` (group label "Storage" in `admin/sections/Limits.tsx`), hide storage screens when the `platform.byos` client flag is off.
3. OPS-IMM-001, then OPS-IMM-002 (see the roadmap rows).

## Done
- `crates/sr-storage/src/mount_policy.rs`: `WhoMayMount`, `EndpointAllow`, `MemberStanding` (pure, 6 unit tests).
- `crates/sr-admin/src/settings.rs`: `byos.endpoint_allow`, `byos.require_client_remux`, `byos.who_may_mount` (group `storage`, with validation and 3 tests). `byos.presign_ttl_*` already existed. byos-browser adds `byos.max_mounts_per_user`, `byos.max_mounts_per_guild`, `byos.upload_parts_per_hour`: NOT added here.
- `crates/sr-features/src/byos.rs`: feature `platform.byos` (Platform + Guild scopes, default on, client visible), guild settings `guild_mounts` and `member_shelves_in_guild`; registered in `sr-server/src/app.rs::feature_registry`.

## Tests run
- `cargo test -p sr-features byos` (3), `-p sr-admin --lib settings` (9), `-p sr-storage --lib mount_policy` (6): green.

## Decisions
- The two guild choices are settings of the one feature `platform.byos` (not separate feature ids); a server's value is ANDed with the platform's.
- `byos.who_may_mount` empty string means `all`; an unrecognised stored value fails closed to `staff`.
