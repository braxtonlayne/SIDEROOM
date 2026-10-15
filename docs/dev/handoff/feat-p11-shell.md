# Handoff: feat/p11-shell (agent p11-shell)

Scope: every unchecked `[FE]` item of `docs/inventory/03-lounge-shell.md` (web only: `apps/web/src/lounge`, store, i18n). Base: `integration/phase11`.

## Done (all ticked in the inventory)

| ID | What | Where |
|---|---|---|
| LNG-SHL-009 | Notices bar: verify email (as before), "new version ready" from the PWA state, platform announcements from `GET /announcements/active`, each dismissible (announcement dismissals kept per browser) | `lounge/shell/Banners.tsx` |
| LNG-RAIL-011 | Discover button opens the Server Directory: search, categories, paging, Join without invite, Open when already a member. A modal, not a routed page | `lounge/shell/DirectoryDialog.tsx`, `directory.ts` |
| LNG-RAIL-012 | Guild context menu now has Server Settings submenu (section shortcuts), Privacy Settings (opens the user Privacy section), Edit Per-server Profile (nick, bio, avatar, banner via `PATCH /guilds/{id}/members/@me`), Create Category; the header dropdown uses the same real entries | `lounge/shell/useGuildMenu.ts`, `GuildProfileDialog.tsx` |
| LNG-RAIL-015 | Rail "Install app" button (browser prompt, iOS Add to Home Screen steps) | `lounge/shell/InstallButton.tsx` |
| LNG-HOME-004 | "Support SIDE ROOM" nav item, shown only when `GET /donations/config` says `enabled` (the operator toggle) | `HomeSidebar.tsx`, `shell/useSupportEnabled.ts` |
| LNG-HOME-012/013 | Arrow beside mic and headphone: device list, volume slider, Voice settings (same device-local voice settings as the settings page) | `lounge/shell/DevicePopover.tsx` |
| LNG-FRD-013 | Active Now cards: voice channel (members, +N, server), screen share (LIVE), game/watching/reading/listening (platform, details, elapsed), watch party with Join | `lounge/shell/ActiveNow.tsx`, `activeNowModel.ts` |
| LNG-DM-004 | DM header: identity opens profile, voice and video call (join + ring), pins, add friends to DM (1:1 becomes a group), profile-panel toggle, "Search <name>" (existed) | `ChannelHeader.tsx`, `ChannelView.tsx`, `shell/dmCall.ts`, `CreateDmDialog.tsx` |
| LNG-DM-005 | DM welcome header: avatar, `@username`, mutual servers, Remove Friend, Block | `shell/DmWelcome.tsx` |
| LNG-DM-006 | DM right panel: banner, avatar, name, `username • pronouns`, badges, add-friend and ⋯, bio, Member Since, mutual summary, View Full Profile | `shell/DmPanels.tsx` |
| LNG-DM-007 | Group panel: members (owner mark, owner can remove), add friends, group settings (name, icon), owner invite link | `shell/DmPanels.tsx` |
| LNG-DM-009 | Already built (hidden-run row, LNG-MSGUI-032); now covered by a DM test | `p11ShellDm.test.tsx` |
| LNG-PRF-003..011 | Popout and full profile rebuilt on one merged view (`profileView.ts`): badges, custom-status bubble, theme gradient with contrast-aware text, decoration and effect layers, per-server profile override, full modal with left column (bio markdown, Member Since, Friends Since, connections with stats and safe links, note) and tabs (Activity, Mutual Friends, Mutual Servers, Watching & Reading, Reviews), ⋯ menu | `lounge/profile/*` |

Also: the DM row "Call" menu entry is live now; `SR_ACTIVITY_UPDATE` is folded into presence by the reducer (Watching with expiry); presence now keeps every activity (`Presence.activities`, `custom`).

## Not done (not mine or blocked)

- LNG-PRF-001 [DB], PRF-002 [BE], PRF-013 [BE][FE], PRF-014 [GW], PRF-015 [BE]: backend/gateway items, with p11-userset-be. Their client side is in place and optional (see contracts).
- Create Event in the guild menu stays disabled: there is no scheduled-events API.
- Stream card has no thumbnail (needs the stream-preview route).

## Contracts the client codes against (reconcile at merge)

All new fields are optional; the UI renders nothing when absent. Typed in `lounge/profile/useProfile.ts`, `extras.ts`, `data.ts`, `store/types.ts`.

