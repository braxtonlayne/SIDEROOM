# Handoff: integration/phase13

Branch `integration/phase13` (base `main` at `337b49a`). Local only: nothing pushed, no PR.

## Merged (merge commits, in this order)

`feat/p13-answers`, `feat/p13-events`, `feat/p13-mfa-protect`, `feat/p13-library`, `feat/p13-web-fe`,
`feat/p13-settings-admin`, `feat/p13-web-ui2` (contains settings-admin), `feat/p13-h-web` (finished late, merged after
web-ui2), `feat/p13-devplat`, `feat/p13-voice`, `feat/p13-foundation`, `feat/p13-review` (the last two finished late and were merged at the end). All
twelve phase 13 branches are in.

## Overlaps and how they were resolved

- **Custom emoji.** One resolver API in `packages/markdown-react`: `customEmoji?: (emoji) => ReactNode | undefined`
  (web-fe's semantics: `undefined` falls back to `customEmojiUrl` and `:name:`). The rendering and popout are web-ui2's
  server-backed `lounge/msg/CustomEmoji.tsx` (`/sr/emojis/{id}/url`, `/sr/emojis/{id}`, Join for listed servers).
  web-fe's per-guild `customEmojiResolver`, its `.lng-emoji-pop*` CSS (its `.lng-emoji-btn` also clashed with the
  emoji picker's class) and its duplicate `lounge.emojiPopout.*` catalog keys (a duplicate key makes the catalog merge
  throw) were dropped. web-fe's two emoji tests in `p13RichCards.test.tsx` now exercise the merged component.
- **Snowflake epoch.** One TypeScript source: `SIDEROOM_EPOCH_MS` in `apps/web/src/lounge/liveMap.ts`
  (settings-admin). `Welcome.tsx` (web-fe), `requestReason.ts` (mfa-protect) and `securityNudge.ts` (h-web) import it.
  New Rust test `sr-core` `snowflake::tests::web_epoch_copy_matches` fails if the web value drifts from the Rust one.
  Doc 05 still says no TypeScript copy at all (WASM or generated constant); that stronger form is not done.
- **Share to Lounge cards (D-226).** library posted a plain URL, which the Lounge renders as a link, not a card (the
  card nodes come only from `<library:id>` / `<cinema:id>`). `ShareTarget` gained `card`; the dialog now posts the
  token (plus the note), and a "Copy link" button copies the URL (D-226 second item; strings in
  `i18n/messages/en/p13-integration-1.ts`). Test `library/__tests__/p13.test.tsx` expects `<library:1>`.
  Not done: the channel picker is not filtered by `SEND_MESSAGES`/`USE_LIBRARY_EMBEDS` (the server still refuses).
- **DM routing (mfa-protect G-95 vs settings-admin D-230).** `SocialService::open_dm` runs the server Protection rule
  first (`dm_access_protected`, keeps `sr_request_reason`), then the user's `dm_spam_filter` level on top.
- **Guild/DM objects.** Both `hide_suspicious_links` (mfa-protect) and `approximate_*_count`/`sr_max_upload_bytes`
  (settings-admin) kept; `sr_request_reason` and `sr_max_upload_bytes` both on the DM object.
- **MFA.** mfa-protect's `MfaGate` (900123/900004) and `withReauth` paths and settings-admin/web-ui2's admin routes
  with a fresh second factor are both in; there was no textual conflict.
- **Migrations.** 0820, 0825, 0830-0832, 0840, 0842-0844, 0850, 0851, 0860, 0880 (foundation), 0890-0892 (review); no 0841, no duplicate prefix
  (`sr-db` `migration_versions` test).
- **Permission bits.** Only voice added one (`SET_VOICE_CHANNEL_STATUS`, bit 48); no collision in `sr-core` or
  `apps/web/src/settings/server/permissions.ts`. WASM package rebuilt.
- **h-web doc rows (doc 05).** Checked against the merged code and corrected: upload limit (perk only, REST only,
  not in `GUILD_CREATE`), privacy (`sr_filtered` per-viewer filtering not built), reaction shim (no `burst` field on
  the reaction object), D-228 creator-interested (not built), `GET /tv/reminders` and `tv.reminder_lead_minutes`
  (not built), `library.share_reading` (not built; `cfi` is).
