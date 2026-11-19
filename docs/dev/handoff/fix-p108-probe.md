# Wave 108 handoff: probe (D-1025, the hit-test helper's edge rule)

## Clock
Started 20:33. Spec committed 20:35 (helper untouched), base job 20:34 to 20:39. Helper committed 20:40, after job 20:40 to 20:42. Handoff 20:44.

## Done (commits on `fix/p108-probe`)
- `D-1025 probe spec (helper untouched, base run pending)`: `e2e/tests/p108-probe-shots.spec.ts` (new).
- `D-1025 hit-test.ts: a point outside the viewport is not tested; a short control at the edge gets a span point (READY)`: `e2e/support/hit-test.ts` and its header comment.
- Rule as built: a test point outside the viewport (`y < 0`, `y >= innerHeight`, `x < 0`, `x >= innerWidth`) is not tested: the point is kept in `points` with `edge: 'top'|'bottom'|'left'|'right'`, `taker: null`, `relation: null`, `ok: true`. Every point with `edge === null` must be ok for `pass`. The centre outside the viewport, both the above and the below point outside, and a point inside the viewport that returns null are still `ProbeError` (`not-reached`). A control with `box.height < 44` and an above (below) point off the top (bottom) gets a third point `span` at y = 43 (y = innerHeight - 44); with `sides` the same on x for `box.width < 44`. `PointName` gains `'span'`; `PointResult.edge`, `HitResult.edge` (first non-null of above, below, left, right) and `ControlRow.edge` are new (null for a row with no result). New export: the type `EdgeName`. No signature, `MENU_ITEM_SELECTOR`, reach rule (D-983) or naming order (D-1012) changed. An `ok: true` edge point never shows up in the spec idiom `points.filter(p => !p.ok)`.
- Not covered by the literal decision: a control 44 px or taller whose centre is inside the viewport but whose top is off screen (box -12..32) has no span point and passes on its inside points. Say the word and the span point is added whenever any point is outside, whatever the height.

## Not done
No cure (task). Nothing outstanding.

## FINDERS (rule 13)
`git grep -n "hit-test"` in `apps/web/src packages scripts`: none. Every e2e spec that imports the helper compiles (e2e `tsc` over all of `e2e/`, exit 0). Specs that read `.points` or `.status` keep working: the only new points are `edge` ones (ok) and `span`; the older specs measured the bars with `reach: 'box'`, where no point is outside.

## JOBS
| id | class | submitted | started (waited) | ran | rc |
|---|---|---|---|---|---|
| 0248-probe-shoot | shoot base, PHASE=base (helper untouched) | 20:34:46 | 20:38:31 (225 s) | 32 s | 0 |
| 0254-probe-shoot | shoot base, PHASE=after (helper edited) | 20:40:38 | 20:42:04 (86 s) | 33 s | 0 |

Both `1 passed`, `problems: []`.

## Figures
Ordinary roots, 375 touch, base against after: channel (148 rows: 18 pass, 16 fail, 114 skipped), Server settings > Members (18 rows: 16 pass, 2 skipped), Library Browse main (14 rows, all pass). **Every row equal** (tag, role, type, name, path, status, reason, box, offset, inside points, takers). **No row touches the edge, so no row changed: the list of changed rows on ordinary roots is empty.** In the base the three roots had 0 `not-reached`.

Section bars, base (old): every link of the top bar was `not-reached` ("the point above (x, -1) is off the page", no box): Library 6, Cinema 6, Lounge Home 3. The Library's bottom nav (4 links, 52 tall) and the Lounge's rail, Friends and bottom bar rows passed.

Section bars, after (new): no `not-reached`, no null. All 15 top-bar links are `fail`, `edge: top`, with a taker named on the below point and the span point. Points: centre ok, above `edge-top`, below FAIL, span (y 43) FAIL.

