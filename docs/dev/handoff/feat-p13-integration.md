# Handoff: integration/phase13 (first batch)

Branch `integration/phase13` (base `main` at `337b49a`). Local only: nothing pushed, no PR.

## Merged (merge commits, in this order)

`feat/p13-answers`, `feat/p13-events`, `feat/p13-mfa-protect`, `feat/p13-library`, `feat/p13-web-fe`,
`feat/p13-settings-admin`, `feat/p13-web-ui2` (contains settings-admin), `feat/p13-h-web` (finished late, merged after
web-ui2), `feat/p13-devplat`, `feat/p13-voice`, `feat/p13-foundation` (finished late, merged last). Not merged (still
running): `feat/p13-review`.

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
- **Migrations.** 0820, 0825, 0830-0832, 0840, 0842-0844, 0850, 0851, 0860, 0880 (foundation); no 0841, no duplicate prefix
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

## Gates (final tree, after the foundation merge)

- `cargo fmt --check` ok; `cargo clippy --workspace --all-targets -D warnings` ok.
- `cargo test --workspace -j 2`: 2384 passed, 0 failed, 357 ignored.
- Postgres (own scratch database, `--include-ignored --test-threads=1`, dropped afterwards): sr-db 219, sr-server 93,
  sr-library-social 23, sr-api 627, sr-storage 65, all passed.
- OpenAPI, `schema.d.ts`, MSW handlers regenerated (790 operations; `mocks:check` ok); route matrix regenerated, no `?`,
  `route_matrix` 5 tests pass. WASM package rebuilt (bit 48 present).
- Web: install offline, `pnpm -r typecheck` ok, `pnpm -r lint` ok, prettier (apps packages docs scripts deploy) ok,
  vitest apps/web `--maxWorkers=2`: 195 files, 1997 tests passed (`signing.test.ts` passes with foundation's fix),
  package tests (api-client 2, markdown-react 3, wasm 3, bot 1, ui 14 files) passed, `vite build` and bundle budget ok
  (lounge 384.5 KB of 600), `i18n-verify-diff` MISSING 0, doc links ok, ops files ok, `render-caddyfile --check` ok,
  `install.test.mjs` 6 and `ci-scripts.test.mjs` 5 passed. Build works with the new `VITE_*` values unset.
- `cargo deny check`: advisories, bans, licenses, sources ok.

## Not verified

- Playwright (shared ports, per the brief); real browsers for the voice video path (voice handoff).
- One `sr-voice` e2e failure (`ice_tcp_reaches_the_shard_that_owns_the_participant`) was seen once under load before
  the port retry and not reproduced in 8 later runs; it may be a separate load-timing flake.
- Doc 05's "no TypeScript copy of the epoch" is only half met (one copy plus a drift test).
- The Share to Lounge channel picker does not hide channels without `SEND_MESSAGES`/`USE_LIBRARY_EMBEDS`.
