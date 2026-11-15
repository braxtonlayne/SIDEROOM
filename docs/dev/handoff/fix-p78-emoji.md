# Wave 78 handoff: emoji (SEC-AUD-306, D-806)

Branch `fix/p78-emoji` (base `8694067e`). Web only: no Rust, no migration, no generated file, no new string.

## Done

- **SEC-AUD-306: every column of the emoji picker's grid is whole.** Commit `SEC-AUD-306: every column of the emoji grid is whole (D-806)`.
  - `apps/web/src/lounge/components/emojiPicker.css`: `.lng-emoji .lng-emoji-grid { grid-template-columns: repeat(7, minmax(0, 1fr)) }`; `.lng-emoji .lng-emoji-btn { min-inline-size: 0; padding: 0 }` (the browser's 6px side padding on a button was what made a cell as wide as 42px); `img` in a button `max-inline-size: 100%`; the picker `max-inline-size: 100%` (never wider than its panel on a narrow phone).
  - `apps/web/src/lounge/msg/msg.css`: the sticker grid (`repeat(4, 1fr)`) and the GIF category grid (`repeat(2, 1fr)`) became `minmax(0, ...)` columns, the same hazard (not shot, see below).
  - **Seven columns stay**; the cell is **39.8 px** wide at both sizes (the cell was 42.2 px on the base).
- Spec `e2e/tests/p78-emoji-shots.spec.ts` (new, local-only): base case, whole-column measurement, shots, click-through, and the tail's popover look-over (second `test`).

## The fault, measured (confirmed first, on the base, CSS held back, same stack)

`SR_P78_BASE=1` run (passes only when a column is cut on the phone):

| size | scroll area client | content width | buttons cut | cell | last button right edge vs client right |
|---|---|---|---|---|---|
| 1280x800 (desktop) | 279 | 296 | 6 of 54 | 42.2 | 1017.5 vs 1001.0 |
| 375x812 (phone) | 279 | 296 | 6 of 54 | 42.2 | 365.5 vs 349.0 |

The desktop is cut too (the same numbers: the picker is a fixed 320px + 8px padding at both sizes; the panel is 360px on the desktop and 359px on the phone, `100vw - 16px`, so the panel never limited it). Cause as the plan said: a `1fr` column does not shrink below its button's content (a 22px glyph plus the button's default padding), so seven were 296px in a 279px area and the last column hung 16.5px past the end edge (and was clipped by the panel).

After (same stack, CSS in place): desktop and phone, every theme, top, a later category, after pressing every category, with favourites, search ("smile"), the reaction picker, the thread pane: `cut 0`, `scrollWidth 279 = clientWidth 279`, first button at the area's start edge (722.0 / 70.0), last at its end edge (1001.0 / 349.0), the picker (673..1009 desktop, 21..357 phone) inside its panel (660..1020, 8..367), page overflow 0, same gap either side. All asserted in the spec; the numbers are in `measurements.txt` of the shot folder.

