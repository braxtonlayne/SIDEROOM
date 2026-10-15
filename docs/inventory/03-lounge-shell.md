# 03 — Lounge: Shell, Navigation, Home, Friends, DMs, Profiles

Phase 1. Spec: [lounge-chat](../specs/lounge-chat.md) §1–4.

## LNG-SHL — App shell
- [x] **LNG-SHL-001** [FE] Four-column layout grid (rail 72 / sidebar 240 / main / right panel 240–340) with resizable right panel
- [x] **LNG-SHL-002** [FE] Global section top bar integration (collapsible via setting)
- [x] **LNG-SHL-003** [FE] Title strip: back/forward history buttons, window title text ("Direct Messages", "<server>"), inbox button, help button
- [x] **LNG-SHL-004** [FE] Route model: `/channels/@me`, `/channels/@me/:dmId`, `/channels/:guildId/:channelId[/:messageId]`, `/channels/:guildId/:channelId/threads/:threadId`
- [x] **LNG-SHL-005** [FE] Remember last channel per guild + last location on reload
- [x] **LNG-SHL-006** [FE] Quick Switcher (Ctrl/⌘+K): fuzzy search of DMs, channels, servers, users; prefixes `@ # ! *`; recent list
- [x] **LNG-SHL-007** [FE] Keyboard shortcut system + shortcut sheet modal (Ctrl+/)
- [x] **LNG-SHL-008** [FE] Connection state banner ("Connecting…", "Reconnecting…", offline mode)
- [x] **LNG-SHL-009** [FE] App-level notices bar (verify email, update available, platform announcements) *(`shell/Banners.tsx` NoticesBar: verify email, PWA update waiting, `GET /announcements/active` with per-browser dismissal; test `p11ShellNotices.test.tsx`)*
- [x] **LNG-SHL-010** [FE] Mobile layout: swipeable panes (list ↔ chat ↔ members), bottom tab bar (Home, Notifications, You)
- [x] **LNG-SHL-011** [FE] Drag-and-drop file overlay on the chat column
- [x] **LNG-SHL-012** [FE] Global modals host + context-menu host + popout host (profile popouts)

## LNG-RAIL — Server rail
- [x] **LNG-RAIL-001** [FE] Home button (door mark), unread/mention badge for DMs, selected pill
- [x] **LNG-RAIL-002** [FE] DM unread avatars stack under Home (Discord parity: unread DMs surface in rail)
- [x] **LNG-RAIL-003** [FE] Guild icon (image/acronym fallback, animated on hover, rounded→squircle hover morph)
- [x] **LNG-RAIL-004** [FE] Unread pill (small), selected pill (tall), hover pill
- [x] **LNG-RAIL-005** [FE] Mention count badge (red, 99+)
- [x] **LNG-RAIL-006** [FE] Tooltip with server name (+ voice/stream indicators)
- [x] **LNG-RAIL-007** [FE] Drag to reorder guilds; persist in user settings *(`guild_positions` in `/users/@me/settings`, merge patch debounced 1 s, `SR_USER_SETTINGS_UPDATE` applied, localStorage as first paint and offline cache, D-179)*
- [x] **LNG-RAIL-008** [FE] Folders: create by drag-onto, expand/collapse, name, color, folder unread aggregation, folder settings modal *(folders sync as `guild_folders` in the user settings blob, D-179)*
- [x] **LNG-RAIL-009** [FE] Staff Server special frame/badge
- [x] **LNG-RAIL-010** [FE] "+" Add a Server button → Create Server modal
- [x] **LNG-RAIL-011** [FE] Discover (compass) button → Server Directory *(`lounge/shell/DirectoryDialog.tsx` over `GET /directory`, `POST /directory/{id}/join`; a modal, not a routed page; test `p11ShellDirectory.test.tsx`)*
- [x] **LNG-RAIL-012** [FE] Guild context menu: Mark As Read, Invite People, Mute Server ▸ (durations), Notification Settings ▸, Hide Muted Channels, Server Settings ▸ (shortcuts), Privacy Settings, Edit Per-server Profile, Create Channel/Category/Event, Leave Server, Copy Server ID *(`useGuildMenu.ts`, `GuildProfileDialog.tsx`; Create Event stays disabled: no scheduled-events API; Privacy Settings opens the user Privacy section; test `p11ShellRail.test.tsx`)*
- [x] **LNG-RAIL-013** [FE] Voice-connected indicator on guild icon (speaker glyph) and live indicator
- [x] **LNG-RAIL-014** [FE] "NEW" badge on recently joined servers
- [x] **LNG-RAIL-015** [FE] PWA install prompt replacing "Download apps" button *(`shell/InstallButton.tsx` in the rail: browser install prompt, iOS Add to Home Screen steps; test `p11ShellInstall.test.tsx`)*

