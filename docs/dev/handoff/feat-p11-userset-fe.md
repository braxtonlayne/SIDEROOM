# Handoff: feat/p11-userset-fe (phase 11, user settings web)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p11-userset-fe, branch feat/p11-userset-fe (base integration/phase11). Local commits only, nothing pushed. Web only (`apps/web`), no backend, OpenAPI or route matrix change. UI strings are in the new fragment `apps/web/src/i18n/messages/en/p11-userset-fe-1.ts`.

## Audit result

Invites and Patron/donations were not "probably built" in the user-settings sense: the Support / Patron page (`apps/web/src/support/SupportPanel.tsx`) already covered Patron status, month buttons, the invite-credit purchase, the invoice dialog, invoice history and leaderboard privacy, but the inventory boxes were stale. Privacy (message requests, DM from server members, friend sources, blocked list) and Appearance were built in phase 5/10 and only have backend-blocked leftovers.

## Done (ticked in docs/inventory/07-lounge-user-settings.md)

| ID | What |
|---|---|
| 017, 018 | Activity switches were complete in the UI; ticked with the note that nothing publishes activity yet. |
| 028, 030, 034 | Already built on the Support page; ticked. |
| 031 | New free-versus-Patron perks table (`PerksTable`, `perkRows`). A Patron value of 0 reads "Same as free". |
| 032 | History rows now carry a "View transaction" link to the configured explorer (`Invoice.explorer_url`). |
| 033 | New "Server contributions" card: paid `server_level` invoices with the server name (reads `GET /donations/invoices`, 50 newest, filtered client side). |
| 046 | Attenuation (0 to 100 percent): `RemoteAudio.setAttenuation`, engine pass-through (optional method, so fakes need no change), controller applies it from `recompute()` while you transmit. |
| 047 | Camera select (saved, used as the default by `controller.startCamera` and the call controls menu), live preview in settings (releases the camera on stop/unmount), "Always preview video" opens a preview dialog before the camera goes live (`lounge/voice/CameraPreview.tsx`). |
| 048 | Screen share defaults (quality preset, share audio, content hint) seed `ScreenSharePicker`; entitlement locking is still done by the dialog. |
| 054 | Interface language picker over the i18n runtime (`setLocale`); English only today, pseudo-languages appear in app test mode or dev builds. |
| 055 | New Streamer Mode section: master switch plus hide personal info (email stays masked, no Reveal), hide invite links (invite dialog masks code and link), disable sounds (voice cues and desktop alerts). Applies from saved prefs; no OBS auto-detection. |
| 056 | App test mode (diagnostics card, pseudo-languages). Developer mode and Reset UI state existed; Copy ID items stay always shown (earlier decision). |

Partly done, left unticked with notes in the inventory: 035 (Cinema watch history list + Clear all + server pause, Cinema defaults: autoplay next, data saver, subtitle language; Library defaults: reading mode, direction, background), 049 (incoming ring switch added), 050 (E2EE info), 051 (inline media size added), 052 (desktop notification + tab title blink consumer `pwa/useDesktopAlerts.ts`, DMs and mentions only, silenced by Streamer Mode).

## Still open, with reasons

- 006 account deletion, 019 data export: backend (LNG-USET-007 and 020) missing.
- 009: avatar decorations, profile effects, two-colour theme, server tag, markdown in About me need server fields.
- 011: Profile Layout is the extension phase.
- 012, 013 (spam filter, per-server override), 015, 023 (providers), 025, 026, 027, 029, 036: backend routes missing. Missing in particular: account invite credit balance and eligibility, create/list/revoke registration invites, frozen state, user-scope feature modules, per-source sensitive-media and spam-filter settings, age confirmation read/reset, connection add/remove.
- 039: no theme package store.
- 040 (show avatars in compact: compact has no avatar column), 042 (TTS rate: no TTS reader; autoplay GIFs: no GIF control).
- 035 leftovers: per-item history removal and reading history clear (no routes), audio language, quality cap and preload count (the Cinema player and reader have no such settings; adding them touches player code owned by the Cinema agents).
- 049 soundboard volume (no soundboard). 050 hardware acceleration hint and QoS (nothing in the engine to apply them to), "show privacy codes" (needs E2EE calls).
- 051 sticker autocomplete and markdown preview toggles (the composer has neither feature).
- 052 per-type sounds (message, mention): stored, no message sound exists. Reaction notifications have no producer. Server sends no Web Push or email digest.

## Decisions I made

- Streamer Mode and app test mode are account prefs in `user_prefs` (sync across devices); attenuation, camera, share defaults, ring switch are device-local in the voice settings store like the rest of Voice & Video; Cinema and Library defaults reuse the player's and reader's own localStorage records, so changing them in settings and in the player are the same thing.
- Desktop alerts only for DMs and mentions (including @everyone), only when the window is unfocused. Per-server notification levels are not consulted yet (a muted server could still alert on an explicit mention).
- `voice/controller.ts` now imports `settings/user/prefs` for `streamerState()` (one-way, no cycle).

## Open owner questions

- Should Copy ID stay always visible, or be gated on Developer Mode like Discord? (Left as is.)
- Do we want the Streamer Mode to auto-enable when OBS is detected? (Browsers cannot detect it; not attempted.)
- Which invite/credit routes should the backend add so the Invites section (025 to 029) can be built? (Suggested: `GET /sr/invites/mine` with balance and eligibility, `POST /sr/invites`, `DELETE /sr/invites/{code}`.)

## Tests run

- `tsc --noEmit`, eslint on changed files, `prettier --check src`: clean.
- New: `settings/user/__tests__/p11userset.test.tsx` (13: streamer, media size, language, test mode, voice additions, attenuation gain, watch history/pause/clear, Cinema and Library defaults), `pwa/desktopAlerts.test.tsx` (4), two new tests in `support/__tests__/support.test.tsx`.
- Full web vitest run with `--maxWorkers=3`: 1492 passed, 5 failed. Re-run alone: the catalog failures were from running before the last fragment keys existed (pass now); the server-menu and download-modal timeouts passed alone (load); `cinema/player/__tests__/signing` "fails (so the session retries) when a playlist cannot be read" fails deterministically with `Cannot read properties of undefined (reading 'indexOf')` and is unrelated to this branch (no change under cinema/player/signing).
- `vite build` and `node scripts/check-bundle-budget.mjs`: all budgets ok (settings 251 KB of 600).
