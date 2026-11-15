# Wave 75 player handoff (SEC-AUD-295, D-789)

## Done
- `0e4a0e1f` SEC-AUD-295 (plus a follow-up commit for the spec's DM setup and this handoff):
  - `apps/web/src/lounge/composerClearance.ts` (new): `trackComposer(el)` (measures `innerHeight - rect.top` on mount, on `ResizeObserver` of the element and on window `resize`; a module-level map, largest wins; removes `--sr-composer-clear` from `<html>` when the last one stops) and the callback-ref hook `useComposerClearance()`. A callback ref because the locked and the live composer are different elements.
  - `Composer.tsx`: one import, one hook call (before the early return), `ref={clearRef}` on both `lng-composer-wrap` divs (locked and live). Nothing else.
  - `player.css`: both bottom docks get `--sr-ply-lift: calc(0.5rem + var(--sr-composer-clear, 0px) / var(--sr-overlay-zoom, 1))` and `inset-block-end: var(--sr-ply-lift)`.
  - `lounge/__tests__/p75ComposerClearance.test.ts` (2 tests, pass).
  - `e2e/tests/p75-player-shots.spec.ts` (local only, `SR_P75_PLAYER=1`).
- **`--sr-overlay-zoom` finding:** the dock's parent `ThemedOverlayRoot` is `display: contents` and carries `--sr-overlay-zoom`; `overlay-root.css` applies `zoom: var(--sr-overlay-zoom)` to each child, so the dock's lengths are in zoomed units while `--sr-composer-clear` is in viewport px. Hence the division, the same as the `100vw / zoom` widths already there. Shots were taken at zoom 100 only; the zoom path is by reading, not looked at.
- **Cap:** bottom docks also get `max-block-size: calc(100vh / zoom - lift - 0.5rem)` and `overflow-y: auto`: a tall skin on a short screen is cut to fit and scrolls inside its own box. Seen on the phone: valve, tidepool, circuit, study are 647px tall (top at 8px, bottom 8px above the composer), their lower controls scroll.

## Measured (spec output, Chromium)
- Confirm on the base: the spec sets `--sr-composer-clear` to 0px (the same as the base CSS, which has no clearance) and asserts the boxes intersect. They did, in all 6 theme x size combinations. Desktop 1280x800: dock top 556, bottom 792 (x 936-1272) vs composer top 730, bottom 800: overlap. Phone 375x812: dock 568-804 vs composer 663-755: overlap (the dock covers the whole composer). I did NOT build or run the base tree; this is the same CSS state by property, not a separate base run.
- After: desktop clear 70px, dock 486-722 (composer top 730, 8px gap); phone clear 149px, dock 419-655 (composer top 663).
- No-composer page: the property is removed (asserted), dock bottom 8px from the viewport as before. Top dock: top is 8px as before (asserted).
- All 8 skins at 375x812 and 1280x800 (Lounge Dark): dock clear of the composer and inside the viewport (asserted).

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p75-player`, 50 PNGs, I opened all 50)
Channel x6, channel bottom-start x6, channel top dock x6, DM x6, thread pane x6, emoji picker open x6, page without a composer x6 (each: desktop and phone, Editorial, Lounge Light, Lounge Dark); plus all 8 skins at the phone size in Lounge Dark (the tallest are valve, tidepool, circuit, study; pill is the smallest at 87px). Clicked through in the spec: play, type a message, send with the Send button (the message appears), open the emoji picker. In every shot the whole composer is visible and usable.

## What the player now sits on or beside (listed, nothing fixed)
- Phone: the bottom tab bar (Home / Notifications / You) is below the composer, so the player sits above composer and bar. On a page with no composer (Friends) the player at 8px lies over the tab bar and the rail's last icons, as before.
- Bottom-start on desktop: lies over the server rail's lower icons and the channel list, and no longer over the user panel (before it covered the user panel). The composer is clear.
- Phone with Retro: the player is about 236px tall, so the message list loses about a third of the screen (the compact phone skin stays the plan's separate design item).
- The emoji picker (z 1050) opens over the player on both sizes; on the phone it hides the player and overlaps the composer's text field (existing popover placement; the emoji button stays reachable).
- Thread pane on desktop: the dock clears the thread's composer (a taller one) and covers part of the thread's messages.
- Not seen: typing line, jump-to-present bar, voice bar (no such state in the shots). A typing line or reply bar is inside `lng-composer-wrap`, so it is counted in the clearance.
- Other `lng-composer-wrap` users: only `Composer.tsx` (both wrappers hooked). No other composer component found.

## Checks run
- vitest `p75ComposerClearance.test.ts`: 2 pass. Without the module it cannot run (the import fails); I did not separately watch that.
- web `tsc --noEmit`, eslint (the three TS files), prettier: clean. e2e `tsc`: clean. `i18n-audit --max 0`: 0. `check-css-tokens`: clean (covers `player.css`). `check-doc-links`: ok.
- Not run: `player-layout.spec.ts` and the other existing player e2e (CI does).

## First paint (one web build, lock held)
- `app-*.js` first paint: 215,903 bytes gzip (limit 250 KB, room 40,097). Wave 73's figure was 215,895: +8 bytes, not from my code (the string `sr-composer-clear` is only in `LoungeRoot-*.js`, a lazy chunk; no file of mine is imported by first paint; most likely a changed chunk hash in the preload map). I did not build the base to confirm.
- `player.css` is in `PlayerHost-*.css` (lazy), not in the first-paint stylesheet `app-*.css`; it grew by 7 lines of source.
- `check-entrance-isolation`: clean. `check-bundle-budget`: ok. `check-staff-delivery`: ok.

## Docs
No doc outside the decisions log and the inventory states where the dock sits (searched MUS-PLY-001, "floating player", "corner", "bottom-end"; hits are handoffs, the roadmap and the inventory row). Nothing edited. For the commander: `docs/dev/handoff/feat-p30-player-shell.md` line 81 ("The dock is one of four corners ...") could add "a bottom dock sits above a message composer when one is on screen (D-789)"; it is a historical handoff, so I left it.

## Proposed inventory row
`- [x] **SEC-AUD-295** [FE] The floating player docked at the bottom no longer covers the message composer (D-789): the Lounge composer reports its top edge as --sr-composer-clear on the document root (lounge/composerClearance.ts, largest wins, removed when none is mounted) and the bottom docks sit that far above their usual 0.5rem, capped to the viewport; top docks unchanged. Spec e2e/tests/p75-player-shots.spec.ts, vitest p75ComposerClearance.test.ts.`
Threat-model rows: none.

## Changelog sentence
The music player now sits above the message box instead of covering it, on desktop and on a phone.

## Cleanup
Stack stopped by the PID that owned each port, `sr_p75_player_ui` dropped, `p75-player-bin` deleted, lock released, scratch logs removed. No stash touched.

## Owner questions
1. A tall skin (Valve, Tidepool, Circuit, Study) on a phone now fills the screen down to the composer and scrolls inside itself. Keep that, or force a compact player on a phone (the plan's separate design item)?
2. Docked at bottom-start on desktop, the player now covers the server rail's lower icons. Fine, or should the player avoid the rail (a later item)?
