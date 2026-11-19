# fix-p108-toolshots (wave 108, pod tools, shots builder; D-1022, D-1024)

## Clock times
Started 20:33; spec 20:35; base jobs 20:35 to 20:40; merge of fix/p108-tools (5157bb09) and build 20:49; after jobs to about 20:58; handoff 21:00.

## Done (commits)
- `e2e/tests/p108-tool-shots.spec.ts`: 10 pages (switcher, calculator, programmer, units, percent, time zones; JSON, Base64, countdown, Invite card), two parts. Switcher read after Tab (rest), hover, Shift+Tab (focus); its box must equal base at every width. New notes: bit grid height ratio, examples, tz grid, ud rows.
- Merge of fix/p108-tools: no conflict (41ac9824). Build 0261 on it.
- Commands, whole (part 2: PART=2): `bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p108-integ/apps/web/dist --env SR_P108_TOOL_ON=1 --env SR_P108_TOOL_PHASE=after --env SR_P108_TOOL_PART=1 --env SR_P108_TOOL_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p108-toolshots -- tests/p108-tool-shots.spec.ts`

## Not done
- The other `.ud-btn` pages (Hash, Regex, URL, Dice, Coin, Loot, Wheel, Notes, timers) are NOT measured: the spec covers only JSON, Base64, countdown. A grown button in a row there is unmeasured; propose them as a next-wave page set.
- Left to the commander: vitest, eslint, web tsc, i18n audit.

## FINDERS
No app file changed: none.

## JOBS
Base 0246 p1 (rc 0, ran 1.9 min), 0247 p2 (rc 0, 1.3 min). Build 0261 (rc 0). After 0264 p1 (rc 0, 1.9 min), 0265 p2 (rc 0, 1.2 min). Each shoot waited about 1-5 min.

## Figures, 375 hit fails, base -> after
switcher 0 -> 0; calculator 0 -> 0; programmer 32 -> 0; units 0 -> 0; percent 5 -> 0; time zones 24 -> 0; JSON 1 -> 0; Base64 3 -> 0; countdown 3 -> 0; Invite card 0 -> 0. At 1024 (Light, Dark): 0 fails, and every row, container and field box equals base. No sideways scroll anywhere. Problem lines: 0 in both parts. FAULTS: none.
Measured growth at 375: bit grid 160 -> 224 px (x1.4, under 2x); percent examples list 176 -> 236 px (the 11rem cap is lifted); time zone grid 64.5 -> 89.5; Base64 button row 110 -> 184 (the row block, checkbox label 44); countdown `.ud-row` 95 -> 107, `.ud-timer` 267 -> 279; JSON `.ud-row` unchanged. Nothing that passed in base fails after.
Switcher (all three configs): rest border mixed (Light rgb(134,138,145), 3.47 against outside; Dark 3.42), hover = --ui-text-faint, ring 2px offset 1px; box unchanged (303x44 at 375, 520x44 at 1024). Base had plain --ui-border and offset 2.

## Shots looked at (after the jobs ended)
programmer-375 (four rows of eight bits, 44 tall cells, clear gaps, nothing crowded); percent-375 (five examples as full-width 44 rows, list taller, form below; first row cut by the scroll artifact of the hit test, as in wave 107); timezone-375 (header, copy buttons, add-person form fine; the hour cells are in a scrolled strip not in view); base64-375 (Encode/Decode 44 tall, URL-safe row with a centred box, fine); switcher-1024-light (search field with a clear blue ring, results below). Not opened one by one: the other 375 shots, the 1024 twins and base twins (figures are the evidence).

## Checks
Spec: e2e tsc rc 0 and prettier before commit (20:35). vitest, eslint, web tsc: not run.

## Closing text / changelog / owner questions
Closing: the Drawer's search field has the kit's border and ring; Base64, countdown, percent examples, programmer bits and time zone hours are 44 px targets on a phone. Changelog: tool drawer small controls are 44 px on a phone. Owner questions: none.

## PROCESSES
strays: only the harness shells; none of mine.