## LNG-HOME — Home sidebar & user panel
- [x] **LNG-HOME-001** [FE] "Find or start a conversation" button → Quick Switcher
- [x] **LNG-HOME-002** [FE] Friends nav item (pending-count badge)
- [x] **LNG-HOME-003** [FE] Message Requests nav item (count)
- [x] **LNG-HOME-004** [FE] Optional "Support SIDE ROOM" nav item (operator toggle). **No Nitro / Shop / Quests** *(`HomeSidebar.tsx` shows it when `GET /donations/config` says `enabled`; opens User Settings > Support; test `p11ShellHome.test.tsx`)*
- [x] **LNG-HOME-005** [FE] "Direct Messages" header + "+" Create DM picker (friend search, multi-select ≤ 9 others, "Create DM/Group DM", copy group invite link) *(group invite link not built; needs a backend route)*
- [x] **LNG-HOME-006** [FE] DM row: avatar + status, name, server-tag chip, activity subline (custom status, game, watching/reading, "Glooping…"), close "×" on hover
- [x] **LNG-HOME-007** [FE] Group DM row (icon or avatar collage, member count subline)
- [x] **LNG-HOME-008** [FE] DM row context menu: Profile, Call, Add Note, Close DM, Invite to Server ▸, Remove Friend, Ignore, Block, Mute Conversation ▸, Mark as Read, Copy User ID *(Call stays disabled until voice ships)*
- [x] **LNG-HOME-009** [FE] DM list ordering by last message, virtualized
- [x] **LNG-HOME-010** [FE] User panel: avatar w/ status (click → status picker & account switcher popout), name, custom status line *(no account switcher yet)*
- [x] **LNG-HOME-011** [FE] Status picker popout: Online, Idle, Do Not Disturb (with durations), Invisible; Set custom status (emoji + text + clear-after) *(custom status has text and clear-after, no emoji yet)*
- [x] **LNG-HOME-012** [FE] Mic toggle + dropdown (input devices, input volume, "Voice settings") *(`shell/DevicePopover.tsx` arrow beside the mic in `UserPanel.tsx`; test `p11ShellDevices.test.tsx`)*
- [x] **LNG-HOME-013** [FE] Headphone toggle + dropdown (output devices, output volume) *(same popover for output; device switch needs `setSinkId`, the popover says so otherwise)*
- [x] **LNG-HOME-014** [FE] Settings gear → User Settings
- [x] **LNG-HOME-015** [FE] Copy username on name click (toast "Copied!")

