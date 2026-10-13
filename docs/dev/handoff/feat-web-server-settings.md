# Handoff: feat/web-server-settings

Worktree: C:/Users/braxt/Desktop/SIDEROOM_wt/ssettings (branch feat/web-server-settings, base integration/phase1). Nothing pushed.

## Goal and scope
Full-screen Server Settings at apps/web/src/settings/server/, route /settings/server/:guildId[/:section]. Only apps/web/** and packages/ui/** changed. docs/open-questions.md untouched; inventory LNG-SSET-* NOT ticked yet.

## DONE (code written, tsc + eslint clean; NOT yet tested beyond ui)
- packages/ui: PermissionMatrix + TriStateToggle (primitives/Permissions), SettingsLayout upgrades (roving tabindex, blockNavWhenDirty, wide, mobile collapse toggle). Tests: packages/ui/src/primitives/permissions.test.tsx (passing, with misc.test.tsx).
- web: store/events.ts (onDispatch seam, called from store.ts applyDispatch), routes/settingsServer.tsx + router.tsx, lounge menus (ChannelSidebar, GuildRail) open settings, GuildPermissions.serverSettings flag (liveMap).
- Sections (all wired to real endpoints, in settings/server/sections/): Overview, DeleteServer, Roles (reorder via buttons+DnD, matrix, hierarchy locks, members), Channels (tree, reorder, overview, overwrites tri-state, sync-with-category), Members, Bans, Invites (+pause via sr/community), Expressions (emoji/stickers), Moderation (verification/content filter + AutoMod CRUD), AuditLog (+auditText.ts), Integrations (webhooks, templates), Community (overview/onboarding/welcome).
- Shell: ServerSettings.tsx (permission-filtered nav, dirty bar via useSettingsForm, ESC).

## STATUS (completed)
- Tests: 58 server-settings web tests (__tests__/: shell 6, overview 3, roles 11, channels 7, people 7, content 12, moderation 12) + 8 ui tests. Full web suite 231 passed; ui suite passes.
- Verified: reorder logic (rotates position values; tests), audit reason header is percent-decoded server-side (settings/deps.rs audit_reason), gateway names exist in sr-gateway protocol.rs (GUILD_BAN_*, INVITE_*, GUILD_EMOJIS/STICKERS_UPDATE, AUTO_MODERATION_RULE_*). Emoji CDN URL guess was WRONG (media key uses a separate new_id, not the emoji id): images removed, placeholder shown.
- Inventory ticked: SSET-001,002,004,008,014,021,022,031,033,035,037,038,057,058,061.

## Missing backend endpoints / gaps
- Member search / filter by role: GET /guilds/{id}/members has only limit/after (client-side search).
- GET /guilds/{id}/roles/{id}/members (role member list) absent; derived from member list.
- Webhook URL/token only returned on create (POST /channels/{id}/webhooks); no way to re-copy.
- Emoji/sticker image URL: media key is emoji/{guild}/{random new_id}.{ext}, not derivable from the emoji id; objects should expose a url/key.
- Guild object lacks AFK channel/timeout, icon/banner upload, features; expression slot limits not exposed (client uses sr-chat defaults); emoji/sticker objects lack image URL.
- No prune endpoint, no verification of AUTO_MODERATION_RULE_* gateway events (LNG-SSET-060 open).
- Templates: no public template page route for a copyable link (code only).

## Decisions for owner
- Section gates mirror permissions (see SECTIONS in ServerSettings.tsx); verify against server checks.
- Icon/banner shown as disabled "coming soon".

## Resume
cd C:/Users/braxt/Desktop/SIDEROOM_wt/ssettings; npm_config_manage_package_manager_versions=false npx pnpm@10.12.1 --filter @sideroom/wasm build; then in apps/web: npx tsc --noEmit; npx eslint src; npx vitest run; and pnpm --filter @sideroom/web build. node_modules already installed.
