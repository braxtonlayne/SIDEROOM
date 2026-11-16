# Wave 85, kit builder: SEC-AUD-365 (D-870), SEC-AUD-366 (D-878), SEC-AUD-367 (D-871)

T = 15:00 on 2026-11-16. No new item from 16:00, checkpoint 16:15, finished 16:30, hard limit 16:45. Base 774d3073. Branch `fix/p85-kit`.

## Done (commits)
- `SEC-AUD-365` `packages/ui/src/primitives/Overlay/useFloating.ts` (the guard), `Overlay/zoom.test.tsx` (one case), new `util/position.test.ts`.
- `SEC-AUD-367` `apps/web/src/components/entrance.css` (one rule).
- Local spec `e2e/tests/p85-kit-shots.spec.ts` (needs the sweep's seeded world; header says so).
- **Not built: SEC-AUD-366 (D-878): the fault is not there** (see item 2). `Tabs.tsx` is unchanged.

## Item 1: SEC-AUD-365, the "More" menu (D-870)

**Step 0 (no lock).** `position.test.ts`: anchor `{989, 316, 28, 28}`, float 200 x 560, viewport 1280 x 800, `bottom-end`, offset 4 gives **(817, 232)**, as the plan said; the same call with an anchor of all zeros gives (8, 4) (second case in the test).

**Dev server serves the kit from source** (the temporary logs showed up at once).

**Step 1, the console lines of the base** (1280 x 800, Lounge Dark, a message in the middle of the list; the temporary logs are gone, `git diff` shows none):
```
P85LOG dropdown-open {"conn":true,"rect":[989,316,28,28],"trig":[989,316,28,28]}
P85LOG update {"tag":"SPAN","cls":"_anchor_uczx0_1","conn":true,"disp":"flex","rect":[989,316,28,28],"ow":200,"oh":560,"vp":[1280,800],"res":{"x":817,"y":232,...,"placement":"bottom-end"}}
P85LOG update {"tag":"SPAN","cls":"_anchor_uczx0_1","conn":true,"disp":"inline-flex","rect":[0,0,0,0],"ow":200,"oh":560,"vp":[1280,800],"res":{"x":8,"y":4,...,"placement":"bottom-end"}}
```
The base boxes (spec, `base-more-*`): trigger 989,316 28 x 28, menu **(8, 4) 200 x 560 at the click, 300 ms and 1 s, endDelta -809**; with the pointer off the message the toolbar is `display: none`, trigger 0 x 0, menu still (8, 4).

**Case A: the anchor read had no box.** Cause in one sentence: the first `update()` reads the wrapper `<span class="anchor">` correctly (817, 232), then a second `update()` runs while the message toolbar (`.lng-msg-tools`) is `display: none` (`lounge.css:1085-1090`: it shows only on `.lng-msg:hover` / `:focus-within`), so the wrapper's computed `display` is `inline-flex` instead of the flex item's `flex` and its rect is 0,0,0,0, and `computePosition` puts the menu at (8, 4); nothing re-measures afterwards. The line that proves it is the third log line above (`rect: [0,0,0,0]`, `disp: inline-flex`, `res: 8,4`). The planner's reading (a wrapper with no box at an instant) was right; the wrapper is connected, its ancestor is `display: none`.

**What I changed** (`useFloating.ts`, the kit side only; `MessageItem.tsx` and `lounge.css` not touched): `hasBox(anchor)`: an element anchor that is not connected, or (where the browser has `checkVisibility`) measures 0 x 0, is not used. `update()` then keeps the position already set; with none yet the element stays hidden and the anchor is measured again on the next animation frame, `UNMEASURED_ANCHOR_FRAMES` = 12 frames at most, cancelled when the effect re-runs, on close and on unmount. The 0 x 0 test is skipped where `checkVisibility` is missing because jsdom measures every element as 0 x 0 (every kit test with a Popover, Menu or Select would otherwise stay hidden). `position.ts` unchanged (case B did not apply).

**After** (same spec, change in; `more-*`): trigger before the click 989,316 28 x 28.

| moment | menu box | menu inline-end minus trigger inline-end (before the click) | in viewport |
|---|---|---|---|
| click (+100 ms) | 817,232 200 x 560 | 0 | yes |
| 300 ms | 817,232 200 x 560 | 0 | yes |
| 1 s | 817,232 200 x 560 | 0 | yes |
| pointer moved off the message | 817,232 200 x 560 (unchanged) | 0 | yes |

The menu is pinned on the block axis only (taller than the room above or below), so it covers its trigger. A consequence the plan did not name: **the pointer is then over the menu, not the message, so the toolbar hides (`display: none`) while the menu is open and on Escape the focus cannot return to "More"** (it lands on `body`; the base did the same, measured: `more-escape active body`). The kit's `DropdownMenu` focuses `[aria-haspopup]` inside its wrapper, which is `display: none`. **Rule for the commander / the lounge builder** (`lounge/lounge.css`, next to `.lng-msg:hover .lng-msg-tools`): keep the toolbar shown while one of its menus is open:
```css
.lng-msg:has(.lng-msg-tools [aria-expanded='true']) .lng-msg-tools { display: flex; }
```
(use the same `display` value as the existing `:hover` rule). With nothing open the toolbar still hides when the pointer leaves (`more-hides`: toolsDisplay none, as in the base). Not done by me: the file is the lounge builder's.

**Unchanged against the base** (before / after, from `base-measure.jsonl` and `measure.jsonl`): toolbar emoji picker 481,146 360 x 369 / 481,146 360 x 369 (at click, 300 ms, 1 s, pointer away); right-click menu at the pointer (553,416): 553,8 200 x 560 / 553,8 200 x 560 (the base gave the same: it is flipped up and pinned); the channel header's notification menu (default placement): 504,95 200 x 289 / 504,95 200 x 289; a kit Select's list in Settings > Appearance: 498,199 472 x 280 / 498,199 472 x 280; "Add Reaction" sub-menu of "More": base (203,11 200 x 254) beside the menu (8,4), after (1012,239 200 x 254) beside the menu (817,232): same offset to its parent (+195 x, +7 y); "More" at 375 (touch): 156,240 200 x 560, trigger 328,208 28 x 28, inline-end delta 0 / the same. **A tooltip: not measured** (my query for `role=tooltip` found none on the header button on either run; no figure, say so). Vitest: `util/position.test.ts` 2 passed, `Overlay/zoom.test.tsx` 8 passed (one new case: an anchor with no box is not placed against, the last position is kept when the box is lost), `primitives/overlays.test.tsx` 17 passed, all single files. **I did not run the new vitest case on the base** (the e2e base run is the failing check).

## Item 2: SEC-AUD-366, the tab strip at load (D-878): not reproduced, nothing built
Admin > Moderation at 375 x 812, Lounge Dark: 7 tabs, the **selected tab at load is "Cases" (the second)**; list `scrollLeft` 0, `scrollWidth` 673, `clientWidth` 343, `data-overflow="end"`, at load, 300 ms and 1 s. The temporary log in the effect: selected `cases`, list rect 16..359, tab rect 125.4..197.3: fully inside, so the effect scrolled nothing. The shot (`tabs-moderation.png`, opened) shows Dashboard, Cases, Holds, Appeals from the start, the first tab not cut. Saved items in the Lounge: selected Bookmarks (first), `scrollLeft` 0 at the three times. A server's settings has no tab strip at 375 (a "Sections" select). A late tab chosen by a tap (Takedowns, the 7th) comes into view: `scrollLeft` 330, `data-overflow="start"`, tab inside the list (`inView` true). Case: **the fault is not there** with this world; D-878 is dropped. Wave 84's shot could have been made with another selected tab or an older build; I did not find it. Open question: Moderation opens on "Cases", not on "Dashboard" (the first tab): a page choice, not the kit's.

## Item 3: SEC-AUD-367, the card's form (D-871)
One rule, directly after `.ent-invite-card`'s block in `entrance.css`: `.ent-invite-card > form { align-self: stretch; }` (no section, no theme name; D-385). The shots were taken with the same rule one block earlier in the file (my first edit landed after another rule; moved before the build, same selector, same cascade).

Boxes (x, y, w, h; content box of the card = the `p` lines):
| setting | card content box (left, width) | form before | field before | button before | form after | field after | button after |
|---|---|---|---|---|---|---|---|
| in the app, 375 (all 3 themes) | 56, 263 | 67,504 241x113 | 67,525 241x44 | 67,581 241x36 | 56,504 **263**x113 | 56,525 263x44 | 56,581 263x36 |
| in the app, 1280 (all 3 themes) | 452, 376 | 519.5,490 241x109 | 519.5,511 241x40 | 519.5,563 241x36 | 452,490 **376**x109 | 452,511 376x40 | 452,563 376x36 |

`doc` sideways scroll 0 and nothing outside the card (0 boxes) before and after, every case. The label ("Server name") box widens with the form and stays start-aligned over the field (it was the same width as the field before): no second rule needed. Every other box of the template card is identical before and after (listed in `measure.jsonl`: the lines, the list). **Cards with no form: nothing moved** (template signed out 375 and 1280, invite in the app and signed out at 375 and 1280: every child's box is identical in `base-measure.jsonl` and `measure.jsonl`).

## Build and checks
`heavy.sh run kit ... build` ran once (waited for the lock, exit 0, 13 s of vite), after the temporary logs were gone. First paint `app/app-BpEuVXB3.js` **218 321 bytes gzip, room 37 679 bytes** (identical to wave 84: the change adds no bytes to first paint). Wave 84: 218 321 bytes gzip, room 37 679; wave 83: 218 309. `check-bundle-budget`, `check-entrance-isolation` ("every public surface is clean") and `check-staff-delivery` all passed. `apps/web/dist` is ignored; deleted.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p85-kit` (local, not committed): 48 PNGs (24 after, 24 base) and the two `measure.jsonl`, two `console.txt`. **All 48 opened.** After: More menu open and with the pointer away, the emoji picker (2), the sub-menu, the right-click menu, the header's menu, the Select list, More at 375, the three tab strips, the template in the app at 375 and 1280 in Editorial, Lounge Light and Lounge Dark (6), signed out (2), the invite in the app (2) and signed out (2). Base: the same set (the base template at 375 and 1280 in all three themes shows the 241 px form).

## Docs
No doc states a limit of this. A sentence for `docs/10-design-system-and-theming.md` if D-870 belongs there: "A floating element (menu, popover, tooltip, select list) is never placed against an anchor that is not in the document or measures 0 x 0: it keeps the place it has, or stays hidden and is measured again on the next frames (12 at most) until the anchor has a box." I did not edit that file (not in my list).

## Proposed inventory rows
- SEC-AUD-365: "The message More menu opened 809 px from its trigger: its anchor was measured while the toolbar was display: none" - fixed (D-870, `useFloating` guard). Open follow-up: the toolbar hides while the menu is open (CSS rule above), so focus does not return to "More" on Escape.
- SEC-AUD-366: "Moderation's tab strip scrolled to its end on load" - not reproduced (selected Cases, `scrollLeft` 0 at load, 300 ms, 1 s at 375); closed as not reproduced; D-878 dropped.
- SEC-AUD-367: "The invite/template card's form 241 px in a 440 px card" - fixed (D-871).
- Whether SEC-AUD-355 and SEC-AUD-348 close: 355 (the More menu's placement) closes with 365 for the placement, but its "focus on More after Escape" part stays open until the CSS rule lands. I did not look at SEC-AUD-348's text.

## Changelog sentences
- A message's "More" menu now opens beside its button instead of in the corner of the window.
- (366: nothing to say.)
- The name field and the create button of a server template card now fill the card.

## Owner questions
- Moderation opens on "Cases" rather than "Dashboard": intended?
- The More menu covers its own button when it is taller than the room (D-870 pins it on the block axis); acceptable, or open it upwards over a shorter list?

## Cleanup
Stack stopped by PID and by port, `sr_p85_kit_ui` dropped, `p85-kit-bin` deleted at the end, `p80-base-bin` untouched. No stash touched.
