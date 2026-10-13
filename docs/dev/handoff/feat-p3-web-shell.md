# Handoff: feat/p3-web-shell

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p3-web-shell (branch feat/p3-web-shell, base integration/phase2). Nothing pushed. Frontend only (apps/web, plus the small `compute` addition to the `packages/wasm` facade and e2e specs); no Rust, openapi.json or generated-client changes.

## Done

- **F-17 (D-178) channel-level permissions.** `lounge/permEngine.ts` loads `sr-permissions-wasm` `compute` lazily (same pattern as the markdown engine; the shell starts the load on mount). `liveMap.computeChannelPermissions` feeds it roles, the channel's `permission_overwrites` and the viewer's `communication_disabled_until`, and `buildLiveSnapshot` puts the result in `channel.permissions` (view, send, manageMessages, pinMessages, mentionEveryone, manageChannels, createInvite) and derives `canSend` from it. `MessageList` (delete, pin), `ChannelView` (slowmode bypass, @everyone), `ChannelSidebar` hover actions and `useChannelMenu` (edit, invite) read the channel bits and fall back to the guild level until the engine loads or the viewer's member object is missing. Pin is `PIN_MESSAGES` only at the guild level too (it used to accept Manage Messages). `Channel.permission_overwrites`, `Member.communication_disabled_until` are new store fields; `@sideroom/wasm` gained `permissions.compute` and its types.
- **F-18 (D-179) rail order and folders in user settings.** `lounge/shell/userSettingsSync.ts`: `guild_positions` and `guild_folders` (Discord shapes; bare servers are entries with `id: null`, color is an integer), plus `collapsed_categories`, `last_channel_by_guild`, `right_panel_width`. GET on boot; the server wins for keys it holds; keys only this device holds are pushed once (migration from localStorage); changes go out as one merge patch after a 1 s debounce (record keys diff with `null` for removals; untouched defaults are not written); `SR_USER_SETTINGS_UPDATE` is applied through the store's new `userSettings` slice (older versions ignored) without echoing back. localStorage stays first paint and offline cache; a failed PATCH is retried on the next change or the `online` event. New folder ids are numeric like Discord's; old string ids still work.
- **F-19 (D-183) live Friends.** `shell/social.ts`: `applyRelationshipEvent` (ADD upsert, UPDATE nickname/type, REMOVE) and `applyDmEvent` (message-request DMs join or leave the request list on `CHANNEL_CREATE/DELETE`); `startSocialLive()` subscribes through `store/events.ts`, and a fresh READY reloads lists that were already loaded. The reducer keeps `is_message_request` DMs out of the DM list until the server re-sends the accepted channel. The re-fetch after each Friends action, Add Friend and the user menus is gone.
- **F-21 (D-182) invite dialog.** "Copy link" (was "Generate invite link") creates `external: true` and copies; the note says "This link can be used once" (or the real count) plus expiry; pickers start on 7 days / 1 use and, once touched, send explicit limits. "Invite friends" and the member-menu invite DM use the server defaults (`{}`), or the pickers' values when edited.
- **F-22 (D-174)** inventory only: LNG-CMP-001 renamed "Rich composer on Lexical" and moved to the end of the composer list.
- **F-23 (D-164) webhooks.** The URL is shown once after creation and after "Reset URL" (`POST /webhooks/{id}/reset-token`, with a confirm); the disabled "Copy URL" on existing webhooks is removed.
- **F-26 (D-166) + Prune.** New Members, "Prune" button (needs Kick Members) and `sections/PruneDialog.tsx` on `GET/POST /guilds/{id}/prune`: day choice (1, 7, 30), include-roles checklist, live count preview, "Prune N members". Copy says "have not sent a message in N days" while `LAST_SEEN_TRACKED` is false; flip that constant when F-02 ships to get Discord's wording.
- Follow-ups ticked: F-17, 18, 19, 21, 22, 23, 26. Inventory ticked or updated: LNG-RAIL-007/008 (caveats replaced), LNG-FRD-003, LNG-INV-007, LNG-SSET-029 ticked; LNG-SSET-045 annotated (still open: avatar); LNG-CMP-001 reworded.

