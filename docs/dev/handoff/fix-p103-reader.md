# Wave 103 handoff: reader (code builder, pod library; SEC-AUD-462, SEC-AUD-465 item 1; D-972)

## REACH table (committed 2026-11-18, before 12:35)
| file:line | label / id | page and how to open | sheet rule now | theme | raw or kit | fit | found by |
|---|---|---|---|---|---|---|---|
| `apps/web/src/library/reader/Reader.tsx:844` (`ChapterPicker`, in `label.lib-chapter-select`) | `lib-sr-only` span `library.reader.chapterSelect`; no id | `/library/r/<chapterId>/1` (full-screen Reader, outside the section chrome). Renders only when the series has 2+ chapters. | `library.css:1040` `.lib-chapter-select select` (max-width 220px, height 32px, #1c1d18 / #ece6d6 / #2a2b24) | Reader chrome is its own dark bar (literal colours); page theme = Library `sideroom` | raw (becoming kit) | `fit="content"` (it was content-wide in an inline label, capped 220) | `e2e/tests/p102-pages-shots.spec.ts:488,495,508,510` by `.lib-chapter-select select` and `.lib-chapter-select` (xpath parent = the bar). Keep the `lib-chapter-select` label class. |

How to open the bars: the chrome (`data-chrome="on"`) is on at load and after any `poke()` (pointer move, key, tap on the stage), and it hides by a timer: `CHROME_HIDE_MS = 3000` (`Reader.tsx:46`) with no panel open. A click in the middle of the stage (`onStageClick`) toggles it in strip mode; any key press or mouse move on the stage brings it back. A panel open (comments, bookmarks, settings) keeps it on. For a hit test or a shot: poke, then measure within 3 s, or open a panel to pin it. The p102 spec loops ArrowRight/ArrowLeft + a centre click until the picker is visible.

## Chevron colours (read from `packages/ui/src/tokens/themes.ts`, for libshots to measure with `e2e/support/contrast.ts`)
The kit's chevron is drawn in `--ui-text-muted` on the wrapper's `::after`; the bar ground is `#1c1d18`.
- Library shipped theme `sideroom`: `--ui-text-muted` `#a9a493` on `#1c1d18` = about 6.8:1 (passes 3:1).
- `lounge-light` (chosen for the Library): `#5c5e66` on `#1c1d18` = about 2.6:1 (UNDER 3:1). Computed by hand from the token values; libshots must measure it with the helper. If the chevron is under 3:1 in the light shot, the cure is (a): picker back to raw, rule as the base had it.

## Done (commits on `fix/p103-reader`)
- Merged `fix/p103-kit` (`872f2312`, `fit="content"`) into this branch, as the brief says.
- `SEC-AUD-465 Reader.tsx (READY)`: the chapter picker is `<NativeSelect fit="content" className="lib-chapter-pick">`, same `value` and `onChange`, the `lib-sr-only` label span and the `label.lib-chapter-select` untouched. `.lib-chapter-select select` (as p102's spec finds it) still matches the inner select.
- `SEC-AUD-465 library.css (ALL READY)`: `.lib-chapter-select select { max-width; height: 32px; background; color; border }` became `.lib-chapter-select .lib-chapter-pick { max-inline-size: 220px; background: #1c1d18; color: #ece6d6; border-color: #2a2b24 }` (two classes, so it wins over the kit's one class whatever the chunk order).
  - `height` and the 1 px border width are gone: the kit's 40 px (44 on a phone) field and border are the field now (D-972).
  - Kept: the 220 px width cap (a layout need: the bar is a flex row; `fit="content"` also makes the field content-wide, which is what the base's 218 px at 375 was, so the p102 103 px collapse should not recur), and the three literal colours, because the Reader's bar is a dark chrome of its own and not the theme's. Only `border-color` is set, the kit's border width/style stay.
- Nothing else in `library.css` touched.

## Who imports Reader.tsx
`git grep` : only `apps/web/src/library/routes.tsx:15` (`lazyNamed(() => import('./reader/Reader'), 'Reader')`): the Library's lazy chunk. Not first paint, not a public page. `ChapterPicker` is local to `Reader.tsx`. `lib-chapter-select`/`lib-chapter-pick` are used by no other file or unit test; `e2e/tests/p102-pages-shots.spec.ts` finds it by `.lib-chapter-select select` (still valid).

## Checks (all on this tree after the last edit, 2026-11-18, about 11:58)
- `prettier --check` Reader.tsx, library.css: clean.
- web package eslint on Reader.tsx: exit 0.
- vitest single files, unedited: `reader.test.tsx` 16 passed, `chrome.test.tsx` 5 passed, `p13.test.tsx` 10 passed (the only test naming the chapter picker is p13).
- `node scripts/check-css-tokens.mjs`: exit 0 (73 files clean). `node scripts/i18n-audit.mjs --max 0`: TOTAL 0. `node scripts/check-doc-links.mjs`: ok.

## TYPECHECK
`bash cli-agents/tc.sh run reader bash -c "cd .../p103-reader/apps/web && npx tsc --noEmit -p ."`: RESULT_PLACEHOLDER

## STACK-ROUND: none (code builder)

## For libshots
Measure the chevron with `e2e/support/contrast.ts` against `#1c1d18` in the Library's shipped `sideroom` (hand figure 6.8:1) and in `lounge-light` (hand figure 2.6:1, under 3:1). If light fails, FAULTS entry for reader with cure (a): picker back to raw and the rule exactly as the base had it (`git show integration/phase103:apps/web/src/library/library.css`, lines 1040-1046).

## Proposed text
- SEC-AUD-462 / 465 item 1: the Reader's chapter picker is the kit's `NativeSelect` with `fit="content"`; its sheet rule no longer styles a `select` by element name or sets a height; the dark bar colours and the 220 px cap stay on `.lib-chapter-select .lib-chapter-pick`.
- Changelog: The Reader's chapter picker is now the kit's native select, 40 px tall (44 on a phone), content-wide up to 220 px, keeping the reader bar's dark colours.

## Owner questions
- The kit chevron (`--ui-text-muted`) on the reader's fixed dark bar is about 2.6:1 if the Library is set to Lounge Light. Accept, or should the Reader bar use its own chevron colour (needs a kit hook, not in this wave)?
