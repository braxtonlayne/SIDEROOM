# Handoff: feat/p23-a11y (RDY-A11Y-001 to 004)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p23-a11y`, branch `feat/p23-a11y`. Nothing pushed. No migrations, no new dependencies, no downloads.

## Resume here

Push the branch and open the PR: the `playwright` job of `e2e.yml` runs every new spec on Chromium, Firefox and WebKit (they live in `e2e/tests/`), and the `live` job now also runs `keyboard-6-voice.spec.ts`. That CI run is the cross-browser proof and the only proof of the parts listed under "Waits for CI".

## Done

- **RDY-A11Y-001** `e2e/tests/a11y-signed-in.spec.ts` (helpers `e2e/support/a11y.ts`, `world.ts`): axe (WCAG 2 A/AA, 2.1, best practice, contrast and landmarks) over 37 signed-in routes on a fixture world built through the API (server with messages and a thread, a published Cinema title, a watch party): Lounge channel, thread, home; 19 Settings sections; server settings; Library home and browse; Cinema home, movies, title, player, party, TV guide, My list; Admin home; Developers and its docs; Support. Zero serious or critical on Chromium and on the Windows WebKit build. The first run found 28 contrast, 8 label-in-name, 1 scrollable-region findings; all fixed in the components (below).
- **RDY-A11Y-002** roving tabindex and skip links. `useRovingTabindex` gained a `grid` mode (row from item positions) and `RovingScope` (`display: contents` wrapper). Applied to: member list, message list (`.lng-msg`; the log region is no longer a second tab stop), Library Browse grid, My Library, Library home rows, Cinema Discover rows, Cinema Browse and More-like-this grids. The TV guide already had its own tested grid navigation (`cinema/tv/GuideGrid.tsx`). `SkipLink` is mounted in the Lounge, Cinema, Library and Admin shells (Developers has had its own); it now links to the region's real id. Unit tests: `a11y/rovingGrid.test.tsx`, `lounge|library|cinema/__tests__/roving.test.tsx`, `admin/__tests__/skipLink.test.tsx`. The keyboard map in `docs/dev/a11y-checklist.md` has no follow-ups left.
- **RDY-A11Y-003** eight specs `e2e/tests/keyboard-<n>-*.spec.ts` (helpers `support/keys.ts`: `tabTo`, a live-region recorder, a text recorder). See "Waits for CI" for what ran where.
- **RDY-A11Y-004** `packages/ui/src/theme/tokens-contrast.test.ts` walks every theme (and so every section): text on surfaces 4.5:1, ink on fills, focus and status 3:1; High contrast resolves every text token to full strength (7:1) and borders to 3:1. `apps/web/src/a11y/reducedMotion.test.ts` walks the stylesheets of the Lounge, Cinema, Library, watch party and the UI kit. `Modes.stories.tsx` renders both modes (axe-checked with the other stories). `e2e/tests/a11y-modes.spec.ts` checks both modes in a browser.

## Real bugs found and fixed on the way

- `Menu`: a menu opened with the keyboard left focus on the trigger (the panel is `visibility: hidden` until positioned and the focus call ran too early). jsdom never saw it.
- `useFocusTrap`: `[data-autofocus], <focusable>` is one selector list, so the dialog's close button beat the autofocus field (the quick switcher opened with focus on "x").
- Library reader: Enter on "Next chapter" was undone by the reader's own URL sync (it wrote the old chapter's address over the new one). Mouse clicks happened to win the race.
- Settings: closing with Escape dropped focus to `<body>`; it now returns to the control that opened Settings (`settings/user/nav.ts`).
- High contrast only reached `--ui-*`; the Lounge (`--text-muted`) and Side Room (`--sr-text-2/3`, `--sr-line`) CSS did not change. Now it does.
- The in-app Reduced motion setting only affected CSS that read its attribute; there is now a global guard for it as well as for the OS setting. Poster-row smooth scrolling honours both.
- Contrast: `--sr-text-3` and `--ui-text-faint` (3.4 to 3.7:1) raised in all themes; light theme idle status dot; Lounge Ash muted text; link-variant buttons (accent text 3:1); `Esc` hint opacity; disabled slider text; offline member names (opacity) ; gold-on-gold anchor buttons in the player.
- Names: guild rail initials and badge, avatar initials, the Cinema logo sub-title are now generated content (`data-text` / `data-initials`) so a link's name need not repeat its visible text; user-panel button text spacing.
- Settings content area is a tab stop only when it scrolls and holds nothing focusable.
- New announcements: upload start, completion and failure in `UploadTray` (`i18n/messages/en/p23-a11y-1.ts`). Landmarks for the Cinema header, Library toolbar, Lounge window bar and Home sidebar; a hidden h1 on Cinema home when there is no hero.

