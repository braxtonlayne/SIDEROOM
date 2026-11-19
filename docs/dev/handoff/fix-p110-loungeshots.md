# Wave 110 handoff: loungeshots (shots builder of pod lounge; D-1022, D-1035; SEC-AUD-542, 545 to 549, 552)

## Clock times
Started 23:13; spec compiled 23:17; base job 0329 ran 23:19 (a spec bug, below); runner stopped 23:20 to 23:54 for mounts's cargo; merges 23:21; base 0333 ran 23:54; build 0335 23:55; after 0339 23:58; looked at shots and wrote this 23:59.

## Done (commits on fix/p110-loungeshots)
- `e2e/tests/p110-lounge-shots.spec.ts` (new; 4 commits: spec, first tap on the time, reload before the spoiler tap, file size line box). Prettier clean, e2e `tsc` clean (23:17 and 23:20, exit 0).
- Merged `fix/p110-msgs` (0c7fa842) and `fix/p110-lounge` (2cb92bff) into my branch, no conflicts. No app file of mine.

## Not done
No FAULTS cure round needed. Not run (left to the commander): vitest, eslint, web tsc.

## FINDERS
Pod code builders carry theirs. Mine: `git grep` is not needed (spec only).

## JOBS
| id | class | waited | ran | rc |
|---|---|---|---|---|
| 0329 | shoot base | 9 s | 94 s | 0 (spoiler step failed: my first tap hit the spoiler itself; fixed) |
| 0333 | shoot base | 1997 s (runner stopped) | 91 s | 0 |
| 0335 | build | 79 s | 24 s | 0 |
| 0339 | shoot after | 5 s | 81 s | 0 |
Both phases on the same spec: figures in `visual-sweep-shots/p110-lounge/figures-base.json` and `figures-after.json`.

## Figures (375 touch unless noted), base then after
- Toolbar after a tap on a message's text: 1 shown, then 0 (D-1035 works). Keyboard focus at 375: shown, shown. 1024 mouse hover: shown, same box 286x34 at 478,315 both; Reply opens the reply bar, both.
- Spoiler tap: revealed in base too (topmost at its centre is the spoiler, not the toolbar, in my flow): the base fault did not reproduce at the spoiler's centre; after: revealed.
- Long-press menu: 13 of 13 pass after (base run died before this step in 0329; in 0333 it passed too).
- Friends (All, Pending): fails 1 then 0, 2 then 0; row height 56 both; at 1024 all pass both.
- Forward: fields 185 wide in a 303 line (375) and 360 line (1024), then 303 and 360 (fill the line); placeholder of field 0 cut in base, whole after; rows 28 then 44 px (the 48 px one stays 48 at base, now 48 as well); hit test 11 fails then 0; list maxHeight 240 and still scrolls (scrollTop set 40 reads 40, both). Emoji picker search 185x44 / 185x32 unchanged.
- Embed title: display inline, 176x86 (4 lines) then flex, min-block-size 44, 179x88; hit fail then pass; embed card 307x224.3 unchanged, description box unchanged. 1024: unchanged.
- File card: name link and download icon both fail then both pass. Icon 20x28 then 28x28 (at 1024 too: 20 then 28, the name gives up 8 px: 272 then 264.4, meta and size line the same). Card 307x62 unchanged. msgs's question: `a.lng-file-name` box is now 225.4x44 at y 352 (card top 354, size line y 385 to 407): it overlaps the size line by 11 px and reaches 2 px above the card; the size line is text, not a control, and the hit test passes for both controls.
- Viewer bar: items on 4 rows then 3; "Open original", "Copy link", "Open in studio" 2, 2 and 3 lines each then 1 line each; Download, Open original, Copy link failed (taker the bar), now 4 of 4 enabled controls pass; bar 303x68 then 303x124; dialog 334 then 390 tall; stage 303x180 both. 1024 unchanged.
- Problem lines of the after run (3, none a fault): `375|tb.afterTap gone after` (the toolbar is display none, as intended); `1024|fc.meta` and `1024|fc.size` 272 to 264.4 wide (the 8 px the download icon takes back, allowed by the task).
- No sideways scroll in any shot.

## Shots, opened after the jobs (folder `visual-sweep-shots/p110-lounge`, base twins `base-`)
- `tap-message-375.png`: the 14-reaction message after a tap: row tinted, no toolbar; base twin shows the toolbar strip lying half over the date line above and the author name. 
- `hover-message-1024.png`: toolbar on hover at the same place, ten buttons; unchanged.
- `forward-375.png`: both fields fill the dialog line, placeholder whole, destination rows airier, Send disabled; fine.
- `embed-375.png`: title link block, card unchanged.
- `filecard-375.png`: name link ellipsised, 1.1 MB below, download icon square at the right; nothing overlapping visibly.
- `viewer-375.png`: bar wraps in three lines: prev, 1 / 1, next and Download; Open original, Copy link; Open in studio; no label breaks.
- `friends-pending-375.png`: Tess row with check and cross, row unchanged.
- Not opened one by one: `spoiler-tap-375`, `friends-all-375`, `forward-1024`, `embed-1024`, `filecard-1024`, `viewer-1024`, friends 1024 (figures equal base; see the numbers above).

## Faults and rows
FAULTS: none. Proposed row: none.

## Commander's command
bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p110-integ/apps/web/dist --env SR_P110_LOUNGE_ON=1 --env SR_P110_LOUNGE_PHASE=after --env SR_P110_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p110-lounge -- tests/p110-lounge-shots.spec.ts

## Owner questions
None. Note for Fable: the tapped row keeps its hover background tint on a phone (a separate rule from the toolbar).

## PROCESSES
strays: none found
