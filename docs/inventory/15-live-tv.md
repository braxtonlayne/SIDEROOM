# 15 — Live TV (`TV`)

Phase 5. Spec: [cinema-live-tv](../specs/cinema-live-tv.md). Mockup: `cinema-live-tv.jpg`.

## TV-ENG — Schedule engine (`sr-tv`)
- [ ] **TV-ENG-001** [DB] tv_channels, tv_schedule_slots, tv_guide_imports, tv_favorites, tv_reminders _(p8: tv_channels and tv_blocks exist (0460, 0461); p12: tv_reminders (0780); slots, guide imports and favorites do not)_
- [x] **TV-ENG-002** [BE] Channel model (number, call sign, name, era label, logo, kind, timezone, age-restricted, filler policy, rules) _(p8: sr-tv model; kinds rule_based and manual, no imported_guide yet)_
- [ ] **TV-ENG-003** [BE] Resolver for `imported_guide` (weekly slots → concrete blocks, skip missing, overrun/cut policies) _(p8: not built)_
- [x] **TV-ENG-004** [BE] Resolver for `rule_based` (content pool filters, ordering sequential/shuffle/weighted, dayparts, rerun spacing, seeded RNG per channel-week) _(p8: sr-tv engine, seeded RNG kept in a cursor so the schedule rolls on)_
- [x] **TV-ENG-005** [BE] Resolver for `manual` (calendar placements) _(p8: staff placement with trim and split, POST /admin/tv/channels/{id}/blocks)_
- [x] **TV-ENG-006** [BE] Filler policies (bumpers, next_program_early, slate, off_air bars+tone) _(p8: bumpers, next_program_early, slate and off_air blocks; the bars and tone media themselves do not exist)_
- [ ] **TV-ENG-007** [BE] Block cache per channel-day + invalidation on edits/library changes _(p8: blocks are stored and extended lazily 24 h ahead; no invalidation on library changes yet)_
- [x] **TV-ENG-008** [BE] Deterministic tests with clock override ("simulate now") _(p8: ManualClock in the service tests, pure engine tests with DST)_

## TV-HLS — Virtual-linear streaming
- [x] **TV-HLS-001** [BE] Live master playlist (common rendition subset) _(p8: not played on a real player)_
- [x] **TV-HLS-002** [BE] Live media playlist: wall-clock media sequence, sliding window, discontinuities at program boundaries, PROGRAM-DATE-TIME, signed segment URLs _(p8: not played on a real player)_
- [ ] **TV-HLS-003** [BE] Slate/bars filler assets (pre-encoded short loops in each rendition) _(p8: not built, slate and off-air blocks answer 404 off air on the live playlists)_
- [x] **TV-HLS-004** [BE] Viewer counting via playlist requests (metrics only; nothing persistent) _(p8: in-memory presence, 45 s, metrics only)_
- [ ] **TV-HLS-005** [TEST] Two-client sync test (±1 segment), Safari discontinuity test _(p8: playlist windows are unit tested (same window inside a segment, discontinuities); no two-client run, no Safari)_

## TV-UI — Guide page
- [x] **TV-UI-001** [FE] Page header (eyebrow, H1 "A familiar frequency.", + Create channel for MANAGE_TV)
- [x] **TV-UI-002** [FE] Now-playing panel: CRT preview frame (bezel, scanlines, channel number, logo bug, "CHANNEL PREVIEW"), muted hover playback, eyebrow channel, title, meta, synopsis, Program details, Tune in, Edit lineup, "● GOOD RECEPTION" health dot (web: the preview is a muted live stream on hover or focus (a still picture for restricted channels); not verified against a real stream)
- [x] **TV-UI-003** [FE] Filter chips (All, Anime & animation, Movies, Series, Custom, Favorites)
- [x] **TV-UI-004** [FE] Time window navigation (← Evening lineup →, Now)
- [x] **TV-UI-005** [FE] Guide grid: channel column (☆, number, call sign/logo, sub-label), monospace half-hour header, duration-sized program cells (NOW/start time, title, type · year), current highlight, now-line, horizontal virtualization (web: one day per request instead of horizontal virtualization; arrow-key grid navigation)
- [x] **TV-UI-006** [FE] Program popover (synopsis, Tune in / Remind me / Go to title) (web: Remind me calls `PUT /tv/reminders/{block_id}`; synopsis and rating come from the catalog)
- [x] **TV-UI-007** [FE] Favorites (☆) persistence
- [x] **TV-UI-008** [FE] Local time vs channel time toggle
- [x] **TV-UI-009** [FE] Footer note (operator-editable) (web: shows the default text; the server has no operator-editable setting yet)
- [x] **TV-UI-010** [FE] Mobile guide (channel list with now/next, swipe) (web: agenda list per channel; no swipe)

## TV-PLY — Live player
- [x] **TV-PLY-001** [FE] Live mode of the player (no seek bar, LIVE dot, Back to live after pause)
- [x] **TV-PLY-002** [FE] Channel OSD (number + logo + program, fades), channel up/down, mini-guide overlay (G)
- [x] **TV-PLY-003** [FE] Tuning static transition + sound (toggle) (web: noise burst, optional sound, switchable)
- [x] **TV-PLY-004** [FE] CRT filter toggle (scanlines, curvature, vignette; reduced-motion aware)
- [x] **TV-PLY-005** [BE][FE] Reminders ("Remind me" → notification at program start) _(p12: `PUT|DELETE /tv/reminders/{block_id}`, table `tv_reminders` (0780), fired by `userfeat.tick` as a DM + push 5 minutes ahead; a regenerated-away block ends with a "dropped" notice; tests in `sr-tv` service_tests, `sr-server` tv_wiring_pg_tests, `sr-api` tv tests)_

## TV-ADM — Admin
- [x] **TV-ADM-001** [FE] Channel list & editor (fields, logo upload, enable) (web: logo upload is not on the server yet)
- [x] **TV-ADM-002** [FE] Rules editor (pool filters UI, ordering, dayparts)
- [ ] **TV-ADM-003** [FE] Weekly grid editor (manual channels; drag programs)
- [ ] **TV-ADM-004** [BE][FE] Guide import (CSV/JSON template download, upload, fuzzy show matching, mapping report matched/ambiguous/missing, confirm) _(p8: not built (backend half))_
- [x] **TV-ADM-005** [FE] Schedule preview (next 7 days) + simulate-now time travel
