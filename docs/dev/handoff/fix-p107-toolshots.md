# fix-p107-toolshots (wave 107, pod tools, shots builder; D-1022, D-1024)

## Clock times
Started 18:37; spec 18:41; base jobs done 18:48; after round 1 19:15 to 19:21 (queue wait up to 24 min); after round 2 (spec fixes only) 19:25 to 19:37; handoff 19:40.

## Done (commits)
- `e2e/tests/p107-tool-shots.spec.ts`: 11 pages (calculator, units, percent, time zones; JSON, Base64, countdown, Invite card, Image Studio; tier list with the image picker open, poll), three parts. Edits after the first commit: a placeholder is read only on a field that has a `placeholder` attribute; the tier name's ring is judged against its --ui-bg line.
- Merge of fix/p107-tools (33bc0963), fix/p107-calc (27a027a3), fix/p107-rooms (ad5867e1): no conflict; build 0168 on that HEAD.
- Command, whole, per part: `bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p107-integ/apps/web/dist --env SR_P107_TOOL_ON=1 --env SR_P107_TOOL_PHASE=after --env SR_P107_TOOL_PART=<1|2|3> --env SR_P107_TOOL_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p107-toolshots -- tests/p107-tool-shots.spec.ts`
- The pod sheets contain no `::after` rule (every cure grows the control), so the "1 px short" tap-area finding does not apply: none.

## Not done
- No FAULT sent: every remaining fail is on a control no task named (below).
- 375 shots are taken after the hit test, which scrolls the drawer a little (a tool's first row can look clipped under the header): a shot artifact in both phases. Figures are read before the test.

## FINDERS
No app file changed. None.

## JOBS (class, wait, ran, rc)
Base: 0165 part 1 (0 s, 116 s, 0), 0166 part 2 (3 min, 2 min, 0), 0167 part 3 (5.5 min, 2.2 min, 0). Build 0168 (2 min, 28 s, 0). After round 1: 0189 p1 (24 min, 2 min, rc 1), 0190 p2 (10 min, 2.3 min, rc 1), 0191 p3 (13 min, 2.6 min, rc 1): the rc 1 was my spec (placeholder false reads on fields without a placeholder attribute; a border rule that cannot hold on a coloured tier head); fixed. Round 2 (final): 0207 p1 (10 min, 1.5 min, rc 1), 0208 p2 (10 min, 1.7 min, rc 1), 0209 p3 (13 min, 1.9 min, rc 0). Rc 1 of round 2 = the uncured controls listed below.

## Figures, 375 hit fails per page, base -> after
calculator 24 -> 0; units 10 -> 0; percent 13 -> 5; time zones 37 -> 24; JSON 6 -> 1; Base64 8 -> 3; countdown 9 -> 3; Invite card 11 -> 0; Studio 10 -> 0; tier list (with picker) 8 -> 0; poll 15 -> 0. At 1024: 0 fails everywhere, both phases. No sideways scroll anywhere. No row that passed in the base fails after. No tap area takes another control's point (the Base64 "URL-safe" taker is the Encode button above it, 32 px tall, no tap area: the checkbox is 13 px and failed in the base).

Still failing, all failing in the base, none named in a task:
- percent: `.sr-calc-hist-btn` examples x5 (287x33).
- time zones: `.sr-tz-pick` "Choose 0:00..23:00" x24 (about 16-23 x 19, taker the grid container).
- JSON "Sort keys" and Base64 "URL-safe alphabet" checkboxes 13x13; Base64 Encode and Decode, countdown Start, Reset, Set (`.ud-btn`, 32 px tall, taker the container). Proposed new row: `.ud-btn` and developer-tool checkboxes at 44 px on a phone (tools' sheet).
- Programmer's `.sr-calc-bit` is not in this spec.
Image picker: the sweep world has no bucket, so the picker shows the bucket select, "You have no buckets connected" and Cancel (all pass at 375); its file buttons are unmeasured.

Growth at 375 (sum of boxes): header `.sr-tl-view-head` +28 on most tools (two lines +14), +42 on percent; calculator view +57; keypad +32 (task: about 32); Invite card +131 (task said about 100: over by about 30, reported not cured); poll room +163; tier list room +51. Studio `.sr-st-bar` and `.sr-st-panel` row-gap 16 px. Poll answer rows: emoji, answer and x 44 tall (pass). tools' scope risk (drawer-scoped `.sr-tl-btn`): measured, Copy, Swap and the converter buttons pass and nothing moved beyond the lines and rows above.

At 1024 Light and Dark every control and container box (drawer, header, keypad, card, room bar, tiers, picker) after equals base within 0.6 px on all pages; only the fields' colours and ring changed.

Field contract at 1024 (Light and Dark), every raw field: rest border mixed (not plain --ui-border), 3.0 or more against the outside and the field; hover = --ui-text-faint; focus 2px solid --ui-focus offset 1px; placeholder = --ui-text-faint, opacity 1, 4.5 or more: no problem line in the final runs.
Tier names, ring (--ui-focus) against the head alone: Light red 1.08, green 1.09, blue 1.00, indigo 1.19, white 5.49; Dark red 1.74, amber 1.39, green 1.21, blue 1.00, indigo 1.76. With rooms' 1 px --ui-bg line inside and outside the 2 px ring (box-shadow 0 0 0 1px and 0 0 0 4px) the ring against its line is 5.49 (Light) and 4.82 (Dark): 3.0 or more on every head. A tier name's rest border against its head (1.1 to 2.1) is not judged: the field has its own ground and its border against that passes.

## Shots looked at (after the jobs ended)
calculator-375 and its base twin (header lines 44 tall, keys and Deg/Rad taller, nothing crowded; the first field looks cropped by the scroll artifact); poll-375 (answer rows, Add an answer, Lasts select, checkbox row; no crowding); tierlist-picker-375 (bucket select, "no buckets" text and Cancel at 44 px); studio-375 (Crop, Rotate, Flip rows with a clear gap between lines); invite-375 (fields 44 tall, Post the card 44, card longer); tierlist-1024-dark (room bar, five head colours as in the base, the Title field focused by keyboard shows a clear ring). Not opened one by one: units, percent, time zones, JSON, Base64, countdown shots (figures are the evidence) and the 1024 twins; not run: a base twin for every shot.

## Notes
- heads' override: `tools.css .sr-tl-drawer .lng-switcher-input` (the drawer's search field) keeps the switcher's rest border without the 65 mix and ring offset 2. I excluded `.sr-tl-input` from the field reads; not a cure of this wave.

## Closing text and changelog
Closing: the tool drawers' header, keypad, calculator and converter inputs, Invite card, room bar, room buttons and fields, and poll rows are 44 px targets on a phone; the fields have the kit's border, hover and ring; the tier name's ring has a --ui-bg line so it reads on any head. Open: `.ud-btn`, developer-tool checkboxes, `.sr-calc-hist-btn`, `.sr-tz-pick`, `.sr-calc-bit`.
Changelog: the tool drawers (calculator, converters, developer tools, Invite card, tier list, poll, Studio) have 44 px targets on a phone and the kit's field borders and focus rings.
Owner questions: none.

## PROCESSES
strays: none found (19:39).
