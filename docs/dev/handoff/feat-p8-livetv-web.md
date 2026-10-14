# Handoff: feat/p8-livetv-web (Live TV web: guide, live player, Admin > Cinema > Live TV)

Worktree `SIDEROOM_wt/p8-livetv-web`, branch `feat/p8-livetv-web` (base `integration/phase7`). Frontend only (`apps/web`), plus the inventory ticks and two budget entries. No backend, migration or OpenAPI change. Local commits only; nothing pushed.

Coded against `docs/specs/cinema-live-tv.md` section 8 ("HTTP API as built") as it stands on `feat/p8-livetv-be` (commit `697e3cc`, re-read before finishing: unchanged). Nothing here has played a real stream (the backend has not either); everything is covered by MSW, a fake player engine and a mocked `HTMLMediaElement`.

## What exists

| Area | Where |
|---|---|
| Guide `/cinema/tv` | `cinema/tv/Guide.tsx` (page, state, theme), `GuideGrid.tsx`, `NowPanel.tsx`, `LivePreview.tsx`, `ProgramPopover.tsx`, `MobileAgenda.tsx`, `tv.css` |
| Live player `/cinema/tv/watch/$channel` | `LiveWatch.tsx` (tune, age gate, session start), `LiveView.tsx` (chrome), `LiveOverlays.tsx`, `livePrefs.ts`, `live.css` |
| Data layer | `cinema/tv/api.ts` (the only place that knows URLs and wire shapes), `types.ts`, `model.ts` (pure helpers), `favourites.ts` |
| Routes | `cinema/tv/routes.ts` (`createTvRoutes`, appended to `cinema/routes.tsx`), both lazy |
| Admin | `admin/cinema/tv/`: `LiveTvSection.tsx`, `ChannelEditor.tsx`, `RulesEditor.tsx`, `ChannelPanels.tsx`, `tvApi.ts`; registered as section `cinema-live-tv` (group CINEMA, needs MANAGE_TV) in `admin/AdminApp.tsx` |

### Guide (TV-UI)
- Header, H1 "A familiar frequency.", `+ Create channel` and `Edit lineup` only for MANAGE_TV (read from `GET /admin/me`, which answers 403 for non-staff). The mockup's "SAMPLE LINEUP" and "Library preview · No live stream connected" labels are gone; the footer is "Channels are assembled from the SIDE ROOM library." plus a current-time legend.
- Now panel: CRT frame (bezel, scanlines from `--sr-scanline`, channel number, bug, "CHANNEL PREVIEW"), eyebrow, title, meta, synopsis, `Program details`, `Tune in`, `Edit lineup`, "GOOD RECEPTION" / "NO SIGNAL" dot. Hover or focus loads the live stream muted (lazy `LivePreview`, so hls.js stays out of the page chunk); never for age-restricted channels.
- Filter chips (All, Anime & animation, Movies, Series, Custom, Favorites), day navigation (Yesterday to +7 days), `← Evening lineup →` moves four columns, `Now` jumps (also the `n` key anywhere on the page), channel/local time toggle, now line, sticky channel column, duration-sized cells with `NOW · 8:00 PM`, title, episode, `Anime · 1998 · 30m`, and an 18+ badge.
- Keyboard: roving tabindex over the cells; arrows move across and between channels (the block covering the same start time), Home/End, Enter opens, `n` jumps. The programme popover is a floating dialog anchored to the cell (a modal on a phone): synopsis, rating, next airings (same title on any channel), Tune in (if on now), Remind me (if later), Go to title.
- Favourites: the settings blob key `cinema.tv.favourites`, written as the merge patch `{cinema: {tv: {favourites: [...]}}}` (the array is sent whole), optimistic with rollback.
- Mobile (max-width 768): an agenda list per channel (now first, then what is coming), tap opens the popover as a dialog.
- Broadcast theme on this page only: a nested `ThemeProvider section="live-tv"` resolves `sideroom-broadcast`; `themes.ts` and calibrated tokens untouched. The viewer's own per-section theme choice is honoured; the global theme choice is not (the teletext layer needs its tokens).
- Honest states: spinner, "Live TV is not available yet" (404/405/501 without an API body), retryable error, "No channels yet", "No favorites yet".

### Live player (TV-PLY)
- Reuses `playerSession()` (one persistent `<video>`, engine interface, mini player, URL refresh hook). A live channel is a `PlaybackInfo` with `live: {channel_id}`, media id `tv:<channel>`, duration 0. Small additive edits to the player: `types.ts` (`live?`), `session.ts` (no progress saving for live), `api.ts` (`refreshSource('tv:..')` tunes again), `model.ts` (`LIVE_MEDIA_PREFIX`), `MiniPlayer.tsx` (Expand goes back to the live route). `CinemaRoot` treats `/cinema/tv/watch/` as a full-screen route.
- Live semantics: `LIVE` badge, `Back to live` once more than 12 s behind the edge (pause is allowed), a rewind slider only when `video.seekable` spans 90 s or more (the sliding window is about 36 s, so normally none), never past the live edge. Channel up/down (buttons, ArrowUp/ArrowDown, PageUp/PageDown, wrapping), channel list overlay on `G` (listbox: arrows, Enter tunes, Escape closes), now/next banner with a progress bar from the server clock, channel OSD that fades after 3 s, volume, mute, full screen.
- 403 code 900011 goes through `useAgeGate().ensureAdult()` (yes: retry; no: back to the guide). Fatal media errors, stalls (10 s waiting) and a stream that ends trigger up to three re-tunes ("Signal lost. Reconnecting…"), then "The signal dropped" with Try again. A slate or off-air block (no media; the playlists answer 404) draws a client-side colour-bar slate and does not count as a stall; the player re-tunes when the slate ends. `tune` answering `off_air` shows "This channel is off the air".
- Tuning static (noise burst, optional sound, off by default) and CRT filter (scanlines, vignette, rounding; reduced-motion aware), both in a settings panel, stored per device in `localStorage` (`sr.cinema.tv.v1`).
- Mini player: leaving the route while playing floats the channel; Expand returns to `/cinema/tv/watch/<id>` without tuning again.
- Signed URLs: the master is a session-authenticated API path (no signature); media playlists carry freshly signed segment URLs on every reload. So the master "expires" 24 h out, and an `auth` error falls back to a re-tune.

