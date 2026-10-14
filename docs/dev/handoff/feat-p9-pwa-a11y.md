# Handoff: feat/p9-pwa-a11y (web only; gate green)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p9-pwa-a11y, branch feat/p9-pwa-a11y (base integration/phase9). Nothing pushed.

## Done (inventory docs/inventory/18-cross-cutting.md, boxes ticked with notes)
- XC-PWA-005: `pwa/useAppBadge.ts` (mounted in the Lounge shell) sums `mention_count` over the store's read states and calls
  `setAppBadge`, honouring the "unread badge" preference.
- XC-PWA-004: `pwa/push.ts` (typed client) + `pwa/WebPushCard.tsx` (Settings, Notifications) + `push`/`notificationclick`
  handlers in `public/sw.js`. **The backend has no push routes.** The client expects `GET /api/v1/push/vapid-key` ->
  `{public_key}`, `POST` and `DELETE /api/v1/users/@me/push-subscriptions` (body in `PushSubscriptionBody`). A 404/405/501 shows
  "This server has not turned on push notifications yet". Per-device state is the browser subscription plus `sr.push.device`.
- XC-PWA-006: manifest `share_target` (GET, title/text/url) -> `/share` (`pwa/SharePage.tsx`, lazy). Pick a channel or DM; the text is
  appended to that channel's composer draft and the channel opens. Unauthenticated -> `/login?next=`. **Files/images are not handled**
  (needs a POST target intercepted in sw.js).
- XC-PWA-007: `lounge/useVisualViewport.ts` sets `--sr-vvh` while the keyboard is open; `.lng-root` height uses it, the mobile tab bar
  hides then and honours the bottom safe-area inset. Not tried on a real iOS device.
- XC-A11Y-001: `a11y/useRovingTabindex.ts` + `<RovingNav>` on the server rail and channel list; the keyboard map is in
  docs/dev/a11y-checklist.md with follow-ups (member list, message cursor, Library/Cinema grids).
- XC-A11Y-002: `MessageList` announces new messages from others in the open channel; `a11y/voiceAnnounce.ts` (mounted in
  `VoiceBridge`) announces join/leave/mute/deafen/reconnect/drop (assertive for loss).
- XC-A11Y-004: `a11y/signedInRoutes.test.tsx`, table-driven jsdom axe (Lounge home, 5 Settings sections, Share).
  It found a real bug, fixed in `packages/ui` Avatar: an empty name rendered `role="img"` with no label; it is now `aria-hidden`.
- Strings: `i18n/messages/en/pwa-a11y-1.ts`. Tests: `pwa/pwaFeatures.test.tsx`, `a11y/a11yFeatures.test.tsx`.

## Gates run
web typecheck, eslint, prettier (changed files), vitest (105 files, 1370 tests, --maxWorkers=3), packages/ui tests, vite build +
`scripts/check-bundle-budget.mjs` (entrance 203.8 KB gzip of 300; all budgets ok). Playwright not run.

## Open / for the owner
- Build the push backend (VAPID key, subscription table, sender) and tell the Share Target about files.
- Badge counts mentions only; a count-less dot for plain unread is possible with `setAppBadge()` and no argument.
- Real-device checks: iOS keyboard, Android share sheet, installed-PWA badge.
- `pnpm install --offline` and `@sideroom/wasm build` were needed in this fresh worktree.
