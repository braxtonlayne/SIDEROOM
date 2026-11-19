# Wave 109 handoff: lounge (D-1022 + D-1025; SEC-AUD-530 Lounge Home, 534)

## Clock times
Started 21:37 (machine clock), spec 21:39, base job 21:40, build 21:41, after job 21:42, handoff 21:43.

## Done (commits on fix/p109-lounge)
- `D-1022: wave 109 lounge shots spec`: `e2e/tests/p109-lounge-shots.spec.ts` (new).
- `D-1022: Lounge Home section bar and inline-edit textarea reach 44px on a phone`: `apps/web/src/lounge/lounge.css` only. Rules (each new, directly after the rule it amends):
  1. After the `@media (max-width: 768px)` block of `.lng-sectionbar*`: `.lng-root[data-mobile] .lng-sectionbar { height: 44px }` and `.lng-root[data-mobile] .lng-sectionbar-link { display: inline-flex; align-items: center; min-block-size: 44px }` (A.2; the bar's content box was 28 px and its border is extra, so the bar is 45 px and the link 44 px from y 0: the D-1025 span point at 43 lands on the link).
  2. After `.lng-textarea.lng-inline-textarea::placeholder`: `.lng-root[data-mobile] .lng-textarea.lng-inline-textarea { min-block-size: 44px }`.
  The phone rule is `.lng-root[data-mobile]`, the one the sheet already uses.

## Not done
Pile (b) of the channel page is empty, so no cure there; no msg.css, profile.css or markdown.css rule named or touched.

## Step 1 (confirmed at base, job 0287): the three piles
- **Lounge Home bar:** 3 links, 15 px tall at y 6.5 in a 29 px bar; takers: The Lounge `a.lng-guild` (rail Home link); The Arcade and The Study `div.lng-sidebar-top` and `button.lng-find`. Confirmed as the task says.
- **Inline edit:** textarea 307 x 38 at y 409; takers the time above (`time.lng-msg-time`) and the container below. Confirmed.
- **Channel page (world channel), 148 rows: 21 pass, 13 fail, 114 skipped. Pile a (inline in running text, accepted, counted): 13. Pile b (standalone): 0. Pile c (the probe's own): 0.** The 13: author name buttons "Rowan (owner)" 116.5 x 22 (5) and "Juniper" 58.8 x 22 (3) in `header.lng-msg-head`; mention "@Juniper" 77.7 x 24; spoiler span 49.9 x 20; links in message text `a.sr-md-link` (3: example.com 199 x 20, sweep-preview 216 x 42, peertube 306 x 42); the embed title link `div.lng-embed-title a` 176 x 86 (see the judgement call below).
  So "Rowan (owner)" and "Juniper" sit in the message head (an author's name), not in a member row, thread strip or welcome card.
- **New seed's channel (`extras`), 58 rows: 26 pass, 2 fail, 30 skipped:** both pile a: the "Rowan (owner)" name button and the spoiler span (247.8 x 42, a spoiler in the sentence). The 14 reaction pills all pass (wave 108's cure).

## REACH table (D-990)
| element | base (375) | after (375) | at 1024/1280 |
|---|---|---|---|
| `.lng-sectionbar` (Lounge Home and every Lounge page with the bar, channel included) | 29 tall | 45 tall (28 -> 44, plus 1 border) | 29, unchanged |
| `.lng-sectionbar-link` x3 | 15 tall at y 6.5 | 44 tall at y 0 | 15 tall, unchanged |
| rail top, sidebar top, left col top | y 29 | y 45, all three equal | y 53, unchanged |
| rail / sidebar / left col height | 726 / 666 / 726 | 710 / 650 / 710 | unchanged |
| rail / sidebar width | 72 / 303 | 72 / 303 | 72 / 240 |
| find box | y 39, 44 tall | y 55, 44 tall | unchanged |
| inline edit textarea | 307 x 38 | 307 x 44 | none (phone rule only) |
| inline edit foot line | 307 x 44 | 307 x 44 | |
| message row with the edit open | 117 | 123 | |

Dependants (`git grep -n "lng-sectionbar\|lng-inline" -- apps/web/src e2e`): `.lng-sectionbar*` is used by `Chrome.tsx` only (rendered once, in `LoungeShell.tsx`, so every Lounge page on a phone shows the taller bar: the channel page measured 29 -> 45 too, as the task allows); `.lng-inline-textarea` by `InlineEdit.tsx` only. Moved: everything under the bar by 16 px at a phone (sidebar, rail, find, channel header). Not moved: anything at 1024, the tab bar (y 755, 57 tall), the rail's and the sidebar's widths.

## Figures against the lists (job 0289 against 0287)
- Must not change: the 1024 Home figures (bar, links, rail, sidebar, left col, find) and the channel page's 1024 bar, rail, sidebar and main are equal (the spec compared them). No sideways scroll in the 5 shots (375/375, 1024/1024). Rows that passed in base all pass (the spec compared every row by index and path): `problems: []`. Wave 108's cures hold: 14 reaction pills pass, inline cancel and save pass.
- Fails at 375: Home chrome 3 -> 0 (15 of 15 rows pass), inline edit 1 -> 0 (3 of 3 pass), channel 13 -> 13 (all pile a), extras channel 2 -> 2 (pile a). Row counts equal in all four hit tests.

## FINDERS (rule 13)
`git grep -n "lng-sectionbar\|lng-inline" -- apps/web/src e2e`: `apps/web/src/lounge/__tests__/sectionBarVoice.test.tsx` reads `lounge.css` as text (the first `.lng-sectionbar {`, the 768 media block after `.lng-sectionbar-voice-short {`, `.lng-sectionbar-nav { gap: 10px`, no `:has(.lng-sectionbar-voice)`): my rules come after that block and contain none of those strings, so it should hold; not run (left to the commander). E2E: `p107-lounge-shots`, `p108-lounge-shots` (retired shots specs, read `.lng-inline-textarea` by class, not edited), `p61-web-shots.spec.ts:113` (reads the `.lng-inline-edit` error text only). `git grep -n "lounge.css"`: `LoungeShell.tsx`, `settings/server/ui.tsx`, `settings/user/parts.tsx` import it (settings pages have no `.lng-root[data-mobile]` section bar, no effect). No control replaced or moved; no test drives a changed control.

## JOBS
| id | class | waited | ran | rc |
|---|---|---|---|---|
| 0287-lounge-shoot | shoot base | 26 s | 38 s | 0 |
| 0288-lounge-build | build | 3 s | 25 s | 0 |
| 0289-lounge-shoot | shoot after | 1 s | 41 s | 0 |
(submitted/started/ended epochs: 1795055949/55975/56013; 56056/56059/56084; 56089/56090/56131.)

## Shots (all opened after the jobs ended; folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p109-lounge`)
- `home-375.png`: Lounge Home: the bar now 44 px with The Lounge, The Arcade, The Study centred; rail and find box start together below it; Friends, Message Requests, the group DM, user panel and tab bar intact, no crowding. Base twin `base-home-375.png` (viewed): the same, the bar 16 px shorter.
- `inline-edit-375.png`: a room with a message being edited: a blue-ringed textarea 44 px tall, the "escape to cancel, enter to save" line below, the bar 44 px; nothing cut.
- `channel-375.png`: #general with the same 44 px bar, messages, thread card, reaction pill, composer; nothing overlapping.
- `extras-channel-375.png`: the new seed's channel: 14 reaction pills in three rows, spoiler bars, composer; fine.
- `home-1024.png`: Lounge Light at 1024: the bar still thin (29 px) with SIDE ROOM / The Lounge links, title strip, rail, sidebar, Friends page; unchanged.

## Who imports my files
`lounge.css`: `LoungeShell.tsx` (the Lounge chunk, lazy after sign-in), `settings/server/ui.tsx` and `settings/user/parts.tsx` (settings chunks); not first paint, not a public page. The spec: nothing.

## Checks (own tree, last commit)
- `npx prettier --check` on `lounge.css` and the spec: 21:40, exit 0.
- `node scripts/check-css-tokens.mjs`: 21:40, exit 0.
- e2e `npx tsc --noEmit -p tsconfig.json`: 21:39, exit 0 (run once before the submit; memory was 474 MB free then, I did not read the 1 500 MB gate first).
- vitest, eslint, web tsc, i18n audit, doc-link check: not run (left to the commander).

## Closing texts
- **SEC-AUD-530 (Lounge Home):** On a phone the Lounge section bar is 44 px tall inside (45 with its border) and its three room links fill it; the rail, the sidebar and the find box start 16 px lower, together at y 45; the 3 links pass the 44 px hit test from the top edge (D-1022, D-1025); at 1024 nothing changed. The channel page shows the same bar and grew the same.
- **SEC-AUD-534:** On a phone the inline edit's own textarea is 44 px tall at least (was 38); its message row grows by 6 px while editing; 3 of 3 controls of the edit pass.

## Proposed new rows
None from pile (b): it is empty. The channel page's 15 remaining fails (13 in #general, 2 in the extras channel) are all pile a (author name buttons, an @mention, spoiler spans, links in message text, one embed title link), accepted by D-1022 A.4. Judgement call: the embed title link (176 x 86, wrapped) is a block of its own inside an embed card (wave 108's spec accepted it too); if it should not be accepted it needs its own row (taker its own title container, cure A.2 on `.lng-embed-title a`).

## One changelog sentence
On a phone the Lounge's top bar and the message edit box are now 44 px tall, so they are easier to tap.

## Owner questions
None.

## PROCESSES
strays: none found (21:42). No job of mine queued or running.
