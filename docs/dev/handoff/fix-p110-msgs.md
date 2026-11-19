# fix-p110-msgs (wave 110, D-1022: SEC-AUD-542, 546, 548, 549)

## Clock times
Started 23:17. Edits and checks done 23:19. Committed `D-1022 msg.css (ALL READY)`. No spec, no job (sheet-only builder).

## Done
One commit, `apps/web/src/lounge/msg/msg.css` only: four small blocks, each directly after the rule it amends, phone rules in `@media (max-width: 720px)` (the sheet's own breakpoint).

## Not done
Nothing measured: no job is mine. All pixel figures are for loungeshots to confirm; "Code that no job ever ran" is judged by that round.

## REACH table
| row | selector | sized now by | page / how opened | cure | will move |
|---|---|---|---|---|---|
| 542 | `.lng-embed-title a` | no rule; an inline link in `.lng-embed-title` (msg.css `.lng-embed-title a`, about :312) wrapped over lines | channel message with a link embed; 375 | A.2: `display:flex; align-items:center; inline-size:fit-content; max-inline-size:100%; min-block-size:44px` | the title's line box only (one-line titles become 44 px, text centred) |
| 546 | `.lng-forward-row` | `display:flex; padding:4px 0` (about :519), 13 px checkbox, row about 21 px | message menu > Forward; 375 | A.2: `box-sizing:border-box; min-block-size:44px` | each row to 44 px; the list keeps `max-height:240px` and scrolls |
| 548a | `.lng-attach-file .lng-icon-btn` (download icon link) | `.lng-icon-btn` 28x28 in `lounge.css:203`, squeezed to 20 wide by the flex card | channel message with a file attachment | `flex:none` at all widths; on a phone `position:relative` + `::after` `inset-block:-8px` | icon 20 -> 28 wide at every width (name gives up 8 px); no height change |
| 548b | `.lng-attach-file a.lng-file-name` | `overflow:hidden` ellipsis link, 22 px tall (about :166) | same | A.1 cannot be an `::after` (the link has `overflow:hidden`, which would clip it): on a phone `padding-block:11px; margin-block:-11px; position:relative`, a 44 px box with no layout change | none in layout; its box overlaps the size line and the 2 px above the card padding |
| 549 | `.lng-lightbox-bar` and `.lng-lightbox-bar .lng-link-btn` | flex row, `gap:12px`, no wrap (about :262) | click an image in a message, 375 | A.3 + A.1: `flex-wrap:wrap; row-gap:24px`; links `position:relative` + `::after inset-block:-12px` | bar one line to two or three; the dialog body (scrolls, 16 px bottom padding holds the last row's 12 px reach) |

Dependants of the changed classes (`git grep`): `.lng-embed-title` is also a plain div in `InviteCard.tsx:86` (no `a`, not reached); `.lng-attach-file` also on the expired cards (`Attachments.tsx:150,164`, no icon link) and `.lng-media-card` (`:243`, a span name, not `a.lng-file-name`); `.lng-file-preview`/Preview button in the card is a `.lng-link-btn` outside the bar, untouched; `.lng-lightbox-bar` prev/next icon buttons (28 px) are untouched (not in the row); wave 107/108 `.lng-link-btn` phone rule untouched.

## FINDERS (rule 13)
- Classes `lng-embed-title`, `lng-forward-row`, `lng-file-name`, `lng-lightbox-bar`: no unit test or CI spec drives them; only retired shots specs (p107/p108 `embed-title-link`, p109 census) read `.lng-embed-title a` and `.lng-attach-file`.
- `.lng-attach-file`: `e2e/tests/polish-lounge-flows.spec.ts:147,152` waits for it and checks it does not run off a 375 screen; my cure does not widen the card (only the icon gets back its 28 px, inside the card's own max-width).
- `open-in-studio`: `openInStudio.test.tsx`, `studio.spec.ts:120` by test id; the button's markup is unchanged.
- Tests that read the file as text (`git grep msg.css`): `textLink.test.ts:10` and `p103-search-shots.spec.ts` mention it in comments only. None.

## JOBS
None.

## Figures against the task's lists
Not measured (no job). Expected: only the rows in the REACH table move.

## Shots
None (loungeshots').

## Who imports the file
`apps/web/src/lounge/Composer.tsx:99` and `MessageList.tsx:43`, both lounge chunk files (lazy, not first paint).

## Checks
- `npx prettier --check apps/web/src/lounge/msg/msg.css`: passed, rc 0, 23:18.
- `node scripts/check-css-tokens.mjs`: 73 files clean, rc 0, 23:18.
- vitest, eslint, web tsc, i18n audit, doc-link check: not run (left to the commander).

## Closing text and new rows
Rows 542, 546, 548, 549: cure in `msg.css`, awaiting loungeshots' round. Risk to check: 548b (padding/negative margin on the name link: the 11 px reach overlaps the size line, and 2 px above the card; check nothing above is covered) and 549 (the row gap with the 28 px icon buttons).

## Changelog sentence
On a phone, the embed title link, the Forward dialog's destinations, the file card's name and download links and the image viewer's links now meet the 44 px target, and the file card's download icon keeps its width at every size.

## Owner questions
None.

## PROCESSES
strays.sh p110-msgs: `strays: none found`. No job of mine ever existed.
