# Wave 95, page builder: SEC-AUD-433 (D-936) and SEC-AUD-431 (D-937), and the wave's web build

Clock: started 14:52; lock round 14:57 to 14:59 (stack, seed, spec once); build round 15:03 to 15:04. Branch `fix/p95-page`, commit `cbe0dddb` (three sheets and the spec) plus this handoff. Everything was measured; no CSS changed after the round.

## What I saw on the base (375 touch, Lounge Light, `/u/owner`, ran it, CSSOM base)
- Grid: 12 columns of 17.6 px; header 343 wide (y 116, h 144); About 165.5 wide at x 16 (h 196); linktree 165.5 at x 193.5 (h 447.6, `scrollWidth` over `clientWidth`: cut); guestbook 165.5 at x 16 (h 742.8). Two columns, as wave 94 said. Page height 1239.
- `hitTestAll` over `main`: 13 rows, 5 fail, 0 not reached: vouch badge 69 x 19 (above: `header.pp-bar`, below: `div.pp-root`); ring 40 x 40 (above and below: its own `span.rng-wrap`); Report (Juniper) 67 x 28, below taken by Remove; Approve 79 x 28, below taken by Report; Report (visitor) 67 x 28, below taken by Remove. Link blocks and author link pass (wave 94).

## What I built
- `packages/profile-blocks/src/profile-blocks.css`, one `@media (max-width: 720px)` block at the end, comment names SEC-AUD-433/D-936 and SEC-AUD-431/D-937: `.pp-root:not([data-compact]) .pp-grid { grid-template-columns: minmax(0, 1fr) !important }`, `.pp-root:not([data-compact]) .pp-grid > .pp-block { grid-column: auto !important; grid-row: auto !important }`, `.pp-entries li > button { min-block-size: 44px }`. `!important` only on the three placement lines. No section, theme, `data-theme` or kit class named; logical properties.
- `apps/web/src/profilepage/vouch/vouch.css`, phone block (SEC-AUD-431/D-937): `.vouch-panel { padding-block: 14px }`, `.vouch-badge { position: relative }`, centred 44 px `::after`. Drawn size stays 69 x 19.
- `apps/web/src/profilepage/profilepage.css`, phone block: `.pp-body .rng-btn { position: relative }`, `.pp-body .rng-btn::after { inset: -2px; border-radius: 50% }`. Under `.pp-body` only: the Lounge's rings are not reached.
- Order: `reading()` sorts by `y`, then `x`; the renderer already draws in that order, so the one column is top to bottom, then start to end (header, About, Support me, Guestbook).
- Guestbook buttons: Report, Approve, Remove are direct `button` children of the `li` (confirmed in the page); the Sign button (`form.pp-sign > button`) is not caught.

## After (same round, same stack)
- 375 Lounge Light: columns 343 px, every block x 16, width 343, one column; heights: header 91, About 74, Support me 394, Guestbook 498.8; none cut, none collapsed. Page height 1238 (base 1239: the header and About lose their row-span height, the linktree shortens to its content). Scroll 375 = 375.
- Hit test: 13 rows, 0 fail, 0 not reached, 0 regressions; guestbook buttons 44 px; link blocks 319 x 44 pass; author link passes. Lounge Dark after: same, 0 fail, 0 not reached.
- Taps (touch): the centre and 20 px under the centre of Report open the report dialog (dialog count 1 both) and entries stay 2 (Remove not pressed). A tap on the vouch badge shows its tip. The ring, tapped 21 px under its centre, opens.
- 700 x 812: one column (668 px), 13 rows, 0 fail, 0 not reached, scroll 700 = 700, height 1052.
- 1280 Editorial: 68 boxes, 0 moved.
- Public page `/~owner` at 375, signed out: one column 343 px (header, About, Support me, Guestbook), link blocks 319 x 44 pass, 0 fail, 0 not reached, scroll 375 = 375. Base there: two columns of 165.5. (The public page draws no QR image; its linktree is 222 tall after, 325 on the base.)
- Case 5 (changes the world): Approve approved the pending note (Approve buttons 1 to 0, entries 2); Remove removed one note (2 to 1), the other stayed.
- Crowding: the after shots show Remove under Report in a narrow entry (8 px gap, not touching) and Approve beside the entry text; the base shows the same wrap (Report over Remove, 8 px). Nothing overlaps; nothing to compare as a fault.
- Not measured: the mini profile in the popout (compact rendering: excluded by `:not([data-compact])`, no inline placement) and the editor canvas (`.pp-canvas`, its own grid, not `.pp-grid`): not reached by the selectors, known by reading, no measurement. Settings' "view as" is a picture (D-921).
- The Sign / Send note button is disabled when empty, so the helper reports it `skipped`; it is not one of the 13.

