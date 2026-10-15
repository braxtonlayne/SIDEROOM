# feat/p12-feat-ui: web UIs for the p11 feature modules and the server tag chip

Branch feat/p12-feat-ui (web only, apps/web; no backend, route or OpenAPI changes). i18n fragment `p12-feat-ui-1.ts`.

## Done
- **Server tag chip** (LNG-SSET-012): `ServerTagChip` next to author names in chat, in the member list and in the DM list. `primary_guild` on user references maps to `LoungeUser.tag/tagBadge/tagColor` in `toLoungeUser` (`serverTagOf`). The API only sends it while the member may display it; the client also honours `identity_enabled: false`. Colour is applied to the badge glyph only, so a low-contrast server colour cannot make the letters unreadable.
- **Community dialog** (`lounge/community/`): server menu > Community (shown only when a member-facing module is on, via `GET /guilds/{id}/modules`) and the new `/rank`, `/suggest`, `/ticket` slash commands. Tabs appear only for modules that are on.
  - Leveling (FTR-LVL-001): rank card with progress bar, paged leaderboard, opt-out with a warning (it deletes XP) and opt back in, level roles list.
  - Suggestions (FTR-SUGG-001): submit, status filter, staff status and response, vote counts.
  - Tickets (FTR-TICK-001): open a ticket (jumps to the thread), own list for members, everyone's for staff, close.
  - Birthdays (FTR-BDAY-001): upcoming birthdays tab.
- **Birthday card** on User Settings > Profiles > Servers: month, day, optional year, share or hidden, remove. Server announcement channel/role settings are the module's generic settings form (Server Settings > Features), already present.
- **RSS feed manager** (FTR-RSS-001) inside the RSS card of Server Settings > Features: add (channel, include/exclude keywords, interval), pause, remove, last check status. SSRF refusals from the server are shown.
- **Poll template picker** (FTR-POLLT-001) in the poll modal, plus "save as template" for people who manage the server.
- **Reaction roles message picker** (FTR-RR-001): `SchemaForm` got a `customText` hook; the reaction-role panel's `message` field is now channel then recent message (50) chooser, still storing the id.
- Inventory: all boxes were already ticked; the "backend only" notes are replaced by what exists now (09-feature-modules.md) and the LNG-SSET-012 note (06-lounge-server-settings.md).

## Still open
- **Slowmode presets applied on demand** (FTR-SLOW-001): the backend has only weekly schedules (`slowmode_plus.rs`), no on-demand preset route, so no UI. Left as noted in the inventory line.
- Level roles are visible only to people who can read the module settings (Manage Server); there is no member route for them. A small read-only route would let every member see them.
- Suggestion/ticket staff controls are shown to Manage Server only. The module's `staff_role` members are authorised by the API but the client cannot know the role without the (Manage Server) settings route, so they cannot see the respond/all-tickets controls. A `can_respond` flag on `GET /guilds/{id}/modules` would fix it.
- Suggestion vote counts are inferred from the card message's reactions: first two reactions = seeded up/down emoji, minus the system user's own seed vote. The suggestion DTO has no counts; a `votes` field would be sturdier.
- The ticket "open" route is still not tested end to end on the backend (from p11).

## Tests run
- New `apps/web/src/lounge/__tests__/p12FeatUi.test.tsx` (24 tests, axe on each new surface).
- `vitest --maxWorkers=2` over src/lounge, settings, admin, i18n, router, a11y (all pass); tsc, eslint, prettier --check, `node scripts/i18n-verify-diff.mjs` (MISSING 0), `pnpm build` + `scripts/check-bundle-budget.mjs` (lounge 349.7 KB of 600, settings 254.7 KB of 600).
- Note: `packages/wasm/dist` was copied from the main checkout (no wasm rebuild).

## Decisions / owner questions
- The slash commands `/rank`, `/suggest`, `/ticket` are always offered in servers; the dialog says so when no module is on.
- Birthday card lives on the Servers tab of Profiles (it is server-facing), not the main profile tab.
- English slash-command descriptions stay literal like the existing ones (the command table is not yet in the catalog).
