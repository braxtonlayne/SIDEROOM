# Handoff: feat/p10-sset-fe (Server Settings web gaps)

Scope: `[FE]` items of `docs/inventory/06-lounge-server-settings.md` that were still unticked. Web only
(`apps/web/src/settings/server`); no backend was built.

## Done (ticked in the inventory)

| ID | What |
|---|---|
| LNG-SSET-006 | Ten preset gradient banners (`BannerPresets.tsx`). The API only stores images, so a preset is drawn on a canvas and sent as a PNG data URI, exactly like a custom banner. The live preview card shows it. |
| LNG-SSET-017, 019 | Stale boxes: the level card, pool progress bar, grace notice and the contribute flow already existed in `ServerSupport.tsx`. |
| LNG-SSET-018 | Level comparison table (L0 free tier plus every configured level) with the current level marked. |
| LNG-SSET-020 | "Your contributions" list (this server's `server_level` invoices of the viewer, from `GET /donations/invoices`). |
| LNG-SSET-028 | Members: row and page checkboxes, toolbar with Add role, Remove role, Kick, Ban. There is no bulk endpoint, so members are sent one by one; the result line reports failures and leaves them selected for a retry. Kick and ban skip members the viewer cannot moderate. |
| LNG-SSET-063 | "Enable Community" wizard: safety checks (verification level Low, media filter for all members, set on Finish if unmet), rules and updates channels, finishing touches (default notifications to mentions, strip moderation powers from @everyone, agree to guidelines). Writes guild, `@everyone` role, then community settings. |
| LNG-SSET-065 | Interactive onboarding preview of the unsaved draft (channels and roles each answer hands out); with the wizard this closes the item. |

## Partial (left unticked, note updated)

- LNG-SSET-027: sort (name, joined newest or oldest) over the loaded members was added. Missing: join method / invite code and "signals" (the member object has neither), join-date filter.

## Still open, with reasons (missing backend unless noted)

- 005 icon crop modal: the server crops to a square (design choice from p5); a client crop modal is optional polish.
- 007 traits, 011/012 server tag, 013 system-message toggles (no `system_channel_flags`), 016 widget settings: no fields on the guild.
- 009 preview counts ("N Online, N Members"): no member or presence count on the guild object.
- 025 soundboard: no soundboard endpoints.
- 030 member drawer audit history: the audit-log endpoint cannot filter by target.
- 032 two-colour role gradient: not backed.
- 036 "View server as role": needs the channel visibility computation in the Lounge (large, separate).
- 039 access mode, 041 rules screening, 042 membership applications, 066 server guide, 068 insights, 069 directory request: no endpoints.
- 044, 046, 047, 048 Integrations overview, followed channels, per-server app list and command overrides: `/applications/{id}/installs` and the guild command permission endpoints are per application (developer side); there is no "apps installed in this server" or "followed channels" list for a guild.
- 049, 050, 051 guild-scope feature modules, extensions and themes, watch party and library settings: only the admin-scope `/admin/features` exists.
- 052, 054, 055 DM and spam protection, explicit-media scanner hook, 2FA for moderator actions: no settings stored. 053 is partly present (join-rate alert and "Pause invites" in Community); auto-lockdown has no backend.
- 062 AutoMod alert message renderer: the `AUTO_MODERATION_ACTION_EXECUTION` payload exists but no alert message is posted to the alert channel yet.
- 072 delete modal 2FA code: typed-name confirmation and owner-only exist; `DELETE /guilds/{id}` takes no code, so a client-only 2FA prompt would be meaningless.

## Tests run

- `vitest run src/settings src/i18n` (23 files, 256 tests, all pass), including new tests: support comparison and contributions, bulk kick, bulk role with partial failure, member sort, banner presets, Community wizard, onboarding preview.
- `tsc --noEmit`, `eslint src/settings src/i18n`, `prettier --check apps/web/src`: clean.
- `vite build` plus `scripts/check-bundle-budget.mjs`: all budgets ok (settings 244 KB of 600 KB gzip).

## Decisions

- Banner presets are PNGs generated client-side rather than a new "preset" field, to avoid a backend change. If the owner wants presets stored as a colour pair (so they render at any size), that needs a guild field.
- The wizard sets the safety checks on Finish instead of blocking, and always asks for the guidelines checkbox. "Remove @everyone moderation permissions" removes Administrator, Manage Roles, Channels, Server, Kick, Ban and Manage Webhooks.
- New i18n fragments: `p10-sset-1.ts` to `p10-sset-5.ts`.

## Owner questions

1. Should the Community wizard block when safety checks fail instead of fixing them on Finish (Discord blocks)?
2. Is a stored preset banner (colour pair) wanted instead of generated PNGs?
3. Which backend items above are wanted before the next settings pass (access mode and applications, guild features, DM and spam settings are the largest)?