- **Route matrix (foundation G-19).** Foundation's AUTHZ vocabulary and handler-signature test kept. The 25 rows the
  other branches added as `other` now say who may call them: comment edit/delete `author`; likes, comment lists and
  posts, reports, contributors, related, chapter download (Patron check in the service), App Directory and
  `/sr/emojis/{id}/url` `user`; presence, downloads list, notifications, friends reading and recommendations
  `self`; `/sr/emojis/{id}` `visibility` (source server named only for members or listed servers).
- **Review conflicts.** Route matrix: foundation's vocabulary kept; review's new rows: upload delete, duplicates,
  inspect and parts `author`, `/submissions/lookup` and `/providers` `user` (`MemberCtx`; the platform setting gates
  use). `sr-server` `app.rs`: library's trending cache and review's `with_sources` (unwrapped `LibraryDeps`). Library
  routes keep the comment-report routes and the upload abort/parts/inspect routes. `Submissions.tsx` runs the
  browser CBZ pre-check (library) before handing big files to the resumable upload manager (review).
  `sr-library` depends on both `sr-library-rules` and `sr-reader`. Test files: both sides' tests kept.
- **Foundation conflicts.** `Cargo.lock` regenerated offline (sr-library-wasm and sr-loadtest both present);
  `admin.css` keeps the chapter page editor and the unsaved bar; the `Caddyfile.prod` header names both the
  render script (h-web) and `activities.snippet` (foundation).
- **Generated files** (`openapi.json`, `schema.d.ts`, MSW handlers) regenerated from the merged Rust; route matrix
  regenerated (no change, no `?`). Checklists and inventories: tick union.

## Other fixes

`age_confirmed_at` added to the `AccountRecord` fixtures in `developer/tests.rs` and `tests/lounge.rs`; test builder
conflicts in `developer/tests.rs` and `settings/tests.rs` combined (CDN deps, rich store, signer); unused import in
`Privacy.tsx`; rustfmt.

Test fixes: `open_dm_routes_by_protection_and_spoilers_follow_the_media_filter` (mfa-protect) assumed a stranger's DM is
allowed when no server filters; with D-230's default `dm_spam_filter: non_friends` (settings-admin) it is a request, so
the test now sets the filter `off`. The sharded `sr-voice` e2e tests failed under parallel load when another test took
the second UDP port between `free_port_pair` and `start` (one shard instead of two); `start_sharded` now retries with a
fresh pair.

## Gates (first batch, all twelve branches merged, before the H branches)

- `cargo fmt --check` ok; `cargo clippy --workspace --all-targets -D warnings` ok.
- `cargo test --workspace -j 2`: 2422 passed, 0 failed, 364 ignored.
- Postgres (own scratch databases, `--include-ignored --test-threads=1`, dropped afterwards): sr-db 226, sr-server 95,
  sr-library-social 23, sr-api 643, sr-storage 67 passed. sr-cli: 14 of 14 on a fresh database; on the database the
  other suites had used, `staff_roles_are_created_once_and_repaired_on_rerun` fails because a Staff Server already
  exists (unique index `guilds_staff_guild_uidx`): a pre-existing isolation issue of that test (it uses the shared URL
  directly, not a `TestDb` clone), not a phase 13 change.
- OpenAPI, `schema.d.ts`, MSW handlers regenerated (808 operations; `mocks:check` ok); route matrix regenerated, no
  `?`, no `other`, `route_matrix` 5 tests pass. WASM package rebuilt.
