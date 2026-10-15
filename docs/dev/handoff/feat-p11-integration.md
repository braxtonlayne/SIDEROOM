# Handoff: integration/phase11 (wave 11 integration)

Branch `integration/phase11` (was `main` at `afb76f7`). Ten branches merged one at a time, each as a merge commit, in this order: `feat/p11-userset-be`, `feat/p11-msgs`, `feat/p11-featmod`, `feat/p11-featmod-b`, `feat/p11-library`, `feat/p11-review`, `feat/p11-devplat`, `feat/p11-shell`, `feat/p11-userset-fe`, `feat/p11-p10-client`. Local only: not pushed, no PR.

## Merge conflicts and how they were resolved

| Merge | Files | Resolution |
|---|---|---|
| msgs | `sr-core/config.rs` (secret file keys), `sr-server/Cargo.toml`, `sr-server/app.rs`, `sr-api/routes/mod.rs`, route matrix, MSW handlers | Union. `app.rs`: one `.extension(social_api.clone())` (the union had it twice) plus connections/export/push/gif/inbox deps. |
| featmod-b | `app.rs` registration, `sr-api` routes/mod/Cargo, `sr-server/lib.rs`, `threads/start.rs`, `ChannelHeader.tsx`, inventory 09 | Union (both module sets kept, featmod-b not rewritten onto the framework). `announce_started` stays `pub` (featmod needed it wider than featmod-b's `pub(crate)`). Inventory by tick union. |
| library | `sr-server/Cargo.toml`, generated client | Union; generated files regenerated at the end. |
| review | `Cargo.lock`, `library/routes.tsx`, route matrix | Union. Library keeps `ListsPage` and `RankingsPage` (library) and the requests board (review); the `SoonPage` placeholders for lists/requests are gone. |
| devplat | `sr-net` client/lib, `sr-gateway` conn/tests, `Composer.tsx`, `Cargo.lock`, `sr-server/Cargo.toml` | `HttpTransport` has both `send` (userset-be `SafeClient::call`) and `post` (devplat SSRF-safe POST); `sr-net` re-exports both sets. Gateway: `create_session(.., shard)` (devplat) followed by `set_session_platform` (userset-be). Gateway tests: both blocks (presence and sharding). Composer: schedule dialog (featmod-b) and app command form (devplat) both mounted. |
| shell | inventory 03 | Tick union. |
| userset-fe | `LoungeShell.tsx` | See "Desktop notifications" below. |
| p10-client | `MessageItem.tsx`, `msg/Components.tsx` | devplat's `context` prop (channel + message, absent for ephemeral messages) also drives p10-client's staff-card `sr:` buttons; `channelId`/`messageId` props removed, `staffCards.test.tsx` updated. |

## Overlaps resolved after merging

1. **featmod vs featmod-b.** Both register into the one `FeatureRegistry`; the registration moved into `sr_server::app::feature_registry()` with a unit test that framework modules (`lounge.starboard`, `lounge.welcome`) and sr-userfeat modules (`chat.reminders`, `community.tickets`) are all present with guild scope, which is what `GET /guilds/{id}/sr/features` lists. No id collisions (sr-features uses `lounge.*`/`moderation.*`, sr-userfeat `chat.*`/`community.*`).
2. **Announcements.** One source of truth: the Lounge notices bar (`lounge/shell/Banners.tsx`, LNG-SHL-009) now uses p10-client's `useActiveAnnouncements` (polls every 5 minutes, critical notices cannot be dismissed, same localStorage key). The shell's own fetch and the unmounted `AnnouncementBanner`/`AnnouncementList` were removed (and the now unused `lounge.announcements.label` key). `announcements.test.tsx` tests the hook; `p11ShellNotices.test.tsx` also checks that a critical notice has no dismiss button.
3. **Re-auth host.** `ReauthHost` is mounted at the router root as a lazy chunk (renders nothing until asked), so Lounge staff-card buttons using `withReauth` can open the second-factor dialog. `registerReauthPrompt` is now a stack, so the admin shell's own host (kept, its tests rely on it) answers inside `/admin` and the root host everywhere else.
4. **Desktop notifications (found during the merge).** shell's `useDesktopNotify` (LNG-NTF-007) and userset-fe's `useDesktopAlerts` (LNG-USET-052) both raised a browser `Notification` for each DM. `useDesktopNotify` keeps notifications and sounds (it applies server levels and mutes) and now honours Streamer Mode's silence; `useDesktopAlerts` keeps only the blinking tab title. The unused `pwa.alerts.title` key was removed; `desktopAlerts.test.tsx` updated.
5. **Profile contract (shell UI vs userset-be API).** Mismatches fixed on the web side (`lounge/profile/*`), accepting both shapes:
   - `badges` are id strings (`owner`, `staff`, `patron`, `contributor`, `early_member`, `bug_hunter`, `honorary`, `app_developer`, `bot`), not `{id, description}` objects; labels added for `owner` (platform owner, distinct from the derived server-owner badge), `bug_hunter`, `honorary`, `bot` (fragment `p11-integration-1.ts`).
   - "Member since" reads `member_since` (shell read `created_at`).
   - Decoration and effect are slugs in `user_profile.avatar_decoration` / `user_profile.profile_effect` (shell read `user.avatar_decoration_data.asset` and `{id}`).
   - `connected_accounts[].stats` is an object (`{"followers": 12}`), rendered as "12 followers"; `url` typed.
   - `guild_member_profile` has `joined_at` (used as a fallback for "joined this server"); it has no pronouns/accent/theme/effect per server.
   - Presence: activity types 0-5 plus 6 = Reading match `liveMap.ts`. `client_status` is sent but unused by the web.
6. **Group DM.** Exists: `owner_id` and `icon` on the DM channel object, `PUT/DELETE /channels/{id}/recipients/{user}`, and the edit route `PATCH /users/@me/channels/{id}` (name, icon key, owner transfer). Shell called `PATCH /channels/{id}` (guild channels only): fixed to the `/users/@me/channels/{id}` route. See gaps for icons and invites.

## Other checks
- Migrations unique and ordered: 0660, 0670, 0680, 0690, 0691, 0700, 0701, 0710, 0760.
- Scheduler job ids unique across sr-server (1-5, 110, 120 poll sweep, 130-132, 141, 150, 160-162, 172, 540/541, 630 insights, 660/661 userset, 701 review, 760 userfeat); job kind names unique.
- Route matrix regenerated with `SR_UPDATE_ROUTE_MATRIX=1`: no change from the union, no `?` cells, no duplicate METHOD+PATH. OpenAPI, `schema.d.ts` and MSW handlers regenerated.
- Duplicate manifest keys from the union removed (`sr-net` in sr-api and sr-server, `bytes` in sr-server dev-deps); `userfeat_pg_tests` had lost its `#[cfg(test)]`.
- Crash damage: no NUL bytes in any tracked text file (`git grep -lP '\x00'` lists only binary assets); every file changed by the wave ends with a newline; no conflict markers left; every compile/typecheck passes. All inventory ticks from every branch survive.
- i18n: no duplicate keys across fragments.

## Gaps left for owners / next wave
- Group DM icon: the server stores `icon` verbatim as a key and does not accept an image (data URI) like guild icons do; the web hides "Change icon" (`GROUP_ICON_UPLOAD` in `DmPanels.tsx`) until it does. The server also accepts any string as the key (worth validating).
- Group DM invite links: no backend route (`POST /channels/{id}/invites` is guild-only); the panel shows its error state.
- Presence: the web never sends op 3 with `afk: true`, so the per-session idle merge is not driven by the client yet; the gateway's activity whitelist drops `platform`, `party` and `expires_at_ms`, which `liveMap.ts` reads (no producer sends them today).
- Per-server profile has no pronouns/theme/effect overrides (backend), so those come from the account profile.
- Badge descriptions are client labels; the server sends no tooltip text.
- Connection stats keys (`followers`, `karma`, ...) are shown as sent (English words), not translated.

## Also fixed during the gates
- `sr-db/tests/secret_columns.rs` failed on msgs' `0670_web_push.sql: secret`: that column is the VAPID private key, sealed with the server secret by `sr-push`; added to `PLAINTEXT_OK` with that justification.
- `staffCards.test.tsx` prettier.

## Gate results (run one at a time, `-j 2`, `CARGO_PROFILE_DEV_DEBUG=0`)
| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo test --workspace --no-fail-fast` (no Postgres) | 2145 passed, 0 failed, 301 ignored (131 test binaries) |
| Postgres suites (sr-db, sr-server, sr-chat, sr-api, sr-push, sr-userfeat, sr-library-social, sr-features, sr-review, sr-apps; `--include-ignored --test-threads=1`, fresh scratch DB) | 1337 passed, 0 failed (56 binaries) |
| `pnpm install --offline --frozen-lockfile` | ok |
| wasm build, `pnpm -r typecheck`, `pnpm -r lint` | clean |
| `prettier --check apps packages docs scripts` | clean |
| vitest apps/web (`--maxWorkers=2`) | 141 files, 1635 tests passed, no reruns needed |
| web build + `check-bundle-budget.mjs` | all budgets ok (lounge 344 KB / 600, entrance 209 KB / 300, admin 282 KB / 600) |
| `i18n-verify-diff.mjs`, catalog test | MISSING 0, passes; no duplicate keys |
| api-client tests, `mocks:check` | 6 passed; handlers up to date (702 operations) |
| Route matrix (`SR_UPDATE_ROUTE_MATRIX=1`) | unchanged, no `?` cells |

`cinema/player/__tests__/signing.test.ts` passes here (5/5, Node 24.21, after a fresh wasm build); nothing in this wave touches `cinema/player`, so the failure userset-fe saw is not reproducible on this tree (likely its worktree environment). Playwright was not run (by instruction).