### Admin (TV-ADM)
Channels list and editor (number, name, call sign, era label, time zone from `GET /tv/timezones`, kind, filler policy, age-restricted, enabled, delete with typed name); rules editor (grid, no-repeat window, dayparts with name, times, days, mode, fit and a pool: kinds, genres, tags, title ids, collection, years, runtimes, restricted mode; validation mirrors the spec); Schedule tab (preview from the rules or the stored blocks, warnings, grouped by day, simulate now via `GET .../now?at_ms=`, publish with a horizon and a confirm); Now control (what is on air, put a title on air with episode and offset, remove an override or manual block, place a programme at a time); What aired (summary plus table from `/admin/tv/report`). 403 shows a permission state; a missing route shows "Not available yet".

## Wire mapping worth knowing (spec section 8)
- Viewer routes are `/tv/...` (no `/sr` prefix): `GET /tv/guide?from&to` (a day per request), `GET /tv/channels/{id}/now`, `GET /tv/channels/{id}/tune` plus `GET /tv/channels/{id}` (the grant needs the channel), `GET /tv/timezones`. `master_url` already carries `/api/v1`.
- A server block becomes a UI block: `program` keeps `media_kind`, `filler/slate/off_air` become filler with `playable: false`; the episode label is built from season, episode and episode title.
- The block carries no synopsis, rating or artwork: the popover and the now panel read them from the Cinema catalog (`getTitle`) by `title_id`, only when needed. So **cells do not show a rating badge** (only the 18+ badge); the rating shows in the popover.
- The channel has no network or category: the sub-label is the era label (or `CUSTOM` for manual channels), the filter category is `custom` for manual channels and otherwise derived from the dominant kind of what airs that day. Call sign falls back to the upper-cased name.
- `healthy` ("GOOD RECEPTION") is derived: a playable programme is on now.

## Decisions
- Favourites use the settings blob (not a server table), as the task said; the server has none.
- No virtualization: one day per request keeps the DOM at a few hundred cells. Revisit if channel counts reach the hundreds.
- The Cinema nav link moved from the placeholder `/cinema/live-tv` to `/cinema/tv` (the route the task names).
- Admin default for a new rule-based channel: one daypart "All day" from 00:00 to 00:00 (the spec says an end at or before the start wraps, so this should mean around the clock; **unverified against the server**).
- New daypart-less rule-based channels are refused client-side (nothing could ever air).
- Added bundle budgets `cinema-tv` (route chunk `Guide`, 221.8 KB gzip first load) and `cinema-tv-live` (`LiveWatch`, 225.3 KB), entrance stays at 202.2 KB.

## Not done / follow-ups
- **TV-PLY-005 reminders**: the Remind me button calls an assumed `PUT|DELETE /tv/reminders/{blockId}` and says "not available yet" on 404. Needs a backend route and a notification.
- **TV-ADM-003 weekly grid editor** (drag programmes) and **TV-ADM-004 guide import**: not built; the screens say so. Programmes can be placed at a time from Now control.
- Logo upload (no route), operator-editable footer note (no setting), per-programme rating in cells (not in the block), `imported_guide` channels, "Watch together" on a live channel (watch parties), swipe on the phone agenda.
- Real streams: nothing was played. In particular unverified: hls.js live behaviour with `EXT-X-DISCONTINUITY`/`MAP` changes, Safari native HLS, whether `video.seekable` gives a usable live window, the muted hover preview, and cookie auth for the playlists in the hls.js loader (same origin, so cookies should go by default).
- Visual check: the guide was compared with `cinema-live-tv.jpg` at 1704 px and 375 px in the browser pane against a throwaway mock API (not committed; dev server on port 5189, stopped). The live player and Admin screens were only checked by tests (the pane's screenshots time out on the full-height page, and the mock has no stream).
- The Admin screens have no Playwright spec (needs a seeded backend); e2e was not run (shared ports).

## Tests and gates
- New: `cinema/tv/__tests__/guide.test.tsx` (19), `live.test.tsx` (19), `model.test.ts` (18), `admin/__tests__/livetv.test.tsx` (20) = 76 tests, with axe on the guide (desktop and phone), the live player, and the Admin channel list, rules form, schedule and report. Updated: the Admin nav expectations in `admin.test.tsx` and `cinema.test.tsx`.
- Whole web suite: 89 files, 1143 tests pass (was 85 files, 1068). `tsc --noEmit`, eslint, prettier `--check`, `vite build` and `node scripts/check-bundle-budget.mjs` are clean.
