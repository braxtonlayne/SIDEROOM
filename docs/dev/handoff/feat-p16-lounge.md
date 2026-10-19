# feat/p16-lounge handoff

## Done
- LNG-SSET-051: Server Settings > Watch Parties & Library (`settings/server/sections/WatchLibrary.tsx`). Who may start a party is the Start Watch Party role permission (explained, link to Roles). The announce channel and filters are the watch party announcer and library feed modules, shown with their switches and SchemaForm (reuses `FeatureCard`, now exported from `Features.tsx`, plus `useFeaturePickers`). No backend change.
- WP-FE-012: `watchparty/WebcamBubbles.tsx`, optional round camera bubbles over the stage, only in a connected call, off by default.
- VOX-UI-016: call controls sheet on phones (`CallControls.tsx`, `voice-p10.css`).
- LNG-CHN-010: stream thumbnail on hover/focus of a streaming voice row in the channel list.
- Audit ticks (already built): LNG-USET-039 (Theme Store card in Appearance), LNG-USET-011 (link to `/profile-page/edit` in Profile settings).
- i18n fragments: `p16-lounge-1..3.ts`. No migrations, routes or OpenAPI changes.

## Still open
- LNG-USET-036: no feature module declares user scope (`Scope::User` is unused), so a user-scope list API would return nothing; build it when the first user-scope module exists.
- LNG-SSET-050 / EXT-PKG-006: adding a pack to a server, a server-wide theme and the server accent color need new backend (guild expression routes take no pack item, members' clients read no server theme). Existing recipe/pack browser unchanged.
- VOX-UI-003: remote per-tile connection quality needs inbound stats mapped to a user (SFU/engine work). Context menu and E2EE shield were already built.
- LNG-CHN-017: per-channel module switches wait for a channel-scope module schema (no module uses channel scope); region is closed (one node).

## Tests run
- vitest (maxWorkers=2) whole web suite: 250 files, 2326 tests pass; new: `p16watchLibrary.test.tsx`, `p16bubbles.test.tsx`, additions in `callUi.test.tsx` and `voiceUi.test.tsx`.
- `tsc --noEmit`, eslint on touched dirs, prettier --check, vite build and `check-bundle-budget.mjs` all pass. No Rust touched.

## Decisions / owner questions
- Watch Parties & Library has no separate "default party channel" setting; the party channel is chosen when starting a party. Owner: wanted as a stored setting?
- Webcam bubbles default to hidden (conservative).