1. `GET /users/{id}/profile?with_mutual_guilds=true&guild_id=<id>` (the `guild_id` param is new; extra params are ignored today):
   - `badges: [{id, description?, icon?}]`. Ids understood: `staff`, `patron[-tier]`, `contributor[-tier]`, `early-member`, `app-developer`; others show with their `description`. Owner and server tag are derived on the client.
   - `user.avatar_decoration_data: {asset, sku_id?}`; image expected at `/cdn/avatar-decorations/{asset}.png` (asset matches `[a-z0-9_-]{1,64}`).
   - `user_profile.theme_colors: [int, int]` (RGB ints), `user_profile.profile_effect: {id}` with ids `sparkle | aurora | snow | embers` (CSS-only effects, frozen under reduced motion).
   - `created_at` (account creation; falls back to the user snowflake), `user.primary_guild.tag`, `user.bot`.
   - `connected_accounts[].stats` (one line) and `.url` (http/https only).
   - `guild_member_profile: {guild_id, nick?, avatar?, banner?, bio?, pronouns?, accent_color?, theme_colors?, profile_effect?}`; image hashes live under `/cdn/avatars/{user_id}/{hash}` as `PATCH /guilds/{id}/members/@me` already stores them.
2. Presence `activities[]` (Discord shape): type 0 playing (`platform`), 2 listening, 3 watching (`name` = app, `details` = title, `state` = episode), 4 custom, **6 = SIDE ROOM "Reading"** (our extension, same fields), `timestamps.start`, `party.id` for a joinable watch party (`/cinema/party/{id}`), `expires_at_ms`. `SR_ACTIVITY_UPDATE` is merged client-side until `PRESENCE_UPDATE.activities` carries it.
3. Group DM channel object: `owner_id`, `icon` (hash under `/cdn/icons/{channel_id}/{hash}`); `PATCH /channels/{id} {name?, icon?}` (data URI) for rename/icon; `POST /channels/{id}/invites` on a group DM returning `{code}` for the owner's invite link (the join page `/invite/{code}` must then accept group DMs). Today the panel shows "not available yet" when the call fails.
4. `GET /cinema/users/{user_id}/reviews` answering `{reviews: [{id, title_id, title, rating?, body?, created_at}]}` for the Reviews tab (empty state while missing). Lists already come from the existing `GET /cinema/users/{user_id}/lists`.
5. `PUT /channels/{id}/recipients/{user_id}` (exists): used to add friends; the 200 answer for a 1:1 DM is the new group.

## Tests run

- New: `p11ShellDirectory`, `p11ShellRail`, `p11ShellNotices`, `p11ShellInstall`, `p11ShellHome`, `p11ShellDevices`, `p11ShellActiveNow`, `p11ShellProfile`, `p11ShellDm` (all in `apps/web/src/lounge/__tests__`), including axe checks on the directory, profile popout, full profile and DM header.
- Updated: `profile.test.tsx` (the full profile now uses tabs).
- Gates: `tsc --noEmit`, `eslint src/lounge src/store`, `prettier --check` (changed files), full web `vitest run --maxWorkers=3` (122 files; the only failures were a missing i18n key, now fixed, and two cold-start timeouts that pass alone: `shellFlows` first test, now preloads the lazy shell, and `admin/cinema.test`), `vite build` and `scripts/check-bundle-budget.mjs` (lounge 328.9 KB gzip of 600 KB).

Cold-start note: the lazy shell takes several seconds to load on a busy machine, so the new files preload it in `beforeAll`.

## Decisions

- Directory is a modal (the brief allowed "page if absent"); a routed `/discover` page would need route-model changes in `routes/lounge.tsx`.
- Privacy Settings in the guild menu opens User Settings > Privacy because the API has no per-server privacy switch.
- Avatar decorations/effects are rendered only from known ids or safe file stems; nothing the server sends is used as a URL or style.
- The PWA update toast in `PwaHost` and the new notice in the Lounge bar can both show; the toast is global and the bar is the in-app copy. Owner may want to drop one.

## Open owner questions

1. Reading activity wire type: is 6 acceptable, or should it be type 3 with a marker?
2. Group DM invite links: confirm the route (`POST /channels/{id}/invites` on a group DM) or name another.
3. Decoration/effect pack catalogue: where do approved free packs live (asset path, ids)?
