# fix-p110-lounge (code builder, pod lounge)

## Clock
Started about 23:18, edits done 23:22, committed 23:2x. No job run (sheets only; loungeshots measures).

## Done
- D-1035 / SEC-AUD-545 `lounge.css`: `.lng-msg:hover .lng-msg-tools` wrapped in `@media (hover: hover)`. The rule that showed the toolbar after a tap was `.lng-msg:focus-within .lng-msg-tools` (a tap focuses a button or the row) plus the sticky `:hover`. `focus-within` is replaced by `.lng-msg:focus-visible .lng-msg-tools` and `.lng-msg:has(:focus-visible) .lng-msg-tools` (keyboard only). The open-"More" rule is unchanged. `data-mobile` is not used.
- D-1022 / SEC-AUD-547 `shell.css`: in the Friends phone block (max-width 720px), `.lng-friends .lng-friend-row .lng-icon-btn` gets `position: relative` and a `::after` with `inset-block: -8px; inset-inline: 0` (28 + 16 = 44 px).
- SEC-AUD-552 `lounge.css`: `.lng-emoji-search:has(+ .lng-forward-list), .lng-forward-list + .lng-emoji-search { inline-size: 100% }` directly after the phone height block. The dialog is the kit Modal (no class of ours), so the two fields are found by the `.lng-forward-list` ul they sit before and after. Picker searches (EmojiPicker, ExpressionPicker, GifPicker) are not matched.

## Not done
Nothing measured by me (no job): the cures are guesses until loungeshots' pixels say so.

## REACH table
| control | rule now | opened by | cure | moves |
|---|---|---|---|---|
| `.lng-msg-tools` | lounge.css ~:1224 (display none), shown ~:1239 | hover a message; keyboard focus; "More" open | hover only under `hover: hover`; focus by `:focus-visible` | no toolbar after a tap on touch |
| Friends row `.lng-icon-btn` (More, Accept, Ignore) | lounge.css :203 (28x28) | Friends page at 375 | `::after` tap area, shell.css Friends phone block | tap area only; row height unchanged |
| forward dialog fields | lounge.css `.lng-emoji-search` (no width, 185 px default) | message More > Forward (MessageDialogs.tsx :170, :200) | `inline-size: 100%` | the two fields' width, every width |

## FINDERS
- `.lng-msg-tools` / hover in e2e: only retired `p84`, `p85`, `p109-censusa` shots specs use the toolbar (mouse `hover()`, not touch; p109-censusa describes the old tap behaviour and is a retired shots spec: not edited). CI specs `lounge-gaps.spec.ts:142` and `lounge-shell.spec.ts:59` hover sidebar channel rows, unaffected. No CI spec uses a touch context with the message toolbar.
- `lng-emoji-search` / `lng-friend-row` in CI specs and `__tests__`: none. Tests reading lounge.css as text: `sectionBarVoice.test.tsx` (reads it; it looks at the voice/section-bar rules, not the ones changed: left to CI).
- Importers: `lounge.css` by `LoungeShell.tsx`, `settings/server/ui.tsx`, `settings/user/parts.tsx` (lazy lounge/settings chunks); `shell.css` by `LoungeDialogs.tsx`. Not first paint.

## Checks
- `npx prettier --check` both files: pass (exit 0). `node scripts/check-css-tokens.mjs`: clean (exit 0).
- vitest, eslint, tsc, i18n, doc links: left to the commander.

## JOBS
none.

## Rows / changelog
Closing: 545, 547, 552 cured in sheets, awaiting loungeshots. Changelog: A tap on a message no longer pops the hover toolbar on touch devices (long-press menu unchanged); the Friends buttons have 44 px tap areas on phones; the forward dialog's fields fill the dialog.
Owner questions: none.

## PROCESSES
strays: none started by me (no process started).