## LNG-FRD — Friends
- [x] **LNG-FRD-001** [DB] relationships table + migrations
- [x] **LNG-FRD-002** [BE] Relationship API (send by username, accept, ignore, cancel, remove, block, unblock, ignore user) → `/users/@me/relationships` (REST by user id; add-by-username lookup follows with the users directory)
- [x] **LNG-FRD-003** [GW] `RELATIONSHIP_ADD/UPDATE/REMOVE` dispatches *(the backend already dispatches them; the web store applies them and DM `CHANNEL_CREATE/DELETE`, with no re-fetch after Friends actions, D-183)*
- [x] **LNG-FRD-004** [BE] Friend request privacy enforcement (everyone / friends of friends / server members)
- [x] **LNG-FRD-005** [FE] Friends header with tabs Online / All / Pending / Add Friend, New Group DM icon
- [x] **LNG-FRD-006** [FE] Dismissible info banner "Looking for accounts you've blocked or ignored? Go to settings"
- [x] **LNG-FRD-007** [FE] Friends search box
- [x] **LNG-FRD-008** [FE] Section heading "Online — N" / "All friends — N"
- [x] **LNG-FRD-009** [FE] Friend row (avatar/status, name, tag chip, activity text, Message button, ⋮ menu: Start Video Call, Start Voice Call, Remove Friend, Ignore, Block) *(video and voice call entries stay disabled until voice ships)*
- [x] **LNG-FRD-010** [FE] Pending tab: incoming (accept ✓ / ignore ✕), outgoing (cancel)
- [x] **LNG-FRD-011** [FE] Add Friend tab: input, submit, success/error states, empty-state art *(icon empty states, no custom art)*
- [x] **LNG-FRD-012** [FE] Empty states per tab (original illustrations) *(icon empty states, no custom illustrations)*
- [x] **LNG-FRD-013** [FE] Active Now column: voice-channel card (members avatars, "+N", server), stream card (thumbnail, LIVE, "Sharing their screen"), game card (platform icon), watching/reading card, party card (Join) *(`shell/ActiveNow.tsx` + `activeNowModel.ts`; voice and stream cards from voice states, watching/reading/game cards from rich activities, party card from `activity.party.id`; `SR_ACTIVITY_UPDATE` folded into presence; no stream thumbnail yet; test `p11ShellActiveNow.test.tsx`)*
- [x] **LNG-FRD-014** [FE] Active Now empty state ("It's quiet for now…")

## LNG-DM — Direct & group messages
- [x] **LNG-DM-001** [BE] Create/open DM (`POST /users/@me/channels`), group DM create/add/remove recipient/rename/icon/leave/transfer owner
- [x] **LNG-DM-002** [BE] DM permission rules (blocked users, "allow DMs from server members" setting, message requests)
- [x] **LNG-DM-003** [BE] Message Requests inbox (accept, ignore, block, spam folder)
- [x] **LNG-DM-004** [FE] DM header: avatar + name (click → profile), call, video, pins, add friends to DM, profile panel toggle, search "Search <username>" *(ChannelHeader `dm` actions, `shell/dmCall.ts`, add-friends via CreateDmDialog `addTo`; test p11ShellDm.test.tsx)*
- [x] **LNG-DM-005** [FE] DM welcome header ("This is the beginning of your direct message history with @x", mutual servers, Remove Friend/Block buttons) *(`shell/DmWelcome.tsx`)*
- [x] **LNG-DM-006** [FE] DM right profile panel (banner, avatar, name, username • pronouns, badges, add-friend & ⋯ buttons, bio, Member Since, mutual friends/servers summary row, View Full Profile) *(`shell/DmPanels.tsx` UserProfilePanel)*
- [x] **LNG-DM-007** [FE] Group DM member list panel + invite link (owner) + group settings (name, icon) *(`shell/DmPanels.tsx` GroupPanel; the invite link calls `POST /channels/{id}/invites` on the group, a contract the backend does not serve yet, so the panel says links are not available; group icon uses `/cdn/icons/{channel}/{hash}`)*
- [x] **LNG-DM-008** [FE] Message Requests page (list, preview, accept/ignore/block, spam tab) *(no message preview and no spam tab: the API has no spam listing)*
- [x] **LNG-DM-009** [FE] Blocked-user message collapse ("1 blocked message — Show") *(already built as the hidden-run row, LNG-MSGUI-032; DM test in p11ShellDm.test.tsx)*

