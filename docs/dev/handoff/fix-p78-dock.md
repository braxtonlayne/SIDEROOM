# Wave 78, dock: the end of a phone page scrolls clear of the bottom player (SEC-AUD-305, D-805)

Branch `fix/p78-dock`. Web only: no Rust, no migration, no route, no generated file, no new string.

## Done

Commit `SEC-AUD-305: the end of a phone page scrolls clear of the bottom player` (plus this handoff).

- `apps/web/src/player/pageEnd.ts` (new, imports only `react`): `trackPlayerDock(el)` and the callback-ref hook `usePlayerPageEnd()`. While the dock element's `data-dock` starts with `bottom`, the document root carries `--sr-player-clear` = `innerHeight - rect.top` in px (the dock's height plus its lift, which already includes the composer or bar clearance of D-789/D-800). Kept current by a `ResizeObserver` on the element, a `MutationObserver` on its `data-dock` (so the hook takes the element, not the dock as an argument), a `MutationObserver` on the root's `style` attribute (the lift follows `--sr-composer-clear`, which moves the dock without resizing it; writes happen only on a change so the observer never wakes itself) and `resize`. A top dock, an element without a box, no element, or unmount removes the property.
- `apps/web/src/player/PlayerHost.tsx`: the import, one hook call, the `ref` on `.sr-ply-dock`, and `import './pageEnd.css'`. Nothing else.
- `apps/web/src/player/pageEnd.css` (new): `margin-block-end: var(--sr-player-clear, 0px)` on
  - Cinema: `.cin-frame[data-mobile] .cin-footer` (a margin on the footer, not its padding: the footer's own `margin-top`/padding untouched; with the variable absent it adds `0px`).
  - Library: `.lib-frame[data-mobile] .lib-content` (a margin after the content box, in addition to the 88px bottom padding `library.css` keeps for the bottom navigation; the 88 is not restated).
  - Arcade: `.arc-page`, inside `@media (max-width: 40rem)`. The Arcade has no phone mark (`ArcadeShell.tsx` sets none); 40rem is the query `arcade.css` itself uses for its phone layout.
- No room is zoomed (the overlay root is; the rooms are not), so no division by the zoom.
- `apps/web/src/player/__tests__/p78PageEnd.test.ts` (new, 6 tests): bottom-end with a faked rect (top 568, innerHeight 812) publishes `244px`; bottom-start; top-end publishes nothing; a resize callback with a new rect publishes the new value; moving `data-dock` to the top removes it and back publishes again; no box publishes nothing; the real hook's ref publishes and removes on `null`. On the base the module does not exist (not run on the base: nothing to run).
- `e2e/tests/p78-dock-shots.spec.ts` (new, local only: `SR_P78_DOCK=1`, `SR_P78_DOCK_OUT`; `SR_P78_BASE=1` measures only).

## What I saw on the base (run, same stack, 375x812, one track queued, page scrolled to its end)

The base is the build with the variable neutralised (`* { --sr-player-clear: 0px !important }`, which gives exactly the layout the base has, since the base has no such rule); I did not swap files. The base pass was run twice (the first stopped at the Arcade, see below); its shots are in `visual-sweep-shots/p78-dock-base`, and I did not open them: the measurements above are the evidence.

| room | last block bottom | dock top | verdict on the base |
|---|---|---|---|
| Cinema bottom-end | 784 | 568 | footer's last link UNDER the player; scroll height 812 = viewport, nothing to scroll |
| Cinema bottom-start | 784 | 568 | under |
| Cinema light (`sectionThemes.cinema = lounge-light`) | 784 | 568 | under |
| Library home (long, 1721) | 724 | 515 | under (dock 515-751, nav top 759) |
| Library browse (short, filler) | 724 | 515 | under |
| Arcade home | 748 | 568 | under |

## After (same spec, change in)

| room | last block bottom | dock top | scroll height |
|---|---|---|---|
| Cinema bottom-end | 540 | 568 | 848 (was 812) |
| Cinema bottom-start | 540 | 568 | 848 |
| Cinema top-end | 784 | 8 | 812 (unchanged, property absent) |
| Cinema no track | 784 | none | 812 (unchanged, property absent) |
| Cinema light bottom-end | 540 | 568 | 839 |
| Library home shipped | 427 | 515 | 2018 (dock bottom 751 above nav top 759) |
| Library home light | 427 | 515 | 2004 |
| Library browse shipped (filler) | 427 | 515 | 2374 |
| Arcade home | 504 | 568 | 2909 |
| Cinema desktop 1280x800 | n/a | n/a | 800 with the variable and 800 without it: identical |
| Friends phone bottom-end | dock 511-747 | | same as wave 77 |

The remaining gaps are each room's own bottom padding (Cinema 28, Arcade 64) and, in the Library, 88 (the room it keeps for its bottom navigation, now counted twice: once inside the dock's lift, once as its padding); I left that 88 as the contract says not to restate it.

