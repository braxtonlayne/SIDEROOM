# fix-p104-loungeshots (wave 104, shots builder of pod lounge; D-992, D-995, D-996, D-997)

Clock: started 14:13, spec compiled 14:17, last round ended 14:50, handoff 14:55. Changed: `e2e/tests/p104-lounge-shots.spec.ts` (new) and this file. No app file, sheet or other spec.

## Result
All four code builders are merged into this branch (keys c355044b, event 56c6a679, forum 2a6b51d0, bridge 70900d15 with its cure). **The after phase on the merged tree: `1 passed`, `problems: []`.** One fault was found and cured on the way (bridge, below). Shots: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p104-lounge`.

## STACK-ROUND
Stack 8801 / 5801 / 9501, database `sr_p104_loungeshots_ui` (fresh, sweep world seeded, no FAIL line), binaries copied from `p92-base-bin`.
- Slot waits: base round 14:17 to 14:27 (10 minutes: `heavy.sh` answered 75 six times, mount, chip and reader held both slots). Later rounds: 14:37 slot at once; 14:42 slot at once; 14:48 after waiting from 14:45 (drill).
- 14:27 to 14:33 (`status`: `loungeshots 14:27`): base on the unmerged tree (spec proof + fixes: `past` check restricted to the form, bridge map scenario, label names); this round ran over its 6 minutes because the spec had to be fixed twice (Mirror fieldset name, my own mistakes).
- 14:34 to 14:36: merge of the four branches (no conflict), after round 1: 1 failed, bridge problem lines only (FAULT below). Lock released 14:36.
- 14:37 to 14:40 (`loungeshots 14:37`): bridge cure merged; base re-run for keys and event with the pod's 8 app files put back by `git checkout ccb3bacc -- <files>` and restored by `git checkout HEAD -- <files>` (git status clean afterwards, no stash), then after: 1 failed (event zoom90 `past` artifact of my spec: CSS zoom scales the box but not the padding I subtract; the body's scrollWidth equals its clientWidth; bridge 375 ECONNREFUSED: the stack died mid-run, a spec/stack error). Released 14:40.
- 14:42/14:48 to 14:50: after rounds 3 and 4. Round 3: one problem (spec: the Switch rule applied at 375 where the Switch is on its own row, x 17 in both phases). Fixed in the spec. **Round 4 (14:48 to 14:50): `1 passed (1.6m)`, `problems: []`.** Ports checked free, stack stopped by PID, database dropped, `heavy.sh status`: `free`.
- No 429 happened (one retry helper for creates is in the spec; never used).

## TYPECHECK
- e2e: `npx tsc --noEmit -p tsconfig.json` from `e2e/`, 14:50, exit 0 (also after each edit).
- Merged tree, STEP TYPECHECK: `bash .../tc.sh run loungeshots bash -c "cd C:/Users/braxt/Desktop/SIDEROOM_wt/p104-loungeshots/apps/web && npx tsc --noEmit -p ."` 14:50 to 14:51, exit 0.
- prettier on the spec: `npx prettier --write` (clean after the last edit). eslint ignores `e2e/` (outside the web package's base path: "File ignored", exit 0): not a check.

## Figures, base against after
(1024 = 1024x768; boxes in px.)

### keys (D-995, "no box")
| step (1024 Light) | base | after |
|---|---|---|
| search Sort, Alt+ArrowDown | path changed to the next channel (b), results panel closed, focus on `body` (the fault) | path unchanged, panel open, focus on the Sort select |
| Escape after it | n/a (already left) | panel and focus unchanged (a native list, if it opened, took the key) |
| plain ArrowDown on the search Sort | value new -> old, ArrowUp puts it back (D-980 holds) | same |
| forum Sort, Alt+ArrowDown | path changed to another channel, focus `body` | path unchanged, focus on the Sort |
| forum Sort, ArrowDown | activity -> created, back | same, and the post order followed: [one, two] -> [two, one] -> [one, two] |
| Create Event channel select (voice kind), Alt+ArrowDown | path changed to the next channel (the fault; the dialog stays open on the old page state) | path unchanged, focus stays on the select "Channel" |
| message list, Alt+ArrowDown / Alt+ArrowUp | next channel, comes back | same (both phases, asserted) |
| a button (Back), Alt+ArrowDown | next channel | next channel |
| body, Alt+ArrowDown | next channel | next channel |
Verdict: the decision holds; nothing else moved.

### event (D-992)
| | base | after | limit |
|---|---|---|---|
| dialog 1280 / 1024 | 402 x 770 / 738 | 562 x 770 / 738 | about 560 |
| dialog 375 | 345 x 782 | 345 x 782 | equal |
| body scrollWidth/clientWidth 1280, 1024, 375 | 400/400, 400/400, 343/343 | 560/560, 560/560, 343/343 | equal |
| Starts, Ends | stacked (y 556.5 and 629.5), 360 wide | **one line (y 518.8), 254 wide each, x 252 and 518 at 1024** | one line |
| date field sw/cw | 358/358 | 252/252 (no clip; the 2 px is the border) | whole |
| 375 date fields | stacked, 303 x 44 | stacked, 303 x 44 | equal |
| every control box at 375 | | equal within 1 px (spec compares all 12+) | |
| hit test (reach box at 1280 and 1024, default at 375), pass/fail | 12/0 each | 12/0 each | no worse |
| fractional width (account zoom 90, 110, 125 at 1024; the dialog box 504, 616, 700 in zoomed px) | not run | dates on one line at all three, 252/252, body 560/560, hit 12/0 | stack, never overflow |
Verdict: none of the may-change limits left, no must-not-change broken. The zero-slack fit holds at 1280, 1024 and the three zooms.

### forum (D-996)
| | base | after | limit |
|---|---|---|---|
| Sort box 1024 | 109 x 19 | 148 x 40 | width +36 plus the type: +39 here, of which 11 px is 13.33 -> 14 px text (86.7 -> 97.7), so inside |
| Sort box 375 | 109 x 19 | 148 x 44 | 44 |
| text clear of chevron | n/a (raw) | text right 164.3, chevron 180.6 | clear |
| select font-size | 13.33 px | 14 px | 14 |
| label span font-size | 16 px | 14 px | note |
| labelGap | 6 | 6 | not under |
| search field | 330.4 x 29, 13.33 px | 330.4 x 29, 13.33 px | equal |
| view buttons | 28 x 28, x 177.3 | 28 x 28, x 209.6 (+32.3) | within the Sort's width growth (+39) |
| toolbar height 1024 | 72 | 84 (+12) | not more than the Sort's growth (+21) |
| toolbar height 375 | 72 | 88 (+16) | not more than +25 |
| toolbar rows | 2 (New Post, search; then Sort, views) | 2 | same wrap |
| first tag chip top / first post top | 84 / 113 | 96 / 125 (+12) | moved by the toolbar's growth |
| sideways scroll at 375 | none | none | none |
| hit test 1024 | 5/0 | 5/0 | |
| hit test 375 | 1 pass, 4 fail | 2 pass, 3 fail | no row worse (the fails take their points to `div.lng-forum-tools` / `div.lng-forum`, i.e. the controls are shorter than 44 px: see proposed rows) |
| keyboard | ArrowDown activity -> created, post order followed, ArrowUp back | same |
Verdict: no limit broken. Wave 103's put-back reason (162 px wide, 16 px type) does not recur.

### bridge (D-997)
First after round (bridge's first version: 400 weight, a 9 px margin) is the FAULT below. After the cure (70900d15):
| | base | after the cure | limit |
|---|---|---|---|
| Kind select 1024 | 246 x 40 | 250 x 40 | not wider than +36 |
| Kind select 375 | 246 x 44 | 250 x 44 | 44 |
| select type | 14 px / **600** / none / normal | 14 px / 600 / none / normal | the base's |
| label text type | 14 px / 600 / uppercase / 0.56 px | same | unchanged |
| label gap | **8** | 8 | not under the base, at most +2 |
| label and field on one line | yes, 1 label line | yes, 1 | |
| text clear of chevron | n/a | text right 158.1, chevron 300.6 | clear |
| Switch x 1024 | 325.6 | 329.6 (+4 = the Kind's growth) | the Kind's growth |
| Switch at 375 | on its own row, x 17, y 118 | same | equal |
| card scrollWidth | no sideways scroll | none | |
| hit test | 4/0 (1024 Light, Dark, 375) | 4/0 | |
| other labelled controls of the page | Mirror another channel fieldset (selective mode, 3 kit selects, labels 12 px 600 uppercase, stacked) | every box, gap and style equal within 0.5 px, 1024 and 375 | must not move |
| keyboard | ArrowDown value "" -> permission_reverted, ArrowUp back | same | |
The wave 103 figure "14 px bold" was right for the base (600), and the base gap is **8**, not 9 (bridge's handoff still describes the first version: 400 and a 9 px margin; the commander corrects it at integration). PlanView's selects (`PlanView.tsx:450,461`) are not on screen in this fake (they need a real plan); not measured: say so.

## FAULTS
1. **bridge, `server-settings.css` (end block) and `ConflictLog.tsx`, the Kind label and select, found in after round 1 at 14:36 (sent to the commander at once).** Cause measured: `admin.css:599` `.ss-root .ss-toolbar > label:has(> select)` gave the base label `display:inline-flex; gap:8px; font-size:14px` and the select its weight 600 (inherited); with the kit's wrapper the select is no longer a direct child. Figures: label text 14 px / 0.56 spacing -> 12 px / 0.48; select weight 600 -> 400; gap 8 -> 9; the Switch moved -4.66 px while the Kind grew +1. Cure (b), rows it may move: the Kind label, its text span, the Kind select, the Switch: a `.ss-root .ss-toolbar > label.ss-field-label:has(> * > .ss-field-pick)` rule restoring the inline-flex, gap 8 and 14 px, the pick at weight 600, the text span's margin 0. **Committed by bridge as 70900d15 and re-shot: after round 4, all of the above equal to the base, no problem line.** (Kept: `admin.css` is another builder's/nobody's file; it still matches `:has(> select)` for the raw selects of Admin.)

No other builder has an entry. keys, event, forum: none.

## Shots (all opened; base beside after, 11 listed + extras)
- `keys-sort-altdown-1024-lounge-light.png`: after: results panel still open on "p104lq... line m3" hits, the Sort has focus ring and the native list is open under it (Newest / Oldest / Most relevant); base: the panel is gone, the next channel's welcome page is showing.
- `event-1280-lounge-light.png`: after: 562 wide dialog, Starts and Ends side by side, whole dates and pickers, no scroll; base: 402 wide, dates stacked, description wraps to two lines.
- `event-1024-lounge-light.png`, `event-1024-lounge-dark.png`: after: same, both themes, dark text and borders fine, dates side by side; bases: stacked at 402.
- `event-375-lounge-light.png`: after equals base visually (same dialog, stacked dates, nothing cut).
- `event-zoom90.png`, `event-zoom110.png`, `event-zoom125.png` (extra): dates on one line, nothing cut or sideways; zoom90 has a black band under the page (the zoomed viewport), zoom125 is cut by the viewport, not the dialog.
- `forum-1024-lounge-light.png`, `forum-1024-lounge-dark.png`, `forum-375-lounge-light.png`: after: the Sort is the kit field (bordered, chevron, "Recent activity" whole), "Sort by" at 14 px, the second row is taller; bases: raw small browser select with 16 px label. No crowding; the label is not touching the field.
- `bridge-1024-lounge-light.png`, `bridge-1024-lounge-dark.png`, `bridge-375-lounge-light.png`: after: "SHOW" uppercase label, kit field with bold "Everything" and chevron, Switch beside it (375: Switch on its own row, same as the base); bases: raw select 4 px narrower with the native arrow. Label text and gap look the same.
- `bridge-map-1024-lounge-light.png`, `bridge-map-375-lounge-light.png` (extra): the "Mirror another channel" fieldset is identical in base and after.

## STALE (earlier specs; I edit none)
- `e2e/tests/p103-lounge-shots.spec.ts:263` (`forum-sort`, `moved: []`) and `:322` (`bridge-conflicts`, `moved: []`): the after phase of that spec would flag these two selects as "unmoved select whose size differs from the base" now that D-996 and D-997 move them (their stops were written for the put-back).
- `e2e/tests/p103-event-shots.spec.ts:10-11` (header: "otherwise Ends goes under Starts" as a description of the 402 px dialog) and its 375-only `compare`: the prose is true only below 560; nothing asserts the stacking. Commander's line 198 reference: no assertion of stacking found there.
- `e2e/tests/p103-search-shots.spec.ts:207` (comment: "Alt+ArrowDown is the Lounge's next-channel shortcut") and `:347-353` ("the Lounge's next-channel shortcut takes it (recorded, not asserted)"): untrue after D-995; it only records, so it does not fail.

## The commander's command, whole
From `e2e/`, on a stack with the sweep world (`SR_ADMIN_REQUIRE_REAUTH=0`, `DATABASE_URL`, `SR_CLI_BIN` set):
```
# base (pod's eight app files at ccb3bacc, or the unmerged tree)
SR_E2E_BASE_URL=http://localhost:<vite> SR_P104_LOUNGE_ON=1 SR_P104_LOUNGE_PHASE=base \
SR_P104_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p104-lounge \
npx playwright test tests/p104-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
# after (merged tree): the same without SR_P104_LOUNGE_PHASE=base
```
`SR_P104_LOUNGE_ONLY=keys,event,forum,bridge` runs a subset. MOVED (spec top) is all true: set one to false to expect the base for that scenario if it is put back.

## Spec notes / limits
- Found by role, label, id or tag in a role-found root; data `p104l-*` (3 text channels, 3 messages, a forum with 2 posts and a tag try) removed in a `finally`.
- Not driven: PlanView's selects (no real plan); no Escape detection of a native list (headless: recorded, not asserted).
- Display names in the spec are real labels ("Sort by"). The forum posts' order follows the Sort in the keyboard step.

## Proposed rows (base faults, not of this wave)
1. Forum toolbar at 375 (`lng-forum-tools`): the search field (29 px) and the view buttons (28 px) fail the 44 px hit test in the base and after (taker: `div.lng-forum-tools`, a container). Taker: a Lounge/forum builder (`forum.css`). Low.
2. `admin.css:599` `:has(> select)` rules style raw selects by structure: any kit select moved into a `.ss-toolbar` label loses the label's inline-flex and type (the cause of this wave's bridge fault). Taker: whoever moves the next toolbar select; or give the Admin toolbar a named class.
3. The bridge map's "Mirror another channel" selects inherit 12 px / 600 from their label (selects read bold at 12 px). Taker: bridge/settings builder; would want `ss-field-pick`-like classes.
4. PlanView's `.ss-field-label` selects: not measured.

## Owner questions
None (the weight question: the base was 600; bridge's cure keeps 600).

## PROCESSES
After cleanup (stack stopped by PID 22584 and 12348 in the last round; the stray vite node 26164 stopped by PID):
- listing of `*p104-loungeshots*`: only `bash.exe` shells of this session and the listing's own `powershell.exe`; no node, no sr-server.
- `netstat ... LISTENING | grep -E ":(8801|5801|9501) "`: no line (ports free).
- `heavy.sh status`: `free` (no hold in my name). Database `sr_p104_loungeshots_ui` dropped; `p104-loungeshots-bin` deleted; `e2e/.state` removed; the scratch folder `.scratch/` (untracked, not committed) holds my round script and logs.
- The p103 stash list: untouched.

## Changelog sentence
None of mine (spec only).
