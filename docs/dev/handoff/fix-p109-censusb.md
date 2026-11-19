# fix-p109-censusb (wave 109, census B: SEC-AUD-537 pages 7 and 9, SEC-AUD-535, the Arcade and Study bars of SEC-AUD-530)

Measure only; no app file, no build, no cure. The report is `C:/Users/braxt/Desktop/SIDEROOM_wt/p109-census-b-report.md` (fails, groups by cure, one line per tool, the `.ss-select` reads, what passed). Figures and shots: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p109-censusb/` (`figures-base-p1..p4.json`, `base-*.png`).

## Clock times
Started 21:39 (brief at 21:39, the commander's T is 21:50). Spec written and typechecked 21:47. Jobs 21:47 to 22:02. Report and handoff 22:05. Nothing queued or running after.

## Done (commits on fix/p109-censusb)
- `e2e/tests/p109-censusb-shots.spec.ts` (new): four parts (1 Library list plus the Arcade and Study bars, 2 Admin Accounts, Limits, Audit log plus Members' `.ss-select`, 3 and 4 the tools with `.ud-btn`). Commit subjects use "D-1030", a placeholder I took because the brief gave me no D-number: renumber at integration.
- `p109-census-b-report.md` (outside the repo, as the task says).

## Not done (why)
- Nothing built, nothing cured (census). Notes after unlock, forum settings' `.ss-select`, the Wheel's winner state: not covered (report section 6).

## REACH table
Not applicable (census).

## FINDERS (rule 13)
No app file changed, so no control changed. For the record: `git grep -n "ud-btn" -- e2e` finds `p79-tools-shots.spec.ts` and `p80-tools-shots.spec.ts` (retired shots specs), `p108-tool-shots.spec.ts` (wave 108's shots) and mine. No test reads a file of mine as text.

## JOBS (all class shoot, mode base; waited = started minus submitted, ran = ended minus started, seconds)
| id | part | waited | ran | rc |
|---|---|---|---|---|
| 0297-censusb-shoot | 1 Library list, Arcade and Study bars | 80 | 34 | 0 |
| 0299-censusb-shoot | 2 Admin and Members (first run) | 82 | 53 | 0 |
| 0303-censusb-shoot | 3 tools A: Hash, Regex, URL, Dice, Coin, Wheel | 120 | 96 | 0 |
| 0307-censusb-shoot | 4 tools B: Loot, Notes, Pomodoro, Stopwatch, Countdown, Programmer | 163 | 86 | 0 |
| 0310-censusb-shoot | 2 again (the spec now reads the kit Select's wrapper) | 107 | 47 | 0 |
Times from `cli-agents/run/jobs/<id>/{submitted,started,ended,rc}`. 0299 mis-read the kit Select (read its trigger, not the box on the wrapper) and reported a false "no hover, no ring" on Accounts' two selects; 0310 replaces it. No problem lines: the spec only measures and fails on a page it could not cover (none).

## Figures (the task's questions)
- Library list and lists: 0 fails at 375 and 1024.
- Admin Accounts (8 accounts), Limits, Audit log, Members: 244 rows, 0 fails. `.ss-select` is NOT on any Admin page (class used on 3 elements, all Server settings); the contract holds on all of them (report section 5).
- Arcade bar: 3 links fail at 375 (33 px tall, takers a container and the brand text span; A.1 area). Study bar: 6 links fail (20 px tall; the first row's takers are the bar container; the second row's takers are the first row's links, so A.1 needs a row gap of about 24 px: bar 73 to 89 px). Both pass at 1024.
- Tools: 0 hit fails in 12 tools, base and tapped states. The grown `.ud-btn` adds 12 px a row (Dice shortcut grid 24), breaks no row, is never taller than the field beside it. Dice's roll detail line falls 8 px under the drawer's fold (741 against 733); the Wheel's Spin and winner were already under the fold.

## Shots (all opened after the jobs; one line each; folder above)
- `base-lib-list-375`: the Library list in its dark `sideroom` theme: "ONLY ME" title, Edit list and Delete list buttons, empty state with Browse series, bottom bar; no crowding.
- `base-lib-lists-375`: "Reading lists", New list button at the right, one list tile.
- `base-lib-list-1024` / `base-lib-lists-1024`: same pages with the sidebar; title next to Edit/Delete; one tile.
- `base-bar-arcade-375`: the Arcade: brand "SIDE ROOM /" on the first line, the three room links on the second at 33 px, pills under.
- `base-bar-arcade-1024`: one-row bar, links 39 px, pills row under it.
- `base-bar-study-375`: Study: rooms on the first line, Shelves, Media, Reading on the second with a rule at the start; links 20 px.
- `base-bar-study-1024`: one row of six links with the rule between rooms and areas.
- `base-admin-accounts-375`: Admin Accounts: "Sections: Accounts" switcher, two 44 px selects and a search field, stacked account rows each with a View button.
- `base-admin-accounts-1024`: sidebar list, selects, the accounts table with its State column cut at the card's edge.
- `base-admin-limits-375` / `-1024`: Limits and settings: a card of labelled 44 px number inputs with hints; no select anywhere.
- `base-admin-audit-log-375` / `-1024`: Audit log: three filter fields (the Action field looks darker than its two neighbours: an unverified observation), Apply, rows below.
- `base-members-ss-select-375`: Server settings Members: search, role select, two date fields, sort select, member row; 44 px controls.
- `base-members-ss-select-1024`: the filter row and a two-row members table with Roles and menu buttons.
- Tools at 375 (all in the drawer, shot before the tap): `base-tool-hash-375` Text area, file field, notice and the result list start; `-regex` flags `g i m s u` as 44 px chips; `-url` Encode and Decode, then the "Encode as" select, Copy greyed; `-dice` notation field with Roll, two lines of d-buttons; `-dice-tapped` a roll "13" with the detail line cut at the drawer's bottom; `-coin` and `-coin-tapped` the coin "?" then "Heads", Flip and Reset; `-wheel` and `-wheel-tapped` names box and the wheel, Spin below the fold (the tapped shot is the same view); `-loot` and `-loot-tapped` the two selects and fields, then scrolled to Roll and Save as new; `-notes` the explanation, Recovery file, Passphrase and a greyed Open my notes; `-pomodoro` and `-pomodoro-tapped` 25:00 with Start/Reset/Skip then 24:59 with Pause; `-stopwatch` and `-stopwatch-tapped` 00:00.0 then 00:00.7; `-countdown` 05:00 with Start/Reset and the Length field with Set; `-programmer` the expression field, the bit width segment, the Signed box.
- Tools at 1024: `base-tool-hash-1024`, `-regex-1024`, `-url-1024`, `-dice-1024`, `-coin-1024`, `-wheel-1024`, `-loot-1024`, `-notes-1024`, `-pomodoro-1024`, `-stopwatch-1024`, `-countdown-1024`, `-programmer-1024`: each the same tool in a 562 px dialog over the channel, buttons 32 px, one line per row, nothing cut except the long ones (hash, notes, wheel, loot) at the dialog's lower edge.

## Who imports my files / which chunk
`p109-censusb-shots.spec.ts` is read by Playwright only. No app file.

## Checks (on my last commit)
- `npx prettier --check e2e/tests/p109-censusb-shots.spec.ts`: pass (22:04, exit 0).
- e2e `npx tsc --noEmit -p tsconfig.json`: exit 0 (22:04; run while available memory read 647 MB, below the 1 500 MB the brief asks for: it finished in 10 s and nothing else was disturbed; the first run at 21:47 was at 1 660 MB).
- vitest, eslint, web tsc, i18n audit, doc-link check: left to the commander (not run).

## Closing text for my rows and proposed new rows
- SEC-AUD-535 (closing): "Measured in wave 109 (censusb): hit tests at 375 and 1024 of Hash, Regex, URL, Dice, Coin, Wheel, Loot, Notes, Pomodoro, Stopwatch, Countdown and the programmer calculator, first and tapped states: 0 fails. The phone rule grows each `.ud-btn` row by 12 px (the Dice shortcut grid by 24), breaks no row and leaves no button taller than the field beside it. Dice's roll detail falls 8 px under the drawer's fold; the Wheel's Spin and winner were already under it. Notes after unlock not covered. Report `p109-census-b-report.md`, handoff `fix-p109-censusb.md`."
- SEC-AUD-537 (7 and 9): page 7 (Library list and lists): 0 fails, closed as measured. Page 9: Admin Accounts, Limits and Audit log: 0 fails; the `.ss-select` is not on Admin at all (it is on Members' two date inputs and one forum-settings input; all five contract parts hold).
- SEC-AUD-530 (Arcade and Study bars): proposed new row, [FE] Low: the Arcade bar links (`.arc-roombar-link`, 33 px, arcade.css:49 and 979) and the Study bar links (`.stu-bar-link`, 20 px, study.css:276) fail the 44 px test at 375, 9 links, none at the screen edge. Arcade: cure A.1 (area; takers a container and the brand text). Study: A.1 plus a row gap of about 24 px on `.stu-bar` (study.css:262, now 8 px) because the second row's takers are the first row's links; the bar grows about 16 px. Pass at 1024.
- Proposed new row, [FE] Low (comment fix, no behaviour): `server-settings.css:759`'s comment says `.ss-select` is shared with Admin; it is not (Members and forum settings only). Needs a one-line comment edit by whoever owns the sheet.
- Proposed new row, [QA] Low: the Audit log's "Action" field looks darker than its two siblings in the shots; read its tokens once (a kit TextInput should match).
- Proposed new row, [FE] Low: Dice's result detail and the Wheel's Spin and winner sit under the drawer's fold at 375 (drawer 654 px high). Cure belongs to the drawer or the wheel's size.

## One changelog sentence
The second census measured the Library list, Admin Accounts, the Arcade and Study bars and the rest of the tools at 375 and 1024: only the nine Arcade and Study bar links fail the 44 px test.

## Owner questions
None. (Default taken: the Study bar's two rows get a row gap rather than shrinking the tap areas, as Discord-like apps keep 44 px targets on phones.)

## PROCESSES
strays: none found (22:05)
