# Handoff: integration/phase12

Branch `integration/phase12`. Merged in order: feat/p12-answers (D-185 to D-224, G-89 to G-98 answers), feat/p12-firstpaint, feat/p12-admin-ui, feat/p12-feat-ui, feat/p12-web-followups (with G-79, G-80, G-83, G-86, G-88), feat/p12-admin-mod-be, feat/p12-devplat-be, then feat/p12-lounge-gw-be, feat/p12-lounge-ch-be, feat/p12-tv-wp-be and feat/p12-g68-be. This note covers the last four merges and the first full build of the merged tree. The branch handoffs are in this folder.

## Conflicts and overlaps

- **Server tag chip built twice.** feat-ui's `lounge/components/ServerTagChip.tsx` with `liveMap.serverTagOf` stays. It honours `identity_enabled: false`, trims and upper-cases the tag, colours only the badge glyph so a low-contrast colour cannot hide the letters, and has a translated title. lounge-gw-be's `lounge/ServerTag.tsx` is deleted. Its G-34 cases in `p12LoungeGw.test.tsx` now render `ServerTagChip` from a `toLoungeUser` result. `primary_guild.badge/color` keep feat-ui's `number | null` typing.
- Kept from lounge-gw-be: `LoungeUser.system` and the `SYSTEM` badge, the idle reporter, activity fields and settings, per-server profile extras, the badge catalogue, locale, and the video call preselect.
- `docs/05-api-and-gateway.md`: the activity paragraph keeps type 6 Reading (D-193) and adds the gateway whitelist and `afk` text. The REST table keeps `GET /sr/features/flags` and adds the profile catalogue and `/sr/auth/reauth` rows.
- `docs/dev/answers-followups-p12.md`: merged item by item (three times), keeping the ticked version of every G item. G-01 to G-98 each appear once.
- lounge-ch-be: `LoungeDialog` has both `community` and `events`/`createEvent`. `ServerInvite.tsx` keeps the translated strings (`entrance.serverInvite.*`) and the group invite branch. `feature_actions_pg_tests.rs` keeps both new tests.
- tv-wp-be: `create_channel` takes both `MfaGate` and `MaybeExt<WatchPartyDeps>`. Doc 03 keeps the in-house Bitcoin row (D-207) and adds the `jiff` row. The route matrix keeps the bot-denied birthday and bookmark rows.
- g68-be: `Guild` (sr-chat domain, test fixtures, `chat_repo` columns) keeps both the G-36 safety fields (`mfa_level`, `system_channel_flags`, widget) and the G-76 community channel ids. Its web changes (Community dialog level roles, `can_respond`, suggestion votes; the updates badge; announcements) merged without conflicts on top of feat-ui and web-followups, so g68's server-driven versions are in the tree.
- **Error code 900122.** g68 uses `900122 SrTesterRecentlyDeclined` (409, G-84). D-223 (decisions log), its answer in `open-question-answers.md`, and G-94, G-97 and G-98 had reserved 900122 for `MFA_ENROLLMENT_REQUIRED`, which is not built yet. Those docs now name `900123`, the next free 9001xx code.
- Generated files (OpenAPI, `schema.d.ts`, MSW handlers) were regenerated from the merged code, not hand-merged.
- The group invite strings (`entrance.invite.group*`) moved from `p12-lounge-ch-be-1.ts` to the core `entrance.ts` fragment. `/invite/{code}` is an entrance route, and `i18n/firstpaint.test.tsx` requires its keys to be in the core fragments.
- `ChannelSidebar.tsx` had `openDialog` declared twice after the merges (feat-ui and ch-be both added it). One is removed.

## Integration work

- **G-48 hook into G-52.** `sr_chat::admin_rules::apply_extras` now checks per-server `profile_effect` and `avatar_decoration` against `sr_chat::profile::catalog`. It returns the same error as `PATCH /users/@me`: `ENUM_TYPE_BASE_TYPE_BAD_VALUE`, "not in the profile catalogue". The unit test and `sr-api/tests/lounge.rs` now refuse `neon-halo` and `halo-1`. The decoration list stays empty until artwork exists.

## Checks

- Migrations are unique: 0760, 0770, 0771, 0780, 0781, 0790 to 0797, 0800, 0810, 0811 (0796 group DM invites, 0797 activity settings).
- Job singleton ids are unique (g68 adds 810, `apps.housekeeping`). The only repeated job kind string is `cinema.probe`, which is the producer and consumer constant of one job, as before.
- The route matrix was regenerated. Only row order changed and there are no `?` cells, so there are no duplicate or unreviewed routes.

## Gate results (after the last merge, feat/p12-g68-be)

- `cargo fmt --check` passes. `cargo clippy --workspace --all-targets -j 2 -- -D warnings` passes; no fixes were needed.
- OpenAPI regenerated (735 operations in the MSW handlers), `schema.d.ts` regenerated, and `mocks:check` reports up to date. The route matrix was regenerated with `SR_UPDATE_ROUTE_MATRIX=1`: route matrix tests 3/3.
- `cargo test --workspace -j 2`: 2233 passed, 0 failed, 333 ignored (the Postgres tests), across 133 test binaries.
- Postgres, on a scratch DB of its own (created and dropped), with `--include-ignored --test-threads=1`. Crates: sr-db, sr-server, sr-chat, sr-api, sr-apps, sr-admin, sr-tv, sr-watchparty, sr-payments, sr-features, sr-gateway, sr-cinema-social, sr-library-social, sr-library, sr-review, sr-userfeat. Result: 1934 passed, 0 failed, across 73 binaries. An earlier run before the g68 merge failed `staff_feed_pg_tests::the_health_check_alerts_about_a_missing_backup_once_a_day`. The cause was drive C: above 99 % full, so the disk alert fired before the backup alert. The test passed once space was freed.
- Web: `pnpm install --offline --frozen-lockfile`, `pnpm -r typecheck`, `pnpm -r lint` and `prettier --check apps packages docs scripts` all pass. Vitest `apps/web --maxWorkers=2`: 1783 of 1784 passed. The one failure was the firstpaint key check above, which is now fixed; after the fix `src/i18n` (46) and the invite and route tests (60) pass. `vite build` and `scripts/check-bundle-budget.mjs`: all budgets ok (lounge 360.3 KB, settings 260.0 KB, entrance 211.8 KB gzip). `scripts/i18n-verify-diff.mjs`: MISSING 0. api-client tests 6/6.

## Not verified

- Playwright (not run, as instructed; the ports are shared).
- The whole vitest suite was not rerun after the key move, only the affected files.
- `cargo deny` was not rerun here (tv-wp-be ran it after adding `jiff`).
