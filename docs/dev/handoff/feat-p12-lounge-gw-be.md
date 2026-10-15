# Handoff: feat/p12-lounge-gw-be (gateway, member, presence and settings follow-ups)

Branch `feat/p12-lounge-gw-be`, base `integration/phase12`. Checklist: `docs/dev/answers-followups-p12.md`.

## Done

| Item | What | Where |
|---|---|---|
| G-27 | Gateway age gate: message, reaction, poll, typing, pin and thread dispatches of an `nsfw` channel (or a thread of one) skip sessions without `age_confirmed_at`. The answer is cached per session and refreshed by `SR_USER_UPDATE`, which `POST /sr/age-gate` now publishes. | `sr-gateway/src/hub.rs`, `session.rs`, `traits.rs` (`channel_age_restricted`), `sr-api/.../auth/{account,common}.rs`, `sr-server/src/gateway_pg.rs` |
| G-28 | Temporary-member startup sweep after the 60 s grace (`TemporaryMembers::sweep`). The item names `temporary_members.rs`; the type lives in `gateway_pg.rs`. | `sr-server/src/gateway_pg.rs`, `app.rs` |
| G-34 | Backend was already built (member, ban and lazy-list user refs carry `primary_guild`). Web: tag chip with badge glyph and colour in the member list and next to author names. | `apps/web/src/lounge/ServerTag.tsx`, `liveMap.ts` |
| G-38 | `GUILD_CREATE` carries `stage_instances` and `soundboard_sounds`. | `sr-gateway/src/hub.rs` |
| G-40 | `CALL_CREATE` and `GET /channels/{id}/call` carry `video`; web preselects Accept with video. | `voice_call.rs`, `IncomingCall.tsx` |
| G-42 | System user hidden: member counts, REST list, lazy list, GUILD_CREATE members and prune leave it out (`NOT_SYSTEM_MEMBER`); kick, ban and role assignment answer 50013; its messages carry flag `SYSTEM_TAG` (1 << 30) and the web shows a `SYSTEM` badge. | `sr-db` (`accounts.rs`, `chat_repo.rs`, `guild_admin_repo.rs`), `sr-chat/src/admin_service.rs`, `sr-api/.../messages/model.rs` |
| G-45 | No Web Push while any session is connected and not idle (`Gateway::has_active_session`, `without_active`). | `sr-gateway/src/hub.rs`, `sr-server/src/push_notify.rs` |
| G-51 | Activity whitelist keeps `platform`, `party.id`, `expires_at_ms`; the web sends op 3 `afk` after 10 minutes and clears it on input. | `sr-gateway/src/presence.rs`, `apps/web/src/lounge/shell/idleReporter.ts` |
| G-52 | Migration `0792_member_profile_extras.sql`; `Member.extras`; `PATCH /guilds/{id}/members/@me` takes `pronouns`, `theme_colors`, `profile_effect`, `avatar_decoration`; member object and `guild_member_profile` return them; per-server profile editor fields. | `sr-chat`, `sr-db`, `sr-api`, `GuildProfileDialog.tsx` |
| G-53 | One badge catalogue (`sr_admin::BADGES`, `badge()`) with description and icon; profiles send `badges: [{id, description, icon}]`; connection stats restricted to `followers|subscribers|karma|stars|repos` (legacy `public_repos` reads as `repos`); web translates known ids and stats and falls back to server text and icon. | `sr-admin`, `sr-api/.../social/model.rs`, `userset/providers.rs`, `badgeModel.ts`, `useProfile.ts` |
| G-58 | Migration `0797_activity_settings.sql` (`activity_shares`, `activity_hides_restricted`; `cinema_share_level` honours them). Cinema watching presence, friends row and scoped trending, and library friends/known ratings use them. The Activity Privacy section writes `activity.*` to the settings blob at once. | `sr-cinema-social`, `sr-library-social`, `activitySettings.ts`, `sections/Activity.tsx` |
| G-17 (backend) | `PATCH /users/@me` accepts `locale` (BCP 47, normalised, pseudo-locales refused), stored on `accounts.locale`. | `sr-api/.../social/{profile,util,model}.rs`, `AuthStore::set_locale` |

Inventory: `LNG-PRF-013` ticked. Doc 05 updated (GUILD_CREATE extras, age gate, activity fields and settings, `locale`, member extras, badges and stats, `CALL_CREATE.video`). OpenAPI, TS client and mocks regenerated; no new routes, so the route matrix is unchanged.

## Not done or only partly

- G-38 web: the web has no stage or soundboard store yet, so there is nothing to seed. It can read the two new `GUILD_CREATE` fields when those stores land.
- G-52: validation is slug and range based because the profile catalogue (G-48) is not built; plug `sr_chat::profile::catalog` in `admin_rules.rs::apply_extras` when it lands. Avatar decoration has no editor field (the catalogue is empty).
- G-58: TV has no shareable server-side activity yet, so `activity.share_tv` is stored and exposed by `activity_shares(uid,'tv')` but nothing reads it. The profile Watching tab reads presence, which follows the same keys; its lists tab is user-authored, not activity. Library rankings are still one anonymous global aggregate (no scoped rankings exist to filter).
- G-51: `client_status` is not named by the item, so the web does not read it.
- G-17 web part lives on `feat/p12-web-followups`; this branch only adds the backend field.

## Decisions and notes for review

- Migration numbers: `0792` as named; `0797` for G-58 (first picked as 0796, renumbered because feat/p12-lounge-ch-be uses 0796; P12-LGB-4).
- `activity.hide_age_restricted` defaults to true on the server (matches the web default and D-190). Cinema tests that expect adult activity set it to false.
- `Member.extras` and `GuildMembership.extras` are `Box<...>` (keeps `InviteJoin` small for clippy).
- The system user id is the constant `sr_chat::SYSTEM_USER_ID = 1` for the kick, ban and role guards (the SQL filters use the `FLAG_SYSTEM` account flag).
- Push suppression treats a session that never sent op 3 as active; `dnd` and `invisible` sessions count as active too.
- Message flag `SYSTEM_TAG = 1 << 30` is a SIDE ROOM extension, set when the message object is built, never stored.

## Tests run

- `cargo fmt --check`; `cargo clippy -D warnings --all-targets` on sr-gateway, sr-chat, sr-admin, sr-auth, sr-db, sr-cinema-social, sr-library-social, sr-api, sr-server.
- `cargo test`: sr-gateway, sr-chat, sr-admin, sr-auth, sr-api (all binaries).
- Postgres (scratch DB, `--include-ignored --test-threads=1`): all of sr-db, sr-server, sr-cinema-social, sr-library-social, sr-api.
- Web: `tsc`, eslint, prettier, full vitest (`--maxWorkers=2`), `vite build` and `scripts/check-bundle-budget.mjs`.

## Open owner questions

None new. G-48 (profile catalogue artwork) still gates real effect and decoration slugs.
