# fix/p108-loungeshots (wave 108, shots builder of pod lounge: lounge + shell)

## Clock
Started 20:33; spec 20:36; base round 1 20:39-20:41; spec fixes (Friends entry, New Post button, friends header selectors) and base part 2 re-run 20:46-20:50; merged lounge 20:50 (build cancelled as stale), merged shell 20:52 (6769f619); build 0267 20:57; after 0273/0274 21:03-21:06; shots opened 21:06; handoff 21:07.

## Done
- `e2e/tests/p108-lounge-shots.spec.ts` (new, from wave 107's). Part 1: text channel, inline edit (textarea emptied, then cancel and save tapped at 375: both close the edit), thread, at 375, 1024 Light, 1024 Dark (fields). Part 2: Home list by URL and from the tab bar, Friends by URL and from the tab bar, Threads popover, forum New Post, at 375 and 1024, fields in Light and Dark. No app file changed. Merged fix/p108-lounge (42675b57) and fix/p108-shell (81bd3b56).
- Entry paths: the app has no friends URL. "By URL" = /channels/@me then tap Friends. "From the tab bar" = /channels/@me, Friends, tab bar Home (back to the Home list, measured as home-list-tab), Friends again. From a channel the tab bar's Home shows the server's channel list (not the Home list), so that path is not used.

## Not done
- A wrapped reactions line (row-gap 20): not measured (not in the seed).
- Dark shots: fields read only, no png. 1024 shots other than threads-popover, friends-url and thread: figures only (0 moved), not opened one by one.

## Figures (375 touch Light, base -> after; part 1 jobs 0249 -> 0273, part 2 0256/0259 -> 0274)
- Named controls: part 1 base fail 7 (2 reaction pills, inline cancel and save each counted twice, Jump to original), after 0. Part 2 base fail 4 (3 Threads tabs, Threads search), after 0. Every named control passes at 375 after; at 1024 (box reach) 0 fail base and after.
- All rows: channel 16 -> 13, inline edit 4 -> 2, thread 2 -> 1, part 2 pages 4 -> 0. What remains is accepted (names, spoiler, mention, md links, embed title: 1 thread-page name) plus the inline edit's own textarea "Edit message" 307x38 (taker the time above and the hint below): failed in base too, not named in any task (proposed row).
- lounge 519: message with reactions 125.9 -> 136.9 (+11, limit 12). Pills 24 px unchanged, thread chip unchanged, reactions row box unchanged (margin). The pills pass (point below no longer taken). Wrapped line: not measured.
- lounge 520: inline hint line 16.5 -> 44, its buttons 44; the edited message 89.5 -> 117 (+27.5); thread page: the system message 70 -> 92 (+22), "Jump to original" passes. In the channel page the "started a thread" row (it holds a Jump to original line too) 48 -> 70 (+22): my spec allowed 0 there so it printed a PROBLEM line; it is the cure's own change, within D-990 ("Jump to original line to 44"), not a fault.
- lounge 521: request link passes 217x44 at 375 by URL and from the tab bar, in the BASE too (wave 107's 217x44 rule already holds in this tree: .lng-dm-empty box 303x84 base and after, unchanged). 1024: 216x40, unchanged. The failing 217x20 of listshots was not reproduced.
- lounge 522: still reads the text colour. The PROBLEM lines: inline-edit placeholder rgb(49,51,56) light / rgb(219,222,225) dark, not --ui-text-faint, at 375, 1024 Light and 1024 Dark. InlineEdit.tsx has no placeholder attribute, so this is a pseudo-element read of a field that has none; the commander said report, not fault. A sheet cannot show it. Nothing visible changes.
- shell Threads popover: tabs 32 -> 44, search 29 -> 44, popover grows (the tabs bar 32 -> 44 and the search +15), all pass. 1024 equal. Threads search fields (Light and Dark): rest border, hover, 2 px ring, placeholder faint: 0 problems.
- shell New Post textarea: 303x138 at 375 (already over 44), 520x138 at 1024; unchanged base and after; passes. No placeholder attribute: accepted. Field reads Light and Dark: 0 problems.
- shell Friends header: 101 -> 105 (+4), body 625 -> 621 (the page is a fixed viewport, so it is 4 px shorter; tabs and list 4 lower). Same by URL and from the tab bar. "Back to channels" and "New Group DM" (28x28) PASS after, and also PASSED in the base here: I could not reproduce the section-bar taker that staffshots saw, so I cannot confirm the padding is what cured anything; it is +4 px of header with no measured need. The spec's "shrank" line on friends-body (2 PROBLEM lines) is that 4 px, expected.
- Request link / Friends / popover / forum / thread: no sideways scroll in any shot (scrollWidth = clientWidth).
- No row that passed in base failed after; no tap area took another control's centre.

## FAULTS
None to send. Notes: (1) inline-edit placeholder (above). (2) Proposed new row: inline edit textarea 307x38 at 375 fails the 44 px test in base and after (not named in lounge's task).

## Shots (opened after the jobs; base twin where one exists)
channel-375: pill row with more room under it, the "started a thread" row lower, rest as base. inline-edit-empty-375: emptied field with focus ring (no placeholder text, as there is no attribute), cancel/save line taller and clear of the textarea. thread-375: "Jump to original" line taller, nothing crowded. threads-popover-375: tabs and search taller, no crowding, popover fits (base twin). friends-url-375, friends-tab-375: header same to the eye, tabs a little lower, identical to each other (base twin identical to the eye). new-post-375: dialog as base, textarea unchanged. home-list-url-375, home-list-tab-375: identical to base, request link visible. threads-popover-1024-light, friends-url-1024-light, thread-1024-light: nothing moved (figures equal base). Not opened one by one: channel-1024-light, inline-edit-empty-1024-light, home-list-url-1024-light, new-post-1024-light (figures equal base).

## FINDERS
Spec only: none.

## JOBS (class; waited s / ran s; rc)
0249 base p1 135/51; 0250 base p2 189/95 (Friends and New Post entries wrong: fixed in the spec); 0256 base p2 166/61; 0259 base p2 128/62 (final selectors); 0262 build cancelled (stale after the shell merge); 0267 build 312/16; 0273 after p1 389/52; 0274 after p2 443/62. All rc 0 except 0262.
Commands (whole): bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p108-integ/apps/web/dist --env SR_P108_LOUNGE_ON=1 --env SR_P108_LOUNGE_PHASE=after --env SR_P108_LOUNGE_PART=<1|2> --env SR_P108_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p108-loungeshots -- tests/p108-lounge-shots.spec.ts

## Checks
e2e tsc exit 0 and prettier --check clean on the spec, 20:46 (last spec edit). eslint, vitest, web tsc: not run (left to the commander).

## Changelog / owner questions
Lounge: reaction pills, the inline edit's cancel and save, "Jump to original", Threads tabs and search have 44 px targets on a phone. Owner questions: none.

## PROCESSES
strays: none found
