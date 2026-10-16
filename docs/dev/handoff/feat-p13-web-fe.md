# Handoff: feat/p13-web-fe (web only)

Branch `feat/p13-web-fe`, base `integration/phase13`. Web only: no Rust, no cargo, no OpenAPI change. Strings are in
`apps/web/src/i18n/messages/en/p13-web-fe-1.ts` (settings) and `p13-web-fe-2.ts` (lounge).

## Audit result

Stale boxes ticked with a note naming the file: LNG-USET-042, LNG-USET-052, ENT-UI-020. Two notes in the Activity
section were also stale ("no route removes a single watch item / clears reading history"): `DELETE
/cinema/titles/{id}/progress`, `.../episodes/{id}/progress` and `DELETE /library/chapters/{id}/progress` exist, so the
per-item removal is now built.

## Done

| ID | What | Where |
| --- | --- | --- |
| LNG-USET-019 | Request my data: status, polling while building, download link, size, expiry | `settings/user/sections/DataExport.tsx` |
| LNG-USET-023 | Connections: provider grid (OAuth start, link-only forms), return notices, show on profile, show stats, refresh, remove | `settings/user/sections/Connections.tsx` |
| LNG-USET-009 | Avatar decoration picker and two-colour profile theme (rest was already built) | `settings/user/sections/Profile.tsx` |
| LNG-USET-035 (partly) | Per-item watch history removal, reading history card | `settings/user/sections/ActivityHistory.tsx` |
| LNG-USET-042 | Text-to-speech speed (`ttsRate` pref, used by "Speak message") | `prefs.ts`, `Accessibility.tsx`, `MessageItem.tsx` |
| LNG-MSGUI-018 | Cinema / Library rich cards via the markdown resolvers; Watch, Read, Watch together | `lounge/msg/RichCards.tsx` |
| LNG-EXP-011 (partly) | Custom emoji now render in messages (they were plain `:name:` text) and open a popout with name and source server | `lounge/msg/CustomEmoji.tsx`, new `customEmoji` resolver in `packages/markdown-react` |
| LNG-NTF-006 | Inbox For You tab: friend requests (accept/decline), followed-series chapters, submission decisions, redeemed invites | `lounge/inbox/ForYou.tsx` |
| LNG-NTF-009 | In-app DM toast with a settings switch (`dmToasts`) | `lounge/shell/DmToasts.tsx` |
| LNG-CMP-005 | Upload tray size check against `GET /donations/me` limits with a Patron hint (warns, never blocks) | `lounge/msg/uploadLimit.ts`, `UploadTray.tsx` |
| ENT-UI-022 | First-run welcome dialog: theme, picture, two-step nudge; once per account via the synced `welcomeDone` pref, only for accounts younger than 14 days | `lounge/shell/Welcome.tsx` |

## Still open, with reasons

- LNG-USET-011 Profile Layout: Phase 6 / EXT.
- LNG-USET-012 sensitive media filters, LNG-USET-013 DM spam filter and per-server DM override, LNG-USET-015 age
  confirmation state: no user-level API (see contracts).
- LNG-USET-035 rest: Cinema audio language and quality cap (player cannot apply them before playback), Library preload
  count (reader has no preload setting).
- LNG-USET-036 user-scope SchemaForm, LNG-USET-039 Theme Store entry, LNG-USET-049, LNG-USET-050: no API or not started.
- LNG-USET-040 only "show avatars in compact" is missing; LNG-USET-051 sticker autocomplete switch, "on hover for mine"
  spoilers and markdown preview are missing.
- LNG-EXP-011 "Join server" for public servers, LNG-EXP-012 burst reactions: need backend (below).
- LNG-CMP-001 (Lexical composer) and LNG-CMP-007 (resumable multipart, no endpoint) are too large or backend-blocked.
- ENT-UI-010 / ENT-UI-025 operator-editable copy: needs a copy API. ENT-UI-026 is a Playwright test (not run: shared ports).
- Keyword highlights UI: the engine supports it but there is still no per-user keyword list route, so no UI.

## Backend contracts wanted

1. User-level safety settings (USET-012, 013, 015): extend `PATCH /users/@me/privacy` (or a new route) with
   `sensitive_media: { friends: show|blur|block, others: show|blur|block, servers: show|blur|block }`,
   `dm_spam_filter: bool`, `server_dm_overrides: { <guild_id>: bool }`; and `GET /users/@me/age-confirmation` returning
   `{ confirmed_at, method }` plus `DELETE` to reset. Today only the per-server `explicit_media_filter_dms` and
   `dm_spam_filter` under `/guilds/{id}/sr/community` exist.
2. Burst reactions (EXP-012): `PUT .../reactions/{emoji}/@me?type=1` (burst), `burst: true` / `count_details` in the
   reaction object, a burst allowance, gateway events carrying `burst`.
3. Emoji popout "Join server" (EXP-011): `GET /emojis/{emoji_id}` for non-members returning `{ name, animated,
   guild: { id, name, icon, public_invite } }`; also a signed image URL for emoji of servers the viewer is not in, so
   cross-server emoji can draw (today they stay `:name:` text).
4. Operator copy (ENT-UI-010/025): public `GET /sr/entrance/copy` -> `{ key: markdown }` for `no-invitation` and
   `house-notes` plus landing strings, and `PUT /admin/entrance/copy/{key}` (staff) with defaults served by the client.
5. OpenAPI naming collision found: `PUT /users/@me/connections/link` references the schema `LinkBody`, which resolves to
   the Cinema staff `LinkBody` (`title_id`, `episode_id`) instead of the connections one (`provider`, `name`, `url`).
   Give the connections struct a distinct `schema(as = ...)` name. The web client uses the real fields.

## Decisions

- Custom emoji resolve only through the message's own server (`/guilds/{id}/emojis/{id}/url`), so DMs and other servers'
  emoji keep the old `:name:` text. The new `customEmoji` markdown resolver lets the app take over a node and falls back
  to the old behaviour when it returns `undefined`.
- Upload size check only warns: server boosts can raise the limit inside a server, and the server answers 413 anyway.
  `/donations/me` is read lazily, only once a file over 8 MiB is in the tray.
- Welcome dialog uses the platform snowflake epoch to decide "new account" (14 days) so existing members are not greeted.
- `dmToasts` and `welcomeDone` are new synced prefs; `ttsRate` too. Prefs key order in `sanitizePrefs` must match
  `DEFAULT_PREFS` (`prefsEqual` compares JSON).

## Tests run

Vitest (`--maxWorkers=2`): new `p13connections`, `p13profile`, `p13history` (settings), `p13RichCards`, `p13Inbox`,
`p13DmToasts`, `p13UploadTray`, `p13Welcome` (lounge), one case in `packages/markdown-react`. Updated tests that
asserted the old "not available" copy (privacy, account, prefsSections). Full web suite: 167 files, 1804 tests passed. Also green: `tsc --noEmit`, eslint (0 warnings), prettier --check, `vite build` and `scripts/check-bundle-budget.mjs` (lounge 367 KB of 600, settings 263 KB of 600, entrance 212 KB of 300). The wasm `dist` was copied from the main checkout (no cargo); Playwright was not run.

## Open owner questions

- Should the welcome dialog also cover accounts created before this release? (Currently: no, 14-day window.)
- Should oversize files be blocked client-side once the effective per-channel limit is exposed by the API?
