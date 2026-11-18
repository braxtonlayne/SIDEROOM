# fix-p105-loungeshots (pod lounge shots; D-1006, D-1007, D-1008)

Clock: started 15:30, last job ended about 15:50. Spec only: no app file changed. Merged forum, title, sheet (all three ALL READY), clean, no conflict.

## Done (commits)
- `e2e/tests/p105-lounge-shots.spec.ts` (new): forum toolbar 375 / 1024 Light / 1024 Dark; title at 1024 and 1280 with the member list open and closed, 375, the DM header, Friends and Message Requests; the phone search sheet at 375 Light and Dark (also scrolled to its end); the desktop search panel at 1024 Light. Own data `p105l-*` (a forum with a tag and two posts, a text channel with a long topic, a text channel with ten messages), removed in a `finally`. No `/src` import.
- Merges of fix/p105-forum, fix/p105-title, fix/p105-sheet.

## Not done
- eslint on the spec: not run (`pnpm` is not on the bash PATH here).

## BASE FIGURES (job 0035)
### for title (1024 Light, member list open, base)
Header 468 x 49; title box 0.5 wide (text 174, clientWidth 0); topic 3.5 wide; tools 380 wide at x 388. Closed: title 29.4 (cut), topic 218.4. 1280 open: title 30.8, topic 229; 1280 closed: title 59.7, topic 443.9. In the open 1024 case the title and topic share about 20 px; the title needs 174.
### for sheet (375, base)
Chips row 12,142 351 x 92 (two lines of 44 px chips), whole while the panel is unscrolled. Ancestors: `div.lng-search-chips` overflow visible; `section.lng-search-panel` 351 x 601 at y 142, **overflow-y auto (the scroller)**; `div.lng-search-wrap` 375 x 673 visible; `div.lng-search-sheet` 375 x 726 visible. Search box 12,90 351 x 44; count row 69 x 15 at y 256.5; Sort 153 x 44 at y 242; list top 321. **Scrolled to its end (scrollTop 456 of 1057) the chips row is at y -314, 0% visible**: scrolled away, not squeezed (the sheet builder's reading is right). Hit test 17 pass, 1 fail (the search input; the taker is its own `form.lng-search` wrapper), same in Dark.
### forum (375, base)
Tools 343 x 88; search 237.4 x 29; views 28 x 28; Sort 148 x 44; New Post 97.6 x 36; first tag y 194; first post y 227; hit 2 pass, 3 fail (search and both view buttons; taker `div.lng-forum-tools`).

## JOBS
| id | class | waited | ran | rc |
|---|---|---|---|---|
| 0026 | shoot base (first spec) | 99 s | 62 s | 0 |
| 0035 | shoot base (final spec) | 68 s | 67 s | 0 |
| 0042 | tsc (merged tree) | 212 s | 43 s | 0 |
| 0054 | build | 167 s | 20 s | 0 |
| 0058 | shoot after | 93 s | 61 s | 1 (one problem line, below) |

## Figures against the two lists (base to after, job 0058)
### forum (D-1006): PASS
- 375: search 237.4 x 29 to 237.4 x 44; views 28 x 28 to 44 x 44; tools 343 x 88 to 343 x 96 (two rows still); first tag y 194 to 202; first post y 227 to 235. Sort (148 x 44) and New Post (97.6 x 36) keep their size; they moved down 8 and 4 px because the row's items are centred in the taller row (base layout, not restyled). Search font unchanged. Hit test 3 fails to 0. No sideways scroll.
- 1024 Light and Dark: every box equal to the base; hit 5 pass 0 fail.
### title (D-1007): works where there is room; the 1024 members-open case is the tools (FAULT note below)
- 1024 members closed: title 29.4 (cut) to 170.4 whole; topic 218.4 to 76.6. 1280 open: title 30.8 to 170.4 whole, topic 229 to 88.6. 1280 closed: 59.7 to 170.4 whole, topic 443.9 to 332.6.
- **1024 members open: title 0.5 to 4 wide, still cut (170 in 4); topic 3.5 to 0.** The cure does what it should (topic to 0), but the header's room is gone: the tools take 380 of its 468 px. The cause is the tools' width.
- The spec's one problem line (the rc 1): `title title 1024-light members-open: title cut (170 in 4) and the base topic was already 3.5 wide: the cause is the tools (388,63 380x28)`. Raised on purpose as a finding, not a probe error.
- Header height 49 in all; every tool box equal in all five header configs; 375 header equal (title 215, topic 0); the DM, Friends and Message Requests title boxes equal; no sideways scroll.
### sheet (D-1008): PASS
- 375 Light and Dark, unscrolled: every box equal to the base. **Scrolled to the end: every chip at y 142, 100% visible (base 0%).** No chip fails the hit test; 17 pass, 1 fail = base.
- 1024 desktop panel: every box equal to the base (chips row, search, count, Sort 153 x 40, list, each chip).
- Cosmetic (as the sheet builder said): results run 8 px under the stuck chips while scrolling.

## FAULTS
None sent back. For the planner:
1. title (SEC-AUD-495): at 1024 with the member list open the channel title cannot be whole whatever the topic does: tools 380 of 468 px. A forum channel's header does the same (`forum-1024-light.png`: "p.."). Needs a decision on which tool collapses, outside this pod.
2. Spec limit: the DM header read finds a 712 x 666 box (`closest('header')` gave the page region). It is compared base to after and equal, but the real 49 px DM header box was not isolated. Not fixed.

## SHOTS (all opened after job 0058 ended; `visual-sweep-shots/p105-lounge`)
- `forum-375-light.png`: New Post and a 44 px search on one line, Sort and two 44 px view buttons below, tag chip, two posts. Base twin: 29 px search, 28 px views, all 8 px higher.
- `forum-1024-light.png`: toolbar as in the base; header title "p.." (member list open).
- `forum-1024-dark.png`: same in dark, readable.
- `title-1024-members-open-light.png`: header shows "#" and a sliver of title, topic gone; the cut remains (note 1).
- `title-1024-members-closed-light.png`: "#p105l-title-..." whole, separator, "p105l topi..." topic; base twin showed "p..." and a long topic.
- `title-1280-members-open-light.png`: title whole, topic cut to "p105l topic:..."; tools unchanged.
- `title-1280-members-closed-light.png`: title whole, long topic with ellipsis.
- `title-375-light.png`: compact header, whole title, no topic; as base.
- `sheet-375-light.png`, `sheet-375-dark.png`: "Search" sheet, five chips whole on two lines, results below; readable in Dark.
- `sheet-375-light-scrolled.png`, `sheet-375-dark-scrolled.png`: chips stay at the top while the results scroll; base twin `base-sheet-375-light-scrolled.png`: chips gone.
- `search-panel-1024-light.png`: desktop panel, one chips row, "10 RESULTS", Sort 153 x 40; as the base.
- Other `base-*` twins: opened only where named.

## Who imports my files
Nobody: the spec is an e2e file. Code under test: forum.css (lazy forum view), lounge.css (lounge chunk, lazy), not first paint.

## Checks (own tree, last commit)
e2e `tsc --noEmit`: exit 0. prettier --check on the spec: clean, exit 0. Web `tsc` job 0042 (merged): rc 0. Build job 0054: rc 0. eslint: not run.

## Closing text
- 491: at 375 the forum search is 44 px and each view button 44 x 44; the hit test went from 3 fails to 0; 1024 unchanged. Closes.
- 495: the topic gives way first; the title is whole at 1024 closed and 1280 open and closed. At 1024 with the list open it is still cut: the tools use 380 of 468 px. Partly closed; propose a new row for the tools.
- 471: scrolled, the phone search sheet's filter chips stay in view (sticky), whole in Light and Dark; the base's chips were scrolled out of the panel, not squeezed; desktop panel unchanged. Closes.

## Changelog sentence
The channel title now stays whole before its topic is cut, the forum toolbar's search and view buttons are 44 px targets on a phone, and the phone search sheet keeps its filter chips in view while results scroll.

## Owner questions (Fable)
- At 1024 with the member list open, which header tool collapses into a menu so the title can stay whole? Default: Inbox and Help first.

## PROCESSES
strays: none found