## Tests
- web: 451 passing in 34 files (425 on the base branch; this branch adds 26): `channelPerms.test.ts` (9, five run against the real wasm engine and skip if `packages/wasm/dist` is not built; `SR_WASM_REQUIRED=1` makes a missing build fail), `userSettingsSync.test.ts` (11), new cases in `homeFlows.test.tsx` (live relationships, DM requests), `serverFlows.test.tsx` (Copy link, with axe), `content.test.tsx` (Reset URL), `people.test.tsx` (Prune, hidden without permission). `shellFlows` invite-friends case updated.
- Gates: web typecheck, eslint, vitest, `prettier --check apps/web e2e/tests packages/wasm/src`, `vite build` all clean. Under CPU load (a cargo build running) a handful of unrelated settings tests time out; they pass on a quiet machine and in isolation.
- Live Playwright (chromium + webkit, real sr-server on a throwaway Postgres DB, dummy storage endpoint): `lounge-shell.spec.ts` (including the new "rail folders follow the account into a fresh browser", which drags a folder, waits for the PATCH and logs in from an empty browser context), `lounge-server-flows.spec.ts` (button renamed to "Copy link") and `entrance-smoke.spec.ts` pass. The friends flow passes against the real gateway events. Server, Vite and the DB were stopped and dropped.

## Not done
- F-20 (Embeds.tsx) belongs to the media-proxy agent; F-24, F-25 are out of scope.
- Channel permissions do not re-evaluate when a timeout expires on its own (they update on the next store change). Thread and forum channels are not given overwrites-based permissions (only text/voice/category rows go through the same path, which is every channel in the store).
- Webhook avatar (LNG-SSET-045) not built.
- The prune count preview uses the backend rule as it is today (joined and silent); wording changes with F-02.
- Docs for F-35 (doc 04 user_settings keys) were not edited; the keys are listed above and in `userSettingsSync.ts`.

## Owner decisions / things to know
1. `guild_folders` ids written by this client are numeric strings converted to numbers (Discord uses integers), names and integer colors follow Discord; a ported client will read them as is.
2. Every channel switch updates `last_channel_by_guild` and therefore sends a (debounced) PATCH to the settings blob. If that is too chatty, raise the debounce for that key or drop it from the sync.
3. After F-19 the Friends page depends on the gateway; with the socket down, actions do not update the lists until reconnect (READY reload covers the gap).
4. F-12 (server sends full `recipients` in READY) is still pending; `loadDms` boot workaround stays.

## Shared files touched (additive)
`store/{types,reducer,rest}.ts`, `test/server.ts` (default handlers for `GET/PATCH /users/@me/settings` and `GET /sr/csrf`), `lounge/{data,liveMap,live,LoungeShell,MessageList,ChannelView,ChannelSidebar,InviteDialog,inviteFlow}`, `lounge/shell/{social,railLayout,useShellBoot,userMenu,InviteFriends,FriendsPage,useChannelMenu}`, `settings/server/{api.ts,sections/Integrations.tsx,sections/Members.tsx}`, `packages/wasm/src/index.{js,d.ts}`. `lounge/msg/Embeds.tsx` untouched.

## How to resume
`npm_config_manage_package_manager_versions=false npx pnpm@10.12.1 install --offline --frozen-lockfile`, `pnpm --filter @sideroom/wasm build`, then in apps/web run `tsc --noEmit`, `eslint .`, `vitest run`. Live e2e as in the p2-lounge-shell handoff (build `sr-server` and `sr-cli`, throwaway DB, start Vite on 5173 yourself, then `cd e2e && playwright test --project=chromium tests/lounge-shell.spec.ts`).
