# Handoff: wave 108 lounge (code builder, pod lounge): D-1022, D-1024; SEC-AUD-519, 520, 521, 522

## Clock times
Started 20:44; REACH table and both files ready 20:46 (no job: sheets only).

## Done (commits)
- `apps/web/src/lounge/msg/msg.css`: D-1022 (READY)
- `apps/web/src/lounge/lounge.css`: D-1022 (ALL READY)
- this handoff.

## Not done
- Nothing measured (no job for a code builder; `loungeshots` measures). The wrapped-reactions row gap is not measured (the seed has no wrapped line).
- 522: `InlineEdit.tsx` (read only for me) sets NO `placeholder` attribute on the inline textarea, so no placeholder text is ever drawn; the "rgb 49,51,56" in wave 107's run was most likely a computed-style read of a pseudo-element that has no text. The own-selector rule is in anyway, as asked. If loungeshots still reads the text colour, say so: the fix is not a sheet.

## REACH table
| control | selector; sizing rule now | page / how opened | cure | will move |
|---|---|---|---|---|
| reaction pill (24 px, area -11) | `.lng-reaction` lounge.css:1240 (h 24), `::after` inset-block -11 msg.css phone block | channel message with reactions, 375 | A.1 kept; the row after it was painting over the lower reach: `.lng-root .lng-reactions` gets `margin-bottom: 11px` (msg.css phone block, the reactions rule) | message with reactions +11 px (limit 12), phone only |
| wrapped reaction lines | `.lng-reactions` lounge.css:1234 (gap 4) | same, only when pills wrap | same block: `row-gap: 20px` (column gap stays 4) | only a wrapped row grows; NOT measured |
| inline edit "cancel" / "save" (17 px) | `.lng-link-btn` msg.css:10 | channel, own message, Edit; 375 | A.2 by `.lng-inline-hint .lng-link-btn` (inline-flex, min-block-size 44, phone only) | the hint line (`p.lng-inline-hint`, InlineEdit.tsx:110) to 44 px |
| "Jump to original" (22 px) | `.lng-link-btn` msg.css:10 | thread-start system row, `SystemMessage.tsx:161` | A.2 by `.lng-system .lng-link-btn` (the only link button inside a `.lng-system` article) | that row's line to 44 px |
| "N message requests waiting" | `.lng-link-btn` in `.lng-pad.lng-dm-empty`, HomeSidebar.tsx:293-296 | Home list, no DMs, request waiting | A.2, now also by `.lng-pad .lng-link-btn` | that row 44 px (was already so with `.lng-dm-empty`) |
| inline edit placeholder | `.lng-textarea::placeholder` lounge.css:1386 | inline edit emptied | B: `.lng-textarea.lng-inline-textarea::placeholder` right after it, faint, opacity 1 | colour only, every width |

521 finding: wave 107's final msgs cure (`.lng-dm-empty .lng-link-btn`, A.2) is in this base; the listshots run was on a tree without it (its handoff shows 217x20), loungeshots' tree had it (217x44). So the sheet was right; `.lng-pad` is added as a second scope so it holds whatever the container class is.

## FINDERS (rule 13)
`git grep` of `lng-inline*`, `lng-reaction*`, `lng-link-btn`, `lng-pad`, `lng-system`, `msg.css`, `lounge.css` in `e2e/`, `apps/web/src`, `packages`, `scripts`: only old shots specs and `polish-lounge-flows.spec.ts:82` (`.lng-reaction, [aria-pressed]` visibility, unaffected); `.lng-msg:not(.lng-system)` in p68..p74 shots specs (retired, unaffected); `textLink.test.ts` reads msg.css for `--text-link` only (unchanged). Result: no CI test drives anything changed.
Dependants of the new selectors: `.lng-pad` is on ChannelSidebar.tsx:604 and QuickSwitcher.tsx:77 (plain `li`/`p`, no link button) and HomeSidebar.tsx:293; `.lng-inline-hint` only InlineEdit.tsx:110; `.lng-system` only SystemMessage.tsx:257.

## JOBS
none.

## Figures / shots
none (not run). Limits to be checked by loungeshots: pills 24 px unchanged; reactions message +11 px phone; hint/jump/request lines 44 px phone; 1024/1280 unchanged.

## Imports
`msg.css` and `lounge.css` are imported by the Lounge chunk (lazy, signed-in); `lounge.css` is also imported by `settings/server/ui.tsx`, `settings/user/parts.tsx` (lazy settings). No first-paint file changed.

## Checks
prettier --check on both files: pass, exit 0, 20:45. `node scripts/check-css-tokens.mjs`: clean, exit 0, 20:45. vitest, eslint, tsc, i18n audit, doc links: left to the commander.

## Closing text and rows
Closing: the reaction pills, the inline edit's cancel/save, "Jump to original" and the request link are 44 px targets on a phone. Proposed row: none unless loungeshots finds a faulted wrapped row.
Changelog: reaction pills, the message-edit buttons, "Jump to original" and the message-request link are easier to tap on a phone.
Owner questions: none.

## PROCESSES
strays: none found (strays.sh p108-lounge, 20:47)