## Shots

Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p78-emoji`, **19 PNGs, all 19 opened and looked at by me**:
- 2 base shots (`base-emoji-desktop.png`, `base-emoji-phone.png`: the cut column visible on the phone: the seventh emoji of each row is half drawn).
- 12: `emoji-top-<desktop|phone>-<lounge-editorial|lounge-light|lounge-dark>.png` and `emoji-later-...` (scrolled to the Animals/Food/Symbols category by pressing its button).
- `reaction-phone-lounge-dark.png`, `reaction-phone-lounge-light.png` (the reaction picker from the "+" beside a reaction), `thread-emoji-phone-lounge-light.png`, `emoji-search-phone-lounge-dark.png`.
In every one: seven whole emoji per row, the same 8px gap either side of the grid, no sideways scroll, the category column and the footer line intact. Account theme set by `PATCH /users/@me/settings` and localStorage (as `p77-picker-shots.spec.ts`).

Click-through (in the spec, run on the real stack): pressed every category button (the area scrolls to it); picked the seventh emoji of the first row on both sizes (it lands in the composer); right-clicked one (it is marked `data-favorite`, and a Favorites section appears with the grid still whole); typed "smile" in the search.

## Tests run

- `p78-emoji-shots.spec.ts` with `--project=chromium --workers=1 --reporter=line`: base case passes on the base (fault confirmed), main test passes with the change (41 s). Watched fail on the base: yes (the base run asserts the fault, and the main test's measurements fail on the base numbers above: `cut 6`).
- `emojiPicker.test.tsx` (single file): 9 passed. `check-css-tokens`: clean. `i18n-audit --max 0`: 0. e2e `tsc`: clean. prettier: clean on all three files. (eslint cannot lint `e2e/` from the web package: "outside base path", no finding.)
- No web build: `emojiPicker.css` is imported by `EmojiPicker.tsx` only; that is imported by `Composer.tsx`, `MessageItem.tsx`, `Reactions.tsx`, `ExpressionPicker.tsx`, `CustomStatusDialog.tsx` (all in the Lounge chunk, loaded by `routes/LoungeRoot.tsx` through `lazyNamed`) and nothing in first paint. `msg.css` is imported by `Composer.tsx` and `MessageList.tsx` (Lounge). `lounge.css` is untouched.

## GIF and sticker pickers

Their grids were built the same way (`.lng-sticker-grid` `repeat(4, 1fr)` with square tiles, `.lng-gif-cats` `repeat(2, 1fr)`); I gave both `minmax(0, 1fr)` columns. **Not shot**: the test stack has no stickers and no GIF source (they show the empty state, as in wave 77), and I did not add a sticker. The GIF result grid is a CSS multi-column layout (`column-count: 2`), which does not have the problem.

## Tail: the Lounge's other popovers on a phone

Second lock round (13:23 to 13:26), fresh stack, phone 375x812, Lounge Dark and Lounge Light, spec . Measured: panel inside 0..375, no descendant past the panel's sides, scroll area .

| popover | reached | before | after |
|---|---|---|---|
| Threads (header list icon) | yes | whole | whole |
| Pins | yes | CUT:  360px + 16px padding (content-box) = 376 in a 359px panel (content 21..375, scroll 366/357) | **fixed**: whole |
| Saved items | yes | CUT:  400px + 16px, content to 382 | **fixed for the box**; the kit's tab strip (Bookmarks, Reminders, Scheduled) still runs past the end: it is the kit's own  list ( Tabs, fenced), the third tab ("Scheduled") is half drawn and reached by scrolling the strip. Listed, not fixed |
| Inbox | yes | whole | whole |
| Custom emoji popout (a message's custom emoji) | yes (a 1px PNG emoji added by API) | whole | whole |
| Thread members (thread pane header) | yes | whole | whole |
| Input/output device menus | NOT reached: the user panel is not on the phone's Home page (it lives under You), the trigger was off screen | | read only:  has no fixed width problem found by reading; not measured |
| Soundboard, privacy codes, per-user volume | NOT reached: need a live voice call (not set up) | | listed for a wave with a voice stack:  (voice-p13.css),  (voice-p14-e2ee.css),  (lounge.css:1986) |
| Reaction picker | yes (item above) | cut (same grid) | whole |

Fix: one rule at the end of :  (the rules live in  and , not mine, so the specificity is raised instead). Shots:  (12, Dark and Light, all 12 opened; Pins and Saved are the fixed ones, both themes).  holds the pre-fix run (the cut numbers above come from its measurements; its PNGs were not opened). Next plan: give  and   themselves, and the kit's Tabs a visible overflow cue.

## Docs

No doc states the picker's columns or width (searched `docs/`, `README.md`). Nothing updated.

## Proposed inventory row (the commander adds it; closes on merge)

`SEC-AUD-306` (web, Lounge): every column of the emoji picker's grid is whole at every width: `.lng-emoji-grid` columns are `minmax(0, 1fr)`, the buttons carry no padding, the picker is never wider than its panel (D-806). Measured on a real stack at 375 and 1280: 0 of 54 buttons past the scroll area's end edge (6 on the base), `scrollWidth == clientWidth`. Closed.

## Changelog sentence

Every column of the emoji picker is whole: the seventh emoji of each row is no longer cut off at the picker's edge, on a phone or on a desktop.

## Owner questions

None. (Chosen default: seven columns kept; cells are 39.8px, large enough for the 22px glyph.)

## Cleanup

Done: stack stopped by PID, scratch database `sr_p78_emoji_ui` dropped, `p78-emoji-bin` deleted, lock released. No stash touched.
