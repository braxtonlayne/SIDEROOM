# Handoff: feat/p2-lounge-shell

Worktree: C:/Users/braxt/Desktop/SIDEROOM_wt/p2-lounge-shell (branch feat/p2-lounge-shell, base integration/phase2). Nothing pushed. Frontend only (no Rust or openapi changes). The new modules live under `apps/web/src/lounge/shell/`; edits to shared files are small and additive (see "Shared files touched").

## Done

Rail and servers
- Rail drag-reorder and folders (drop on the middle of a server makes a folder; folder settings modal with name and color; ungroup; Move Up/Down and Add to Folder / Remove from Folder in the context menu as the keyboard route). Pure logic in `shell/railLayout.ts`, drag in `shell/useRailDnd.ts`.
- Server context menu and header dropdown share `shell/useGuildMenu.ts`: Mark As Read, Invite, Mute Server (durations, Unmute), Notification Settings dialog, Hide Muted, Server Settings, Create Channel/Category, Leave Server (confirm, owner told to transfer first), Copy ID.
- Mutes (server, category, channel, DM) come from `GET/PATCH /users/@me/guilds/{id}/settings` (`shell/notifSettings.ts`, `shell/applyMutes.ts`); muted servers and channels dim and lose unread/mention badges.
- Create a Server wizard (Create My Own or 8 templates, "tell us more", name) and Join a Server (examples). Templates are applied by the client after `POST /guilds` (`shell/serverTemplates.ts`).
- Server welcome checklist when a server has no channel; invite dialog gains "Invite friends" (DM of a single-use invite) and now defaults to single use (D-022).

Channels
- Channel and category context menus; hover Create Invite and Edit gear; drag reorder and re-parent with rollback (`shell/channelMoves.ts`, `shell/useChannelDnd.ts`); Edit Channel modal that reuses the Server Settings channel editor (overview + permission overwrite matrix + sync with category + delete); Delete confirm; Duplicate; Create Channel with six types, auto-slug hint and a private role/member picker.

Home
- Friends page (Online/All/Pending/Add Friend, search, row menus, info banner, Active Now from presence activities, loading/empty/error), Create DM / Group DM picker (up to 9), Message Requests page, DM rows (close on hover, avatar collage for groups, context menu with note, invite to server, mute, relationship actions). Pending count and request count badges are live.
- User panel: status picker (online/idle/dnd/invisible with durations), custom status dialog, name still copies the username.
- Connection state banner and an app notices bar (verify email).
- Member list: moderation menu (Timeout, Kick, Ban dialogs), Roles checklist, Change Nickname, Add Note, Invite to Server, Remove Friend/Ignore/Block, Message.
- DM names: READY only carries `recipient_ids` for DMs, so the shell loads `GET /users/@me/channels` on boot (`shell/social.ts`) to put the people in the store. Without that the DM list showed blank names.

Fixes found while testing against the real server
- `computePosition` (packages/ui) now pins a tall float inside the viewport when neither side has room (a 14-item context menu opened mid-screen hung off the bottom).
- Modal header is a `div`, not `header`, so dialogs no longer add a second banner landmark.
- Channel hover actions and the unread pill selectors had to survive the ContextMenu wrapper element.

## Tests
- web: 373 (was 293): `lounge/__tests__/shellLogic.test.ts` (21, pure logic), `shellFlows.test.tsx` (rail/channel/menu/dialog flows with axe), `homeFlows.test.tsx` (friends, DMs, requests, status, banners with axe). `serverFlows.test.tsx` updated for the wizard step.
- ui: 195 (was 194).
- Live Playwright (chromium, real sr-server, throwaway DB): new `e2e/tests/lounge-shell.spec.ts` (template server, channel create/rename/delete/move, rail folder by drag with reload, mute, add friend / accept / message) plus `lounge-server-flows.spec.ts` (updated for the wizard) and `entrance-smoke.spec.ts` all pass. Storage was not available (no Garage locally), so the server ran with a dummy storage endpoint; nothing here uploads.
- Gates: web typecheck, lint, test, `prettier --check apps/web e2e/tests` clean (3 e2e prettier warnings are pre-existing files).

