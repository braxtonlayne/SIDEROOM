# Wave 77 tabbar handoff (SEC-AUD-302, D-800; tail: D-797 light-Library shots)

## Done
- `SEC-AUD-302` (commits on `fix/p77-tabbar`): the floating player's bottom dock keeps clear of a phone's bottom navigation.
  - `apps/web/src/lounge/composerClearance.ts`: `measure()` answers 0 for an element whose rect has height 0 (`display: none`); otherwise as before. New export `useBottomClearance` (the same function as `useComposerClearance`). Header comment brought up to date (D-800).
  - `apps/web/src/lounge/LoungeShell.tsx`: one `useBottomClearance()` call and `ref` on `<nav className="lng-tabbar">`.
  - `apps/web/src/library/Shell.tsx`: the same on `<nav className="lib-bottomnav">`.
  - `apps/web/src/player/player.css`: the comment only (no rule).
  - `apps/web/src/lounge/__tests__/p77BottomClearance.test.ts` (new, 4 tests), `e2e/tests/p77-tabbar-shots.spec.ts` (new; local only, `SR_P77_TABBAR=1`; `SR_P77_TABBAR_MEASURE=1` only measures).
- Tail: the D-797 shots of the Library submission page (second test in the same spec): done, list below. No fix.

## Not done
- Nothing dropped. No existing assertion changed (`p75ComposerClearance.test.ts` passes unchanged: its fake rects have no `height`, so they are read as having a box).

