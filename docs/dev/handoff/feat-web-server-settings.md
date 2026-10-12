# Handoff: feat/web-server-settings

Worktree: C:/Users/braxt/Desktop/SIDEROOM_wt/ssettings (branch feat/web-server-settings, base integration/phase1). Nothing pushed.

## Goal and scope
Full-screen Server Settings at apps/web/src/settings/server/, route /settings/server/:guildId[/:section]. Only apps/web/** and packages/ui/** changed. docs/open-questions.md untouched; inventory LNG-SSET-* NOT ticked yet.

## DONE (code written, tsc + eslint clean; NOT yet tested beyond ui)
- packages/ui: PermissionMatrix + TriStateToggle (primitives/Permissions), SettingsLayout upgrades (roving tabindex, blockNavWhenDirty, wide, mobile collapse toggle). Tests: packages/ui/src/primitives/permissions.test.tsx (passing, with misc.test.tsx).
- web: store/events.ts (onDispatch seam, called from store.ts applyDispatch), routes/settingsServer.tsx + router.tsx, lounge menus (ChannelSidebar, GuildRail) open settings, GuildPermissions.serverSettings flag (liveMap).
- Sections (all wired to real endpoints, in settings/server/sections/): Overview, DeleteServer, Roles (reorder via buttons+DnD, matrix, hierarchy locks, members), Channels (tree, reorder, overview, overwrites tri-state, sync-with-category), Members, Bans, Invites (+pause via sr/community), Expressions (emoji/stickers), Moderation (verification/content filter + AutoMod CRUD), AuditLog (+auditText.ts), Integrations (webhooks, templates), Community (overview/onboarding/welcome).
- Shell: ServerSettings.tsx (permission-filtered nav, dirty bar via useSettingsForm, ESC).

## REMAINING
- [ ] Web tests with Testing Library + MSW per section (none written yet; plan: __tests__/helpers.tsx seeding store like lounge/__tests__/serverFlows.test.tsx, mock ../../store/connect, MSW handler for GET guilds/:g/members). Cover happy path, hidden sections, validation/server errors, role hierarchy, axe (import axeViolations from packages/ui/src/test-axe).
- [ ] Unit tests: auditText.ts, permissions.ts, validateRule/formToBody, emojiNameFromFile.
- [ ] Run separately: wasm build, typecheck, lint, test, `--filter @sideroom/web build`, `npx prettier --check apps packages`.
- [ ] Tick LNG-SSET items truly done+tested in docs/inventory/06-lounge-server-settings.md.
- [ ] Verify unverified runtime behaviour (never executed in browser/tests): Tabs-per-section dirty bar, DnD, emoji CDN ext guess (/cdn/emoji/{g}/{id}.png|gif), header X-Audit-Log-Reason encoding, GUILD_BAN_*/INVITE_* gateway names.

## Missing backend endpoints / gaps
- Member search / filter by role: GET /guilds/{id}/members has only limit/after (client-side search).
- GET /guilds/{id}/roles/{id}/members (role member list) absent; derived from member list.
- Webhook URL/token only returned on create (POST /channels/{id}/webhooks); no way to re-copy.
- Guild object lacks AFK channel/timeout, icon/banner upload, features; expression slot limits not exposed (client uses sr-chat defaults); emoji/sticker objects lack image URL.
- No prune endpoint, no verification of AUTO_MODERATION_RULE_* gateway events (LNG-SSET-060 open).
- Templates: no public template page route for a copyable link (code only).

## Decisions for owner
- Section gates mirror permissions (see SECTIONS in ServerSettings.tsx); verify against server checks.
- Icon/banner shown as disabled "coming soon".

## Resume
cd C:/Users/braxt/Desktop/SIDEROOM_wt/ssettings; npm_config_manage_package_manager_versions=false npx pnpm@10.12.1 --filter @sideroom/wasm build; then in apps/web: npx tsc --noEmit; npx eslint src; npx vitest run; and pnpm --filter @sideroom/web build. node_modules already installed.
