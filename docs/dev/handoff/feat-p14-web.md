# feat/p14-web handoff

## Done
- H-45 Soundboard favorites in the synced settings blob (`soundboardFavorites`, newest last, cap 50, localStorage list imported once, volume stays per device, `SR_USER_SETTINGS_UPDATE` refreshes the popover). `voice/soundboard.ts`, `lounge/voice/SoundboardPopover.tsx`.
- H-49 Join-date filter ("Joined after", "Joined before") on the members table, client side, inclusive local days.
- H-22 The composer already refused oversize files from `sr_max_upload_bytes` and the store refetches on `SR_ENTITLEMENT_UPDATE` (found stale). Added the Patron hint to the refusal for free members; the tray keeps its `/donations/me` warning only as the fallback when the channel limit is unknown.
- LNG-USET-040 "Show avatars in Compact mode" (`compactAvatars` pref, off by default so nothing changes for anyone).
- G-87 closed without a change, see below.
- New i18n fragment: `apps/web/src/i18n/messages/en/p14-web-1.ts`.

## G-87 measurement
No throttled browser run was possible (shared ports, Playwright banned). Modelled from the production build: `en.rest` catalogue 113 KB gzip, `LoungeRoot` js+css about 148 KB gzip. At 1.6 Mbps the link is the bottleneck, so parallel fetching moves the same bytes and saves only the idle RTT/parse gap (about 100 to 200 ms), under the 200 ms bar. No change made.

## Still open (reasons)
- 05 messages: LNG-MSG-015/016/019, LNG-EXP-004 are [BE]; LNG-CMP-001 (Lexical composer) is a large rewrite; LNG-CMP-007 needs multipart endpoints.
- 07 user settings: USET-011 (Phase 6/EXT), 035 (audio language/quality cap need player support; preload count), 036 (needs user-scope module schema API), 039 (no Theme Store endpoint), 049 (incoming ring), 050 (QoS/E2EE info), 051 (sticker autocomplete switch has no autocomplete to switch; "on hover for mine" spoilers; markdown preview).

## Tests run
vitest: soundboard, soundboardUi, people, p13WebUi2, p13UploadTray, msgGaps, settings/user (all green); tsc, eslint, prettier, i18n-verify-diff (MISSING 0), vite build + bundle budget all pass. Full suite result noted in the final report.

## Decisions / owner questions
- The server settings validator does not declare `soundboardFavorites` on this branch (backend not touched); the blob accepts it. Backend agent should add the key and the 50 cap next to `securityNudgeDone` if the validator is strict.
- Favorites keep the `guild:id` sound keys the popover already used (the answer said "sound id strings").