## LNG-PRF — Profiles
- [ ] **LNG-PRF-001** [DB] Profile fields (avatar, banner, accent colors, bio, pronouns, decoration, effect, per-guild overrides)
- [ ] **LNG-PRF-002** [BE] `GET /users/{id}/profile` (mutuals, connections, badges, member since, friends since, guild member profile)
- [x] **LNG-PRF-003** [FE] Profile popout (mini): banner, avatar/status, name, username, pronouns, badges, custom status bubble, roles (guild), note, "Message @x" input, View Full Profile *(profile/ProfilePopout, FullProfile, parts, badgeModel, decoration, theme, profileView; fields beyond today's backend are optional contract fields in useProfile.ts; test p11ShellProfile.test.tsx)*
- [x] **LNG-PRF-004** [FE] Full profile modal: banner, avatar, name, `username • pronouns`, badges row, Message button, add-friend/friends icon, ⋯ menu *(profile/ProfilePopout, FullProfile, parts, badgeModel, decoration, theme, profileView; fields beyond today's backend are optional contract fields in useProfile.ts; test p11ShellProfile.test.tsx)*
- [x] **LNG-PRF-005** [FE] Full profile left column: bio (markdown), Member Since, Friends Since, Connections list (icon, account name, ↗, optional stats line), Note *(profile/ProfilePopout, FullProfile, parts, badgeModel, decoration, theme, profileView; fields beyond today's backend are optional contract fields in useProfile.ts; test p11ShellProfile.test.tsx)*
- [x] **LNG-PRF-006** [FE] Full profile right tabs: Activity (empty-state "So mysterious! ask them to tell you a secret" + Message), N Mutual Friends, N Mutual Servers, Watching & Reading, Reviews *(profile/ProfilePopout, FullProfile, parts, badgeModel, decoration, theme, profileView; fields beyond today's backend are optional contract fields in useProfile.ts; test p11ShellProfile.test.tsx)*
- [x] **LNG-PRF-007** [FE] Profile ⋯ menu: Copy Username, Invite to Server ▸, Add Note, Remove Friend, Ignore, Block, Report (disabled flag), Copy User ID *(profile/ProfilePopout, FullProfile, parts, badgeModel, decoration, theme, profileView; fields beyond today's backend are optional contract fields in useProfile.ts; test p11ShellProfile.test.tsx)*
- [x] **LNG-PRF-008** [FE] Badges system (Staff, Owner, Patron, Contributor tiers, Early Member, App Developer, Server Tag) + tooltips *(profile/ProfilePopout, FullProfile, parts, badgeModel, decoration, theme, profileView; fields beyond today's backend are optional contract fields in useProfile.ts; test p11ShellProfile.test.tsx)*
- [x] **LNG-PRF-009** [FE] Avatar decorations & profile effects (free, from approved packs) *(profile/ProfilePopout, FullProfile, parts, badgeModel, decoration, theme, profileView; fields beyond today's backend are optional contract fields in useProfile.ts; test p11ShellProfile.test.tsx)*
- [x] **LNG-PRF-010** [FE] Profile theme colors (two-color gradient) applied to popout/modal *(profile/ProfilePopout, FullProfile, parts, badgeModel, decoration, theme, profileView; fields beyond today's backend are optional contract fields in useProfile.ts; test p11ShellProfile.test.tsx)*
- [x] **LNG-PRF-011** [FE] Per-server profile rendering in guild context *(profile/ProfilePopout, FullProfile, parts, badgeModel, decoration, theme, profileView; fields beyond today's backend are optional contract fields in useProfile.ts; test p11ShellProfile.test.tsx)*
- [x] **LNG-PRF-012** [BE] User notes CRUD (private)
- [ ] **LNG-PRF-013** [BE][FE] Presence: status, custom status, activities (game, watching, reading, listening), client platform icons (web/mobile)
- [ ] **LNG-PRF-014** [GW] `PRESENCE_UPDATE` fan-out (friends + shared guilds, intent-filtered)
- [ ] **LNG-PRF-015** [BE] Idle detection (client reports AFK after 10 min) + multi-session presence merging
