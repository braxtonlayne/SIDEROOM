# Wave 103 handoff: event (SEC-AUD-459, SEC-AUD-466; D-978)

Clock: started 11:56, lock waited 12:01 to 12:23 (slot 2), round 12:23 to 12:32, finished 12:33.

## Done (commits on `fix/p103-event`)
- `SEC-AUD-459 events.css: auto-fit grid for the two date fields`, plus the spec (`e2e/tests/p103-event-shots.spec.ts`, two commits).
- `SEC-AUD-466 delete the unused .lng-select rules` (`events.css` and `shell.css`, only those two rules).

## Cause (SEC-AUD-459), measured on the base
The dialog is a fixed box (402 px at 1024 and 1280; body 400 wide, grid 360). `.lng-event-times` was `1fr 1fr`; a `datetime-local` input cannot shrink under its own text: the Starts field is 251 px and Ends 254 px wide, 517 with the gap against 360. The body (`overflow-y: auto`, so also auto sideways) had `scrollWidth` 537 against `clientWidth` 400 at 1024 Light, 1024 Dark and 1280 Light, plain and with the large case; the Ends field ran 157 px past the grid (right edge 849 against 692) and the shot shows its date cut. Radios started at the body's left edge (349 against 312): the "left of the radios cut" in wave 102's note was the scrolled body, not a cut. At 375 the existing one-column rule made it whole.

## Diff in words
`.lng-event-times` is now `grid-template-columns: repeat(auto-fit, minmax(min(100%, 16rem), 1fr))`; the 480 px media rule is gone (a single track is the whole line, as before). Both fields share a line only where each gets 16rem (256 px, above the 254 px field), else Ends goes under Starts. In this fixed dialog that means they stack at every width; the dialog is 74 px taller in its scroll (sh 851 to 924), the body still scrolls up and down only. No markup change.
`.lng-select` (events.css, shell.css): `git grep -n lng-select -- apps/web/src e2e` found only the two rules; deleted.

## Imports
`events.css`: `CreateEventDialog.tsx`, `EventsDialog.tsx`; `shell.css`: `lounge/LoungeDialogs.tsx`. Both in the lazy Lounge chunk; no first paint, no public page. No unit test renders the dialog.

## STACK-ROUND
Slot 2 granted 12:23 (`status`: `trans 12:23`, `event 12:23`). Base rounds (unedited sheets copied back from scratch, no stash) and after rounds, one DB `sr_p103_event_ui`, ports 8802/5802/9502; seed printed no FAIL (0). Released 12:32 (`released`; status then `arcade 12:31` only). Ports free after each.
Base round: problems = the four sideways scrolls (537/400). After: `problems: []`, spec passed. The base file for the large case was recorded in a second base round after the spec's selectors were fixed.

## Figures (base to after)
| case | body sw/cw | Starts, Ends boxes | grid h |
|---|---|---|---|
| 1024 Light | 537/400 to 400/400 | 251x40 and 254x40 side by side to 360x40 each, stacked | 61 to 134 |
| 1024 Dark, 1280 Light | same | same | same |
| 1024 Light large | 537/400 to 400/400 | same | 61 to 134 |
| 375 touch plain and large | 343/343 both | 303x44 stacked, identical | 142 both |
Input `scrollWidth` = `clientWidth` (358/358) after at 1024 and 1280; 301/301 at 375. Radios start at the body's inline start. Dialog height equals viewport minus 32, scrolls only vertically. 375: control boxes equal the base (compare, 1 px tolerance), hit test 12 pass 0 fail plain, 13 and 0 large, same as base. At 1024 the hit test reads 4 pass 8 fail both base and after (pointer width, 21 px reach on 32 to 40 px controls: SEC-AUD-467, not a fault of this change).

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-event`, 11 PNG; I opened the six asked and `base-event-1024-lounge-light`)
- `base-event-1024-lounge-light.png`: Ends cut off at the dialog edge ("11/18/2026 0"), label "ENDS (OPTIONAL)" clipped.
- `event-1024-lounge-light.png`, `event-1024-lounge-dark.png`, `event-1280-lounge-light.png`: Starts and Ends stacked, each whole with its picker icon, radios whole, Dark has the right fills.
- `event-375-lounge-light.png`: same stacked layout as before, nothing cut.
- `event-1024-large-lounge-light.png`: "Somewhere else" with long location, topic and description: fields whole, nothing sideways. (The cover preview is below the fold of the dialog's scroll; its box is in the figures only.) The other `base-` twins (dark, 1280, 375, large) were not opened.

## STALE
`p102-lounge-shots` (after phase only, same world): passed, `problems: []`; its event scenario (1 select, hit 4/8 at 1024, 12/0 at 375) has no stale expectation.

## TYPECHECK
Not needed: no `.ts`/`.tsx` under `apps/web` or `packages` edited. e2e `tsc --noEmit` exit 0. prettier --check clean, `check-css-tokens.mjs` exit 0, `i18n-audit --max 0` TOTAL 0, `check-doc-links.mjs` ok (12:32).

## Commander's command
None (merge `fix/p103-event`; no UNRUN). Spec: from `e2e/`, `SR_P103_EVENT_ON=1 SR_P103_EVENT_PHASE=after npx playwright test tests/p103-event-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` on the sweep world.

## Closing texts
- SEC-AUD-459: fixed (D-978). The Create Event dialog's date fields no longer run past the box: Starts and Ends share a line only where each fits at 16rem, otherwise Ends goes under Starts; body scrollWidth 537 to 400 at 1024/1280.
- SEC-AUD-466: closed. The two unused `.lng-select` rules (`events.css`, `shell.css`) are deleted; nothing named the class.

## Proposed rows, changelog, questions
- Proposed rows: none.
- Changelog: "The new-event dialog no longer runs past its edge: the end date goes under the start date."
- Owner question: in this fixed 402 px dialog the two dates now always stack; a wider dialog would put them side by side again. Chosen as the Discord-like default.

## Clean-up
Database `sr_p103_event_ui` dropped, `p103-event-bin` deleted, stack stopped by PID tree, ports free. Final `heavy.sh status`: see below.