- Web: install offline, `pnpm -r typecheck` ok, `pnpm -r lint` ok, prettier (apps packages docs scripts deploy) ok,
  vitest apps/web `--maxWorkers=2`: 200 files, 2016 tests passed (`signing.test.ts` passes with foundation's fix);
  api-client 6, markdown-react 45, wasm 9, bot 15, ui 278 tests passed; `vite build` and bundle budget ok (lounge
  384.6 KB of 600, admin 312.3 KB of 600); `i18n-verify-diff` MISSING 0; doc links ok; ops files ok;
  `render-caddyfile --check` ok; `install.test.mjs` 6 and `ci-scripts.test.mjs` 5 passed. The build works with the
  new `VITE_*` values unset.
- `cargo deny check`: advisories, bans, licenses, sources ok.

## Not verified

- Playwright (shared ports, per the brief); real browsers for the voice video path (voice handoff).
- One `sr-voice` e2e failure (`ice_tcp_reaches_the_shard_that_owns_the_participant`) was seen once under load before
  the port retry and not reproduced in 8 later runs; it may be a separate load-timing flake.
- Doc 05's "no TypeScript copy of the epoch" is only half met (one copy plus a drift test).
- The Share to Lounge channel picker does not hide channels without `SEND_MESSAGES`/`USE_LIBRARY_EMBEDS`.

## Second batch: feat/p13-h-a, feat/p13-h-b, feat/p13-h-c

Merged on top of `40fa908` as merge commits, in this order: `feat/p13-h-a`, `feat/p13-h-b`, `feat/p13-h-c`.

- **Conflicts.** h-a and h-b merged without a conflict (route matrix, generated client, doc 05, `app.rs`, checklists and i18n
  fragments all merged textually). h-c conflicted only in doc 05 section 3: the snowflake epoch row takes h-c's wording (the
  generated `apps/web/src/lounge/epoch.generated.ts`, checked by `web_epoch_file_is_generated_from_rust`) and the upload limit row
  takes h-b's (D-238, `Entitlements::limits(user, guild).upload_bytes`). The stale "Not built yet: applying `sensitive_media`
  per viewer" sentence in the privacy row was replaced with what H-33 built.
- **Migrations.** 0870, 0871, 0873, 0879 (h-a) and 0874, 0875 (h-b); no duplicate prefix (`migration_versions` passes).
- **Permission bits.** `PlatformPermissions::MANAGE_ENTRANCE = 1 << 24` (h-b) is free among the platform bits (0 to 23 and 63);
  the guild `MOVE_MEMBERS = 1 << 24` is a different bitfield. `apps/web/src/admin/permissions.ts` matches.
- **Epoch.** One source: the Rust constant, written to `epoch.generated.ts`; `liveMap.ts`, `Welcome.tsx`, `requestReason.ts` and
  `securityNudge.ts` import it. Doc 05's "no TypeScript copy" rule is now met (generated file plus drift test).
- **Generated files.** OpenAPI, `schema.d.ts` and MSW handlers regenerated from the merged Rust (814 operations): identical to the
  textual merge. Route matrix regenerated: no change, 849 rows, no `?`, no `other`.
- **Fixes.** None needed beyond the doc 05 resolution.

## Gates (final tree, all fifteen branches merged)

- `cargo fmt --check` ok; `cargo clippy --workspace --all-targets -j 2 -D warnings` ok.
- `cargo test --workspace -j 2 --no-fail-fast`: 2463 passed, 0 failed, 374 ignored (146 test binaries).
- Postgres (own scratch database `sr_p13int2`, created and dropped; `--include-ignored --test-threads=1`): sr-db 233, sr-server 96,
  sr-api 661, sr-library-social 25, sr-storage 67, sr-tv 55, sr-userfeat 48, sr-watchparty 62, sr-cinema-social 30, sr-cli 14
  passed, 0 failed (sr-cli now uses a `TestDb` clone, h-c). `TestDb` templates (`sr_tpl_*`) stay on the server by design.
- `cargo deny check`: advisories, bans, licenses, sources ok.
- Web: install offline, WASM package rebuilt, `pnpm -r typecheck` ok, `pnpm -r lint` ok, prettier (apps packages docs scripts
  deploy) ok, vitest apps/web `--maxWorkers=2`: 208 files, 2043 tests passed (no flakes); api-client 6, markdown-react 45, wasm 9,
  bot 18, ui 278 passed; `vite build` and bundle budget ok (lounge 385.6 KB, admin 315.1 KB of 600); `i18n-verify-diff` MISSING
  0 (i18n tests 46); `mocks:check` ok; doc links ok; ops files ok; `render-caddyfile --check` ok; `install.test.mjs` and
  `ci-scripts.test.mjs` 11 passed.

## Not verified (second batch)

- Playwright (shared ports, per the brief); real browsers for the voice video path.
- The op 8 `REQUEST_GUILD_MEMBERS` server handler does not exist (h-c built the SDK side only).
- The S3 live tests in sr-storage need a bucket; without one they return early (CI `storage.yml` covers them).
- `docs/dev/answers-followups-p13.md` still has 13 unticked boxes (H-04 to H-10, H-19, H-20, H-22 to H-25) although several look
  built (for example `ConnectionLinkBody`, the reaction shim, `/sr/entrance/copy`, the EPUB reader, H-04 in `e92c5c2`). They were
  not re-audited here; the owner or a follow-up pass should tick what is done.
