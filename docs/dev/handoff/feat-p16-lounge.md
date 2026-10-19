# feat/p16-lounge handoff

## Done
- LNG-SSET-051: Server Settings > Watch Parties & Library (`settings/server/sections/WatchLibrary.tsx`). Who may start a party is the Start Watch Party role permission (explained, link to Roles). The announce channel and filters are the watch party announcer and library feed modules, shown with their switches and SchemaForm (reuses `FeatureCard`, now exported from `Features.tsx`, plus `useFeaturePickers`). No backend change.
- WP-FE-012: `watchparty/WebcamBubbles.tsx`, round camera bubbles over the stage, only in a connected call (default and preference changed by D-265, see Follow-ups).
- Owner answers D-264 (no default party channel; rooms first in the picker) and D-265 (bubbles on by default) are implemented in the Follow-ups below.
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

## Follow-ups (D-264, D-265)
- L-01: the Watch together picker (`watchparty/StartPartyDialog.tsx`) lists Watch party rooms first, then voice, then text and announcement channels, and labels rooms "(Watch party room)". With no caller channel it preselects the destination last used in the active server (`watchparty/lastDest.ts`, localStorage `sr.watchparty.lastDest`, not account state), falling back to a private party if the channel is gone.
- L-02: Watch Parties & Library has a "Where parties live" card (link to the channels view with Manage Channels). Inventory and spec wording updated.
- L-03: `UserPrefs.watchPartyCameras` (default on); `WebcamBubbles` shows bubbles without a click, only the toggle when no camera is live; switch under Settings > Voice & Video. Thumbnail layer unchanged (`VideoSurface` sends no viewport).
- New i18n fragment `p16-lounge-4.ts`. No migrations, routes or OpenAPI changes.
- Room flag on the API: the channel object now carries `sr_watch_party_room` (voice channels only: true for a channel with a standing, not ended, persistent party, else false; absent on other types). It is read in the same query as the channel (`CHANNEL_SELECT` in `sr-db/src/chat_repo.rs` has an `EXISTS` over `watch_parties`, index `watch_parties_channel_idx`), so guild channel lists and GUILD_CREATE cost no extra round trips. Domain field `Channel.watch_party_room`; DTO `ChannelObject.sr_watch_party_room`. The create-room route re-reads the channel after the party starts and emits CHANNEL_UPDATE, so live clients see the flag (the first CHANNEL_CREATE goes out before the party exists). Edge: a room whose party is ended without the channel changing keeps its flag in clients until the next channel update. OpenAPI, TS client and MSW mocks regenerated; no migration. Tests: sr-api `guilds` (field present on voice, absent on text), sr-server PG `a_watch_party_room_is_a_persistent_party_that_outlives_its_creator` (flag via one list read).
