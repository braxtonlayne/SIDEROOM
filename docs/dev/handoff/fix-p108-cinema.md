# fix-p108-cinema (code builder, pod sections)

## Clock
Start 20:49. REACH and ALL READY committed 20:51. Reply 20:52.

## Done
- `D-1022 cinema.css (ALL READY)`: one commit, `apps/web/src/cinema/cinema.css`, inside the existing `@media (max-width: 768px)` block after `.cin-post-actions .cin-link-btn::after` (the sheet's own phone rule).

## Not done
- Card "Play" / "Add to My list": no change. Not as the task says (see REACH row 1).

## REACH table
| control | rule now | page / how opened | cure | what moves |
|---|---|---|---|---|
| Card "Play" (`a.cin-qa`) and "Add to My list" (`button.cin-qa`) | cinema.css `.cin-qa` (30x30) and, at max-width 768, `.cin-card .cin-qa` 44x44 (wave 107) | Cinema Browse, Discover, My list poster overlay; shown on a phone | none new. They are already 44x44 on a phone; sectionshots 107 reports 0 named failing for cin-qa. `.actions` (kit Poster.module.css) is a later sibling of the art link inside `.frame`, so it paints over the art; the fallback art span is inside the `a.link`, not decoration to turn off (`pointer-events: none` would only move the tap to the link). The staffshots census (taker: fallback art) predates the 107 cure. A 44 px circle has its +-21 px points inside it. | nothing |
| Search "open all" and "Filters" (`.cin-search-foot .cin-link-btn`) | `.cin-link-btn` cinema.css ~343, padding 4px 0 (about 30 px high); footer `.cin-search-foot` flex, no wrap | Search modal (header search) | A.1, `inset-block: -8px`, max-width 768 | nothing: 12 px grid gap above, nothing below; options are not covered |
| Spoiler reveal (`.cin-spoiler .cin-link-btn`) | same, inline in `p.cin-spoiler` | title page Comments / Reviews, a spoiler post | A.2, `min-block-size: 44px; vertical-align: middle`, max-width 768 | that line about +16 px until revealed (the post's own 6 px gap to the action line is smaller than two reaches, so A.1 would be painted over) |
| Comment actions (`.cin-post-actions .cin-link-btn`) | wave 107 | unchanged | none | nothing |

## FINDERS
`git grep` of `cin-link-btn`, `cin-spoiler`, `cin-search-foot` and `cinema.css` in `e2e/`, `__tests__`, `apps/web/src`, `packages`, `scripts`: only the retired `p107-section-shots.spec.ts` (reads, not CI) and `CinemaRoot.tsx` importing the sheet (lazy cinema chunk). No test reads cinema.css as text. None drives these controls.

## JOBS
none.

## Figures
Not measured (no job). Built by rule, not measured: spoiler button (no comment in the base seed). Search footer is measurable by sectionshots.

## Importers
cinema.css: `CinemaRoot.tsx` (lazy cinema chunk; not first paint, not a public page).

## Checks (last commit)
prettier --check cinema.css: ok 20:50. `node scripts/check-css-tokens.mjs`: clean 20:50. vitest, eslint, tsc, i18n: left to the commander.

## Closing text, changelog
Changelog: On a phone, Search's footer links and the spoiler reveal button have 44 px tap areas.
Proposed row: spoiler and comment lines not measured in the second census (needs seed comment).
Owner questions: none.

## PROCESSES
strays: only the harness's own bash shells (not mine).