## Moderate and minor findings still open (Chromium run, 37 routes)

| Finding | Where | Owner |
|---|---|---|
| `region`: the column resize handle `.lng-resize` sits outside any landmark | `packages/ui` `ResizeHandle` mounted in `lounge/LoungeShell.tsx` | Lounge shell |
| `region`: watch party room title and description when the party is "not available" (WebKit run) | `watchparty/PartyRoute.tsx` message state | watch party |

Nothing else was reported on the other routes. Colour contrast was only measured on the default Lounge Dark and Side Room themes; the token walk covers the others arithmetically.

## Findings for other owners

- `routes/entrance/Landing.tsx` (first-run agent, not touched): the invite field is disabled while the code is checked, so after a refused code focus falls to `<body>` and stays there. `keyboard-1` records it as a `finding` annotation and walks back to the field.
- `Landing.tsx` calls `scrollIntoView({ behavior: 'smooth' })` without the reduced-motion check (outside the Lounge, Cinema and Library the audit was scoped to).
- Admin graph canvas (`admin/sections/graph/GraphCanvas.tsx`) runs a `requestAnimationFrame` loop with no reduced-motion check.
- Rate limits worth knowing for e2e design: 50 logins per account and hour, a guild-create limit, and one settings request per server per page load. The new specs log the Owner in once per worker, share one server per worker (`solo` fixture) and delete what they create. The existing specs leave about 30 servers behind per run; that, not the new code, made later specs fail on a database reused for three full runs.

## Tests run (all on this machine, scratch database `sr_p23_a11y`)

- web `tsc --noEmit`, `eslint` (0 errors, 0 warnings), prettier on every changed file; `packages/ui` tsc and eslint.
- web vitest, full, `--maxWorkers=2`: 292 files, 2550 tests; the first full run had 2 failures (two tests that read the badge and initials as text), fixed and rerun green. `packages/ui` vitest: 321 tests green.
- `vite build` and `scripts/check-bundle-budget.mjs`: all budgets ok (entrance 230.5 of 300 KB gzip, lounge 426.9 of 600).
- Playwright Chromium, the whole suite, fresh database: 67 passed, 23 skipped (live-only specs), 0 failed. One flake seen once in 16 runs of `keyboard-7-reader` under suite load (not reproduced in 12 repeats alone).
- Playwright WebKit (Windows build), new specs: 40 passed, 8 skipped. The skips are the keyboard specs that cross a link: this build does not Tab to links (the Safari default), so they probe and skip with a reason. On Linux they run.
- Firefox does not launch on this host.

## Waits for CI

- Firefox and Linux WebKit for everything above.
- `keyboard-5-attachment`: ran only up to the failed upload (no object storage here): keyboard path, tray, send and the "Upload of ... failed" announcement are proven; "Uploaded", the message in the list and focus back in the composer need Garage (CI).
- `keyboard-6-voice`: ran only up to the failed join ("The voice call could not connect" announced). Join, mute, unmute, leave need the voice stack (the `live` job, added to its Chromium step). A dropped call is covered by the `voiceAnnounce` unit tests only.
- The screen-reader pass is still a human action; every pair is listed as unheard in `docs/dev/a11y-checklist.md`.

## Decisions

- Generated content (`::before` with `data-*`) for initials and badges rather than loosening axe's label-in-name rule: the rule stays on.
- Specs avoid new registrations (20 per address and day): only `keyboard-2` and `keyboard-3` register accounts.
- Reader flow serves its chapter data with `page.route` (manifest, series, chapter list, one-pixel PNG) so it needs no object storage.

## Open owner questions

None.