## What I saw on the base (I ran it)
- Vitest on the base (new test file, the code before my change): all 4 tests fail (the height-0 element publishes the viewport height `812`/`800`; the hook export does not exist).
- Spec on the base (the three source files put back to `90e94c45` in my worktree, then restored from my commit; `SR_P77_TABBAR_MEASURE=1`, shots in `visual-sweep-shots/p77-tabbar-base`, not looked at, measurements only), 375x812, one track queued, bottom-end:
  - Friends, all three themes: dock 568-804, tab bar top 755, `--sr-composer-clear` absent: the dock lies on the bar by 49px (intersects).
  - Library home and browse (shipped): dock 568-804, bottom navigation top 759: intersects.
  - A real click on a tab with the player open failed on the base: the player's Mute button "intercepts pointer events" over the Home/Notifications/You tab.
  - Channel (all themes): dock 419-655, composer 663-755, clear 149px: already clear (wave 75's figure).
  - Desktop Friends: dock 556-792, clear absent.

## Measured after (375x812, one track, retro skin, bottom-end; same in all three themes unless said)
| page | dock | what it is above | clear |
|---|---|---|---|
| Friends | 511-747 | tab bar top 755 | 57px |
| Channel | 419-655 | composer top 663 (bar top 755) | 149px (unchanged: the largest wins; wave 75's figure) |
| Library home / browse (bottom nav top 759) | 515-751 | bottom nav | 53px |
| Desktop 1280x800 Friends | 556-792 (8px from the bottom) | no bar | property absent |
| Cinema phone | 568-804 | nothing (see below) | absent |

## Keyboard finding
`:root[style*='--sr-vvh'] .lng-root[data-mobile] .lng-tabbar { display: none }`: the `ResizeObserver` in `trackComposer` DOES fire. In the spec (Lounge Dark) setting `--sr-vvh: 400px` on the root by hand: on Friends `--sr-composer-clear` went 57px to 0px (polled within 3 s), and back to 57px when removed. In a channel it stays 149px with the keyboard "up" (the composer is the largest, as intended). The vitest also covers the observer callback (bar loses its box: 0px; comes back: 56px). No other listener added. Note: the player then lies at 0.5rem on Friends under a keyboard, which is the correct behaviour (no bar drawn).

## Safe area
The bar's height includes `env(safe-area-inset-bottom)`; the measured rect carries it. Nothing to add (Chromium's emulated phone has no inset, so it shows 57/53px; a real device adds its inset to the figure).

## Who imports what
- `composerClearance.ts` imports only `react`.
- `LoungeShell.tsx`: imported only by `routes/LoungeRoot.tsx`, which `routes/lounge.tsx` loads with `lazyNamed(() => import('./LoungeRoot'))`: lazy.
- `library/Shell.tsx`: imported by `LibraryRoot.tsx`, loaded with `lazyNamed(() => import('./LibraryRoot'))` in `library/routes.tsx`: lazy. (The Cinema has its own `Shell` file; not touched.)
- None is first paint or a public page, so no web build and no bundle/entrance/staff-delivery checks were run. The Library chunk now also holds this ~1 KB module (it was already in the Lounge chunk).

## Cinema note
`git grep -n -i "bottomnav\|tabbar" -- apps/web/src` finds a bottom bar only in the Lounge and the Library. On a phone Cinema page (`/cinema`, empty library here) the player (568-804) lies over the page's footer content ("S... A... To... Ab..." links), nothing else; no bar. Changed nothing. (`cinema-phone.png`.)

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p77-tabbar`, 25 PNGs, I opened every one)
- Friends phone x3 (Editorial, Light, Dark): player above the tab bar, all three tabs visible.
- Friends phone bottom-start x3: same, on the other side.
- Channel phone x3: player above the composer, as wave 75 left it.
- Friends desktop x3: nothing changed.
- Library phone x4 (home and browse; shipped theme `sideroom` and `sectionThemes.library = lounge-light`): player above the bottom navigation, four links visible. The player wears the member's Lounge theme (here Dark) in every section (D-538): a dark player over a light Library page, as designed.
- Cinema phone x1.
- Tail: `library-light-{one-running,three}-{desktop,phone}-{shipped,light}.png` x8.
- Clicked through in the spec (it asserts): each of the three tabs on Friends with the player open (Notifications opens a modal: Escape), each of the four Library bottom links (`aria-current` follows), the dock to top-end (top = 8px) and back, bottom-start.
- Caveat on order: the main test's shots were taken in one run; I then fixed the spec so the "shipped" Library variant sets `sectionThemes.library = 'sideroom'` explicitly (the account setting is merged server side, so a previous "light" persisted and the tail's first run showed two light shots). The main test's Library shots were taken before any light value was stored (dark in the shipped shots, I checked), and I did not re-run the main test after that edit; the tail was re-run and its `data-theme` read back (`sideroom` and `lounge-light` on `.lib-root`).

## Tail finding: what only works on a dark page (light section theme; the next wave's item, nothing changed)
Library home / browse / submission page on a phone and desktop with `sectionThemes.library = 'lounge-light'`:
1. The bottom navigation (`.lib-bottomnav`) stays near-black in a light Library, its link labels are dim grey on black (hardly readable), only the active one is lighter. The desktop sidebar (`.lib-sidebar`, incl. "Settings" and the "MANGA, AT YOUR PACE." tagline) is also still dark.
2. The top section bar (`SIDE ROOM / The Lounge / The Arcade / The Study`): the brand and the active "The Study" are pale cream on the light bar (nearly invisible).
3. Library home hero heading "Find your next good story." and the "Follow your curiosity" heading: pale cream on white (unreadable); the genre tiles under it are solid black rectangles.
4. Browse: the kind chips (Manga, Manhwa, ...) and the "Recently updated" select are black blocks with white text on the light page, "Filters" is light: inconsistent.
5. Submission page (the dock): the panels, fields, text, progress bars and buttons read fine in light (grey panels, dark text, blue bars); the native "Choose Files" input is unstyled (light default button, black border) in light and a grey default button in dark. The dock itself (wave 76) has no dark-only colour.
So the dark-only colours are the shell (sidebar, bottom nav, top bar) and the home/browse page hero and chips, not the submission dock. Selectors to start from: `.lib-bottomnav`, `.lib-bottomnav-link`, `.lib-sidebar`, the section bar of `library/Shell.tsx`, the home hero and tile styles in `library.css`.

## Tests run
- `p77BottomClearance.test.ts` 4 pass (all 4 watched failing on the base); `p75ComposerClearance.test.ts` 2 pass.
- Spec with `--project=chromium` on my own stack (ports 8552/5552/9252, binaries from `p72-base-bin`, which started and migrated fine): main test passes, tail test passes (see above for the one re-run).
- Typecheck, eslint, prettier, `check-css-tokens`, `i18n-audit --max 0`, e2e `tsc`: results at the end of this file.

## Docs
Searched `docs deploy README.md` for `composer-clear`, `SEC-AUD-295`, `D-789`, `composerClearance`: only the decisions log, the inventory, wave 75's handoff and `deploy/UPGRADE.md` (a release note, not mine). Nothing to edit.

## Proposed inventory row (the commander adds it after SEC-AUD-300; closes itself)
`- [x] **SEC-AUD-302** [FE] The floating player docked at the bottom keeps clear of a phone's bottom navigation (D-800): the Lounge's tab bar and the Library's bottom navigation report their top edge through the same --sr-composer-clear clearance as a composer (the largest wins; a bar that is not drawn, e.g. under the keyboard, reports 0). Vitest p77BottomClearance.test.ts; spec p77-tabbar-shots.spec.ts (base: dock 49px on the bar).`

## Threat-model rows
None.

## Changelog sentence
Phone: the music player no longer lies on the bottom navigation bar of the Lounge's Friends page or of the Library's pages.

## Owner questions
- The Library's shell (sidebar, bottom navigation, top bar) and its home/browse pages are not usable with a light section theme (see the tail list). Worth a wave, or is a light Library not wanted? (Goes to Fable.)
- The "Choose Files" input on the submission page is the browser's default control in both themes: leave it?

## Cleanup
Stack stopped by PID (API 32268, Vite 35288), scratch database `sr_p77_tabbar_ui` dropped, lock released; `p77-tabbar-bin` is deleted at the end (see below). I found no stash entries I touched (I never used `git stash`).

## Check results (final)
Web typecheck clean (after fixing a cast in my own test), e2e `tsc` clean, eslint clean on the four source/test files, prettier --check clean on all my files, `check-css-tokens` clean (71 files), `i18n-audit --max 0` TOTAL 0, both vitest files pass. No fragment, no string, no generated file. `p77-tabbar-bin` deleted.
