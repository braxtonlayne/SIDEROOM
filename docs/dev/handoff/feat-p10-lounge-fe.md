# feat/p10-lounge-fe (handoff: lounge-fe)

Web only (apps/web). No backend, OpenAPI or route-matrix changes.

## Done (ticked in docs/inventory/04-lounge-servers-channels.md)
- LNG-CHN-012: unread-mentions jump pill, clickable new-messages banner (MessageList).
- LNG-CHN-019: Edit Channel button in the channel welcome row.
- LNG-CHN-020: pins popout (list, jump, unpin) in `lounge/header/PinsButton.tsx`.
- LNG-THR-004 (Joined tab + search), THR-005 (members, options menu, full view), THR-007 (threads nested in the channel list), THR-008 (private switch), THR-009 (thread settings dialog).
- LNG-FRM-002 and FRM-005 (`lounge/forum/ForumView.tsx`, used for forum and media channels), FRM-006 (forum settings on the Features tab of the channel editor).

## Partial (note left on the inventory line)
- CHN-010: server-mute/deaf icons added to voice sub-rows; voice status text and stream preview on hover open.
- CHN-018: bell menu (`header/NotifyButton.tsx`) and pins are real now; inbox and help are still placeholders.
- CHN-021: read-only notes for rules and announcement channels. Follow button needs a channel-follow API (none exists).
- CHN-024: Rules and Announcements badges. No "updates channel" field on the guild.
- FRM-003: no reaction count on cards. FRM-004: no attachments (server rejects them for forum posts).

## Not done
- LNG-GLD-017, 019, 025, 026, 028..034 and CHN-011, 017: no backend exists for directory, onboarding, screening, per-server profile, security actions, events; CHN-017 gaps were already listed (announcement toggle, voice settings, per-channel feature modules, followed channels). GLD-019 banner/boost indicator not started.

## Tests run
- `vitest run` for all of apps/web: 1392 passed; 1 failure in `src/cinema/player/__tests__/signing.test.ts` (cinema, untouched by this branch, fails on its own too; not investigated). `shell.test.tsx` "renders rail" flaked once under load and passes alone.
- New: `lounge/__tests__/p10LoungeFe.test.tsx` (12 tests), forum settings test in `settings/server/__tests__/channels.test.tsx`.
- `tsc --noEmit`, eslint (lounge, settings/server), prettier, `vite build` + `scripts/check-bundle-budget.mjs` (lounge 308 KB of 600 KB) all pass.

## Decisions
- "Open full view" for a thread expands the thread pane over the channel (CSS `:has`) instead of a new route; threads are not in the channel list so ChannelView cannot render them.
- Bell icon has no muted variant in the icon set; a muted channel dims the button and changes its tooltip.
- Forum post preview/thumbnail fetches the first message with `GET /channels/{post}/messages?after=0&limit=1`; unverified against the real server (falls back to no preview).
- Forum guidelines are the channel topic (Discord parity).
- New strings in `i18n/messages/en/p10-lounge-fe-1.ts`.

## Owner questions
- Should a channel-follow API (announcement Follow) and a guild "updates channel" field be added?
- Is a real route for a thread full view wanted (so threads can be bookmarked as the main view)?