Click-through (in the spec, passed): a trial press on the footer's last link with the player open is received (on the base the player's box intercepts it); the dock moved to top-end removes the property and back to bottom-end restores `244px`; the last link then really opens its page (`/house-notes`; leaving for a public page ends the music, as `PlayerGate` does); in a fresh `/cinema` with the player open, `clear` (close) removes the dock and the property.

## Checks

- How `PlayerHost` is loaded: `PlayerGate.tsx:6` has `const PlayerHost = lazy(() => import('./PlayerHost'))`, and the gate imports only `presence` and `scope`. `pageEnd.ts` and `pageEnd.css` are imported only by `PlayerHost.tsx` (`git grep -n pageEnd`), so they ship in the player's lazy chunk: **no web build run**.
- Reader (`/library/r/**`), a Cinema title's video player and an Arcade game in play: the reader and the video player do not use `.lib-content`, `.cin-footer` or `.arc-page` as their last block (I did not shoot them; read only the selectors); an Arcade game runs in its own frame. I did not open them with the player docked: if one shows extra room after a player, it is harmless (room after the last block only).
- Existing assertions changed: none. `host.test.tsx` and `musicWiring.test.tsx` pass (23 tests with mine).
- `tsc` (web and e2e), eslint and prettier on my files, `scripts/i18n-audit.mjs --max 0` (0), `scripts/check-css-tokens.mjs` (73 files clean), `scripts/check-doc-links.mjs`: all clean.
- Playwright (chromium, workers 1, one lock round, 12:47-12:54): the spec passes against the change (1 passed). Before that two base passes (the first stopped at the Arcade: my route was `/arcade`, the app's is `/games`, fixed).
- A long page: the dev database has no library or arcade catalogue, so the Library's Browse page is short; the spec appends a 1400px filler ending with `#p78-last-row` to `.lib-content` when a page is shorter than 1.5 viewports (Library home and the Arcade were long enough without it: `filler=false`).

## Shots: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p78-dock` (11, all opened)

`cinema-phone-bottom-end`, `-bottom-start`, `-top-end`, `-no-track`, `-bottom-end-light`; `library-home-phone-shipped`, `library-home-phone-light`, `library-browse-phone-shipped`; `arcade-home-phone`; `cinema-desktop-bottom-end`; `friends-phone-bottom-end`. `data-theme` was read back from `.cin-root`/`.lib-root` before each (`sideroom` or `lounge-light`; logged). The light Cinema and Library pages are half dark on my branch (the library and cinema builders' work); the room clears the player in both. The player wears the member's Lounge Dark in every room (D-538). Arcade home (a dark editorial page here, no light shot taken): nothing odd; the last cards clear the player.

## Not done

Nothing of the item. Not shot: the reader, a Cinema title page, an Arcade game in play.

## Proposed inventory row

`SEC-AUD-305` (closed): on a phone the end of a Cinema, Library or Arcade page scrolls clear of a player docked at the bottom (D-805): `player/pageEnd.ts` publishes `--sr-player-clear`, `player/pageEnd.css` adds that room after the last block; tests `player/__tests__/p78PageEnd.test.ts`, `e2e/tests/p78-dock-shots.spec.ts`.

## Changelog sentence

On a phone the last links of a Cinema, Library or Arcade page can now be scrolled out from under the music player.

## Owner questions

None. (Chosen defaults: the Library's room is added to its existing bottom-navigation padding, so a Library page ends about 88px higher above the player than strictly needed.)

## Cleanup

Scratch database `sr_p78_dock_ui` dropped; server (PID 11610) and Vite (PID 25964, the listener of 5562) stopped by PID; lock released; `p78-dock-bin` deleted. No stash touched.