## Who imports my files
`profile-blocks.css`: only `packages/profile-blocks/src/ProfileRenderer.tsx` (public profile page, signed-in page, editor preview, mini profile, Settings). `profilepage.css`: `ProfilePageView.tsx`, `ProfileEditor.tsx`. `vouch.css`: `VouchPanel.tsx`.

## Web build (my tree, 15:03 to 15:04, lock held by `page`)
`pnpm --filter @sideroom/web build` produced `dist` (its exit line scrolled out of my tail; the log shows no error and the three checks ran on the new bundle). `check-bundle-budget.mjs` exit 0: first paint `app/app-CR-OpwZ2.js` **218 351 B gzip, room 37 649 B** (before, wave 94 merged tree: 218 356 B, room 37 644 B; 5 B less). `check-entrance-isolation.mjs` exit 0 ("every public surface is clean"; profile 115.1/150 KB). `check-staff-delivery.mjs` exit 0.

## Checks after my last edit (15:04)
e2e `npx tsc --noEmit -p tsconfig.json` exit 0; prettier --check on the three sheets and the spec clean; `check-css-tokens` clean; `i18n-audit --max 0` TOTAL 0; `check-doc-links` ok; the spec ran once with `--project=chromium --reporter=line`: 1 passed (14:59). Not run: eslint, web tsc, vitest (CSS and a spec only). I did not watch the spec fail on the base by assertion; the base figures above come from the same run (CSSOM strip).

## Shots (`visual-sweep-shots/p95-page`, 8 PNGs plus `p95-page.json`, all 8 opened)
`1-profile-page-375-lounge-light-base-top`, `...-base-guestbook`, `...-after-top`, `...-after-guestbook`, `1-profile-page-375-lounge-dark-after-top`, `3-profile-page-700-after`, `3-profile-page-1280-after`, `4-public-page-375-after`.

## The commander's command (from `e2e/`, on a stack with the sweep's world; use a fresh world: case 5 removes a note)
`SR_E2E_BASE_URL=http://localhost:5732 SR_P95_PAGE_ON=1 SR_P95_PAGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p95-page npx playwright test tests/p95-page-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (use the stack's own Vite port; `SR_P95_PAGE_BASE=1` for the base figures only). Wave 94's `p94-profile` spec: its 375 positions will differ, not pass or fail.

## Rows
- SEC-AUD-433 (closes): "On a phone (720 px and under) a full profile page shows its blocks in one column, full width, in reading order; desktop, the compact rendering and the editor canvas unchanged (D-936)."
- SEC-AUD-431 (closes): "On a phone the profile page's guestbook Report, Approve and Remove buttons are 44 px tall and no tap reaches another; the vouch badge and the ring button keep their drawn size and have 44 px targets (D-937)."
- Changelog: "On a phone, a profile page now reads in one column, and its guestbook buttons, vouch badge and ring are easier to tap."
- No new rows, no AGENTS.md sentence, no threat-model rows, no owner questions.

## Cleanup
Stack stopped by PID (8732 / 5732 / 9432 free), `sr_p95_page_ui` dropped, `p95-page-bin` and my scratch folder deleted (`p92-base-bin`, `p80-base-bin` untouched), lock released through `heavy.sh` (free), no stash touched. My first build attempt was a `heavy.sh run` that the tool moved to the background while it waited; I stopped it before it took the lock and used foreground `acquire` calls.
