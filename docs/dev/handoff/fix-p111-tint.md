# fix-p111-tint (tint, wave 111; D-1035 extended; SEC-AUD-560)

## Clock
Started 00:42, last job ended 00:48, handoff 00:49 (T = 01:00 not yet reached by the clock; limits unaffected).

## Done (commits on fix/p111-tint)
- Spec `e2e/tests/p111-tint-shots.spec.ts` (new), two follow-up commits.
- `apps/web/src/lounge/lounge.css`: the `.lng-msg:hover, .lng-msg:focus-within` background rule became `@media (hover: hover) { .lng-msg:hover }` plus `.lng-msg:focus-visible, .lng-msg:has(:focus-visible)` (same value `var(--ui-bg-hover)`). Nothing else touched.

## Confirmed (base, step 1)
After a tap on a message's text at 375 touch the row had `hover,focus-within` and background rgb(232,234,237) Light / rgb(53,55,60) Dark; an untouched row is transparent. The only rule setting it: `.lng-msg:hover, .lng-msg:focus-within => var(--ui-bg-hover)`. A long press with the menu closed leaves the row transparent (no fault there). 1024 hover: rgb(232,234,237).

## Figures (after)
- May change: tapped row 375 Light and Dark: now transparent = untouched row (sameAsUntouched false -> true both themes).
- Must not change, all identical base to after: 1024 hover bg rgb(232,234,237) (rule now `.lng-msg:hover`); keyboard focus at 375 (Light rgb(232,234,237), Dark rgb(53,55,60); now set by `:has(:focus-visible)`); mentioned row bg color(srgb .6627 .4196 0 / .1) at 375 (also after a tap) and at 1024 (also under hover; `[data-mention]` in msg.css wins over hover as before); long-press row. Problem lines: none. Edited/replied/jumped (`data-flash`) rows not reached by the spec; their rules are untouched and have equal specificity, order unchanged.
- Boxes: a background change only; no layout property changed. The only other dependants of the changed rule are every `.lng-msg` (messages, system rows, thread view).
- Side effect to know: with a mouse, a row focused by a click no longer keeps the tint after the pointer leaves (the old `:focus-within`); hover and keyboard focus show it as before.

## FINDERS
`git grep` for readers of a row's background in `e2e/` and tests: none (only `p72-prose-shots` reads a mention chip's background, not a row). Tests that read lounge.css as text: `sectionBarVoice.test.tsx` (reads the file; unrelated rule, left to the commander's vitest run), `msgGaps` / `prefsEffects` / `p14roleGradient` mention `.lng-msg` but not this rule.

## JOBS
0346 shoot base (first spec), 0347 shoot base (dark theme fixed), 0348 build, 0349 shoot after; all rc 0, each about 45 s run, little wait.

## Shots (opened after the jobs ended)
- tap-message-375-light.png: row not tinted after the tap, no toolbar; base twin: whole row tinted grey.
- tap-message-375-dark.png: no tint, no toolbar (base twin not opened; figures show rgb(53,55,60) in base).
- hover-message-1024-light.png: row grey-tinted with the toolbar, as in base (same colour).
- mention-row-375-light.png: warm tinted row with the left bar (mention route-injected on a seeded message); same value as base.

## Importers
lounge.css is imported by `lounge/LoungeShell.tsx`, `settings/server/ui.tsx`, `settings/user/parts.tsx` (lazy lounge/settings chunks).

## Checks
prettier --check on both files and check-css-tokens: passed. e2e tsc: not run (memory under 1 500 MB at the time; Playwright compiled the spec). vitest/eslint/tsc web/i18n/doc-links: left to the commander.

## Closing text for SEC-AUD-560
Closed in wave 111 (D-1035): the message row's hover background is under `@media (hover: hover)` and keyboard focus tints by `:focus-visible`. At 375 touch a tapped row went from the hover tint (rgb(232,234,237) Light, rgb(53,55,60) Dark) to transparent like an untouched row; the 1024 hover tint, keyboard focus and the mentioned row are unchanged. Handoff `fix-p111-tint.md`.
Changelog: A tapped message row on a phone no longer keeps its hover tint.
New rows: none. Owner questions: none.

## PROCESSES
strays: none found