| bar | links | box (y to y+h) | on-screen reach from the top edge | short of 44 | taker of below and span |
|---|---|---|---|---|---|
| Library (`.lib-sectionbar-link`) | The Lounge 66w, The Arcade 66, The Study 58, Shelves 45, Media 35, Reading 46 | 1 to 40 (h 39) | 40 | 4 px | `div[region]` in `div.lib-topbar` (a container) |
| Cinema (`.cin-sectionbar-link`) | the same six | 1 to 40 (h 39) | 40 | 4 px | `div[region]` in `div.cin-header` (a container) |
| Lounge Home (`.lng-sectionbar-link`) | The Lounge 68w, The Arcade 66, The Study 58 | 7 to 22 (h 15) | 22 | 22 px | The Lounge: `a.lng-guild` (the rail's Home link, a control); The Arcade and The Study: `div.lng-sidebar-top` (a container) |

The Library and Cinema bars need 4 more px below the link (a tap area `inset-block-end` of about -4px, or a link 4 px taller; the link starts at y 1). The taker is a container, so cure A.1 applies. The Lounge Home bar's links are 15 px tall in a bar about 28 px high: 22 px short; the area would run over the rail's first item (a control), so the cure is the bar growing to 44 px on a phone (A.2), which moves everything under it by about 16 px.

Other rows in the bars' roots (Library bottom nav 52, Lounge rail 48, Friends and Message Requests 44, bottom bar 56): pass, unchanged.

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p108-probe`, opened after the job ended; scrollWidth 375/375 in all six; the `base-` twins are on disk and were not opened)
- `probe-bar-library-375.png`: Library Browse (dark): the thin top bar (The Lounge, The Arcade, The Study | Shelves, Media, Reading; cut at the left, it scrolls inside itself), the header with search, bell and avatar, category chips, the sort select, the empty state "Nothing here yet", the 4-item bottom nav.
- `probe-bar-cinema-375.png`: Cinema: the same thin bar, the SIDE ROOM CINEMA header, "The cinema.", chips All and Drama, an "All rooms" select, Filters, one title card.
- `probe-bar-lounge-home-375.png`: Lounge Home (light): the 28 px bar at the very top with The Lounge, The Arcade, The Study in small type, the rail, "Find or start a conversation", Friends, Message Requests, empty direct messages, the bottom bar.
- `probe-channel-375.png`: #general: Juniper and Rowan messages, a thread card, a reaction pill, a started-a-thread line, the composer, the bottom bar, the section bar above.
- `probe-members-375.png`: Server settings > Members: the Sections picker, Joined before, Sort by with Prune, two member cards with Roles and "..." buttons, paging.
- `probe-library-browse-375.png`: Library Browse again (the root of this hit test was `main`).

## Who imports the files
`e2e/support/hit-test.ts`: e2e specs only; no app file, no chunk. The new spec is imported by nothing.

## Checks (own tree, last commit)
- e2e `npx tsc --noEmit -p tsconfig.json` from `e2e/`: 20:40, exit 0.
- `npx prettier --write` on both files reported "unchanged" after the edits: clean, 20:40.
- vitest, eslint, web tsc, i18n audit, doc-link check: not run (left to the commander, rule of the wave).

## The sentence AGENTS.md section 4 point 8 should gain
"A test point outside the viewport is not tested (the row records `edge`); a control passes when every point inside the viewport passes, and a control under 44 px at the screen's edge is also tested at the far side of a 44 px span measured from the edge (the `span` point), so a link at the top of a phone must reach 44 px on screen, not hide half its target off screen (D-1025)."

## Closing text for D-1025
The hit-test helper no longer answers `not-reached` for a control at the top or bottom edge of the screen: points outside the viewport are skipped (`edge`), and a short control there is also tested at 43 px from the edge. Proven: three ordinary roots (channel, Members, Library Browse) have equal rows before and after; the section bars of the Library, the Cinema and Lounge Home, all `not-reached` before, now give every link a verdict.

## Proposed new row (for the next wave; nothing cured here)
The section bars' top links fail the 44 px target on a phone under D-1025, 15 links. Library and Cinema (`.lib-sectionbar-link` in `library.css`, `.cin-sectionbar-link` in `cinema.css`): 39 px tall at y 1 to 40, 4 px short, taker a container, cure A.1. Lounge Home (`.lng-sectionbar-link` in `lounge.css`): 15 px tall at y 7 to 22, 22 px short, takers the rail's Home link and `.lng-sidebar-top`, cure A.2 (the bar grows to 44 px on a phone and the content under it moves by about 16 px). The Arcade and Study bars (`.arc-roombar-link`, `.stu-bar-link`) are not measured here: add them to the census with the new rule.

## Owner questions
None.

## PROCESSES
strays: none found (20:43). No job of mine queued or running.
