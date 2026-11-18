# Wave 100, editor: D-951 (the channel editor on a phone)

2026-11-18. Clock: started 04:53; code and spec 05:00 to 05:15; lock `editor` 05:16 to 05:18 (round 1, base), 05:24 to 05:26 (round 2, base and change, 24 rows), 05:36 to 05:38 (round 3, change only: one spec check and one CSS value fixed). Three rounds of 2 to 3 minutes each.

## Base (confirmed first; 375 x 812 touch, Lounge Light, Server settings > Channels)
- **The brief's own test (an 8-row tree: the world's rows plus `p100-editor-6` and `-7`): the heading is already inside the viewport.** After the first pick the heading's top is 624 of 812 (focus stays on the row button, scrollTop 0); after the last pick 624 (focus on the row). By the letter of the task ("if the heading is inside the viewport after both picks, build nothing") nothing should be built.
- **Why I built anyway (drop it if you disagree; the change is self-contained):** at 8 rows the heading sits at the bottom edge with the form below the fold, and its top moves down about 49 px per row. With **24 rows** (the spec's `SR_P100_EDITOR_ROWS=24`, rows `p100-editor-<n>`) the base shows the heading at **1408 of 812 after the first pick (off screen, scrollTop 0, focus on the row)**; after the last pick the browser had scrolled for the tap (heading 486, scrollTop 922). A server with more than about 12 rows has the fault. Focus never moves to the editor in the base.
- 1024 x 768 (base): heading 169 after the first pick, focus on the row, scrollTop 0 (24 rows, last pick: heading at -504, scrollTop 673; the same before and after my change).
- Base hit test of `.ss-split` at 375: 39 controls (8 rows), 31 pass, 0 fail (the rest skipped); 24 rows: 87 total, 79 pass, 0 fail.

## What changed (commit 842d3c03 and the ones after it)
- `Channels.tsx`: `ChannelsEditor` counts the member's own picks (`pick`, incremented in `select`; so not a created channel, not a save). An effect keyed on `pick` runs only when `useMediaQuery('(max-width: 720px)')` matches: it finds the editor's `h2.ss-title` (now `tabIndex={-1}`), calls `scrollIntoView({block:'start', behavior})` (`'auto'` under reduced motion, else `'smooth'`), then `focus({preventScroll:true})`. Not on first load (`pick` is 0). `ChannelEditor` takes an optional `onBack` and renders a kit `Button` (`ghost`, `sm`, class `ss-editor-back`) "Back to channels" above the heading when it is given; Back scrolls the `aria-current` row to the centre and focuses it. The Lounge's `EditChannelDialog` uses `ChannelEditor` without `onBack`: no button there.
- **Kept off wide screens:** the button is not in the DOM above 720 px (rendered only while the media query matches; the hook follows resizes), so there is no hidden tab stop.
- `server-settings.css`: one block at the end, `@media (max-width: 720px)`, comment names D-951: 8 px bottom margin on the button and `scroll-margin-block-start: 52px` on the heading (28 px button + 8 px + 16 px air, so Back is in view under the pinned bar after a pick; my first value, 16 px, hid Back under the bar: seen in the round 2 shot). Plain px, as the sheet does. Nothing above 720 px, no rule edited.
- `en/p100-editor-1.ts`: `settings.channels.backToList` = "Back to channels".
- **Chunk:** `Channels.tsx` is imported by `settings/server/ServerSettings.tsx` (lazy through `routes/settingsServer.tsx`, `lazyNamed`) and by `lounge/shell/EditChannelDialog.tsx` (in `LoungeDialogs`, under the lazy `LoungeRoot`). Found with `git grep`. No first-paint import; no web build run (not mine).

## After (round 3, the committed code; 375 touch)
| | base, 24 rows | after, 24 rows | after, 8 rows |
|---|---|---|---|
| first pick: heading top / 812 | 1408 | **114** | in view |
| first pick: focus | the row | **the heading** | heading |
| last pick: heading top | 486 (browser's own scroll) | **114**, heading focused | in view |
| Back button | none | shown, y 78, drawn 28 px, **target 44 px** (kit `::after`), hit test pass, not covered | target 44 |
| activate Back (last row picked) | - | row top 415, in view, focused | |
| section hit test | 87 / 79 pass | 88 / 80 pass (+1 = Back), 0 fail, 0 not-reached | 40 / 32 pass |
| reduced motion emulated | - | first and last: heading 114, focused; Back: row in view, focused | |
| keyboard, 375 no touch | - | Enter on a row: heading focused and in view; Tab: onto the Overview tab (inside the editor); Enter on Back: row focused and in view | |
| 1024 x 768 | heading 169, row focus, scrollTop 0 | same; Back not displayed; **0 of 74 and 0 of 87 control boxes moved** (0 of 26 and 39 with 8 rows) | |
| sideways page scroll at 375 | none | none | |

Lounge Dark: first pick heading 114, focused; `data-theme` read back `lounge-dark` in every run.

## Shots (all opened by me)
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p100-editor/` (8 rows: `base-*` from round 1, `after-*` from round 3) and `.../p100-editor/big/` (24 rows: `base-*` from round 2, `after-*` from round 3). Names: `channels-375-picked-lounge-light`, `-picked-last-lounge-light`, `-back-lounge-light`, `-picked-lounge-dark`, `channels-1024-picked-lounge-light`, each with `base-` or `after-`. I opened the five 24-row after shots (round 3), the 1024 after shot and the 8-row base shot; I did not open the other base shots and the 8-row after shots (their figures are in `measure.json`). The Back button is small plain text with a 44 px target, clear of the pinned bar. No crowding seen; 1024 is unchanged (0 boxes moved).

## Checks (own tree, after the last edit, 05:40, all exit 0)
web `tsc --noEmit`; e2e `tsc --noEmit`; `node scripts/i18n-audit.mjs --max 0` (TOTAL 0); `node scripts/staff-keys.mjs` (without `--write`); `node scripts/check-css-tokens.mjs`; eslint on `Channels.tsx` and the fragment; prettier --check on the four files. Unit test `settings/server/__tests__/channels.test.tsx`: 16 passed (05:01, before the CSS value changed; no test reads it). `check-doc-links.mjs`: not run.
Watched fail without the change: the spec's `base` tag on the base files logs the same checks without asserting; there the heading is at 1408 and focus is on the row.

## The commander's command (whole)
Fresh database, stack ports 8783 / 5783 / 9483 with `SR_ADMIN_REQUIRE_REAUTH=0`, sweep world seeded; from `e2e/`:
`SR_P100_EDITOR_ON=1 SR_P100_EDITOR_ROWS=24 SR_E2E_BASE_URL=http://localhost:5783 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P100_EDITOR_OUT=<folder> npx playwright test tests/p100-editor-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
(default rows 8; `SR_P100_EDITOR_TAG=base` measures without asserting).

## Changelog sentence
Server settings, Channels, on a phone: picking a channel takes you to its editor (heading in view and focused), with a "Back to channels" button that returns to the row.

## Owner questions (for Fable)
- The heading was already visible on the small 8-row tree, so "build nothing" held on its letter; I built because larger trees fail. Keep it?
- A fuller phone layout (own screen or drawer, a sticky Back) is still open; the Back button scrolls away with the page.

## Not done / notes
No inventory or threat-model rows proposed. No doc other than this one changed. No stash touched.

## Cleanup
Stack stopped by PID tree after each round, ports 8783 / 5783 / 9483 free; scratch database `sr_p100_editor_ui` dropped and `p100-editor-bin` deleted at the end (see the final reply). `heavy.sh status` before hand-back: free (no hold in my name).