## Not done (and why)
- LNG-RAIL-011 Discover/Server Directory, LNG-RAIL-012 remaining entries (Privacy Settings, Edit Per-server Profile, Create Event), LNG-RAIL-015 PWA install: no backend (GLD-013) or out of scope.
- LNG-SHL-009 notices bar has only the verify-email notice (no update or announcement source). LNG-SHL-012 profile popout host not built.
- LNG-HOME-004, -012, -013 (support item, mic/headphone device menus), LNG-FRD-013 (voice/stream cards: no voice state), LNG-GLD-017 (icon upload/crop), GLD-019 banner/boost, GLD-023 (Profile, Call, Apps, Transfer Ownership), GLD-025/026, GLD-028..034, CHN-010..012, CHN-017 (Invites, Integrations, Features, voice settings, announcement toggle tabs), CHN-018..024 (message-view side, owned by p2-lounge-messages).
- Placeholders that stay "coming soon": Discover, Inbox, Help, Server Support, Create Event, App Directory, Privacy Settings, Edit Per-server Profile, Security Actions, mobile Notifications tab (open question 24 updated).
- Some ticked items carry an italic caveat in the inventory (for example HOME-005 has no group invite link, GLD-021 has three of five cards).

## Owner decisions needed
1. Rail order and folders are stored in `localStorage` per device. LNG-RAIL-007/008 say "persist in user settings", but there is no backend user-settings store for it. Add a `guild_folders`/order setting (and an API) if it should follow the account.
2. Server templates are applied client-side (many sequential POSTs, partial failure shows a toast). LNG-GLD-003 (backend create-from-template) would make this atomic.
3. READY should carry full `recipients` objects for DM channels (backend `crates/sr-server/src/gateway_pg.rs` `build_ready_extras`); the client now works around it with a REST call per boot.
4. Invite dialog default is now single use (D-022 says anything leaving the platform defaults to single use). Say so if the in-app copy-link flow should keep "no limit".
5. Friends use REST polling on load and after each action; there are no `RELATIONSHIP_*` gateway events yet (LNG-FRD-003), so another session's requests show up only after a reload.

## How to resume
- Setup: `npm_config_manage_package_manager_versions=false npx pnpm@10.12.1 install --frozen-lockfile`, then `pnpm --filter @sideroom/wasm build`; run typecheck, lint, test separately in `apps/web`.
- Live e2e: build `sr-server` and `sr-cli` (`CARGO_TARGET_DIR=...p2-lounge-shell-target`, `CARGO_PROFILE_DEV_DEBUG=0`), create a throwaway DB, `sr-cli migrate`, start `sr-server` with `SR_SERVER__PUBLIC_URL=http://localhost:5173` and any storage env, start Vite on 5173 yourself (Playwright's webServer command needs `pnpm` on PATH), then `cd e2e && DATABASE_URL=... SR_CLI_BIN=... npx playwright test --project=chromium tests/lounge-shell.spec.ts`.
- Next candidates: Edit Channel Invites/Integrations tabs, unread jump pills (CHN-012), voice rows once voice state exists, Friends row menu with note/invite (reuse `shell/userMenu.ts`), group DM header and settings.

## Shared files touched (for merging with p2-lounge-messages)
`lounge/store.ts` (additive state and dialog kinds), `lounge/data.ts` (`useRail` applies the layout, `useRequestCounts` reads the social store, `Guild.muted`), `lounge/live.tsx` (mutes applied in `useLiveSnapshot`), `lounge/liveMap.ts` (presence activities), `lounge/LoungeShell.tsx` (banners, boot hook, `PickChannel guildId`), `lounge/LoungeDialogs.tsx`, `lounge/Panels.tsx`, rewritten `ChannelSidebar.tsx`, `GuildRail.tsx`, `HomeSidebar.tsx`, `MemberList.tsx`, `AddServerDialog.tsx`, `CreateChannelDialog.tsx`, small edits to `InviteDialog.tsx`, `UserPanel.tsx`, `settings/server/sections/Channels.tsx` (exports `ChannelEditor`), `test/server.ts` (default handlers), `packages/ui` (Modal header, computePosition). No router, gateway client or generated-client changes.
