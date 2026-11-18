# Wave 103 handoff: shelves (pod library; SEC-AUD-465 item 1, SEC-AUD-469; D-972, D-974)

Code builder, stack-free. Started 11:56 (session clock). Status: code in, waiting for kit's `fit` prop to typecheck (see "Kit").

## REACH table
`.lib-select` itself has no rule of its own in the sheet: it is a plain block `label`, so its span and the select sit side by side. All five took `fit="content"` (the base's raw select was as wide as its content). Theme for all: Library shipped `sideroom`, and once `lounge-light`. Ways in: `e2e/tests/p102-pages-shots.spec.ts`, `p102-library-shots.spec.ts` reached every page. Old class on the select: none was on it; tests find them as `.lib-edition select`, `.lib-select select`, `.lib-rate select` (selectors by element, still true under the kit wrapper).

| file:line (base) | label / id | page and how to open | rule that styled it (base) | now | fit | found by |
|---|---|---|---|---|---|---|
| SeriesPage.tsx:182 | Edition (`label.lib-select.lib-edition`) | a series with 2+ editions, header | `.lib-select select` | NativeSelect | content | `.lib-edition select` (p102-pages) |
| Rankings.tsx:53 | Kind (sr-only span) | /library/rankings, beside scope switch | `.lib-select select` | NativeSelect | content | `.lib-select select` (p78) |
| Pages.tsx:186 | My library Sort | /library/me, page head | `.lib-select select` | NativeSelect | content | `.lib-select select` (p102-pages:435) |
| Lists.tsx:53 | List visibility (editor) | a reading list editor | `.lib-select select` | NativeSelect | content | `.lib-select select` |
| SeriesSocial.tsx:341 | Rating, review form (`.lib-select.lib-rate`, inline-flex) | series, Reviews tab, write a review | `.lib-select select` | NativeSelect | content | `.lib-rate select` (p101:454) |
| Browse.tsx:128 | Sort (kit already) | /library/browse toolbar | none (kit) | label gets `className="lib-sort"` | kit default (fill) | - |
| Browse.tsx:458, 485, 499 | filter selects, `.lib-select-block` (kit already) | browse filters | - | unchanged | fill | - |
| SeriesSocial.tsx:93, 461 | series "Your rating" and add-to-list, `.lib-select-block` (kit already) | series page | - | unchanged | fill | - |

Not under a `.lib-select*` label: the reader end card's rating (reader's).

## Done
- 5 commits `SEC-AUD-465 library/<file>.tsx (READY)`: SeriesPage, Rankings, Pages, Lists, SeriesSocial: `<select>` to `<NativeSelect fit="content">`, import added, nothing else.
- `library.css`: deleted `.lib-select select` (was `:737-745`: 36 px, `padding 0 10px`, bg, colour, border, `font: inherit`, 14 px). Dependants: every raw select it reached is now the kit's (git grep of `.lib-select select` / `lib-select` shows only these five labels; no other raw select sits under `.lib-select`). `.lib-select-block` untouched.
- New block at the end of `library.css` (SEC-AUD-469, D-974): `.lib-select, .lib-sort { font-size: 14px }`. The kit select inherits. Note: `.lib-rate` (`:1316`, 13 px) comes earlier in the file, so the review rating's label now reads 14 px, like the other fields (it was 13 label text, 14 px select).
- `Browse.tsx:120`: only change is `className="lib-sort"` on the toolbar sort's `<label>`.
- `.lib-rate` `justify-self: start` not added: add only if libshots shows the label stretched.

## Who imports (lazy chunk)
All six files are under `apps/web/src/library/`, routed from the Library's lazy route chunk (not first paint, not a public page); `library.css` is imported by the Library entry only.

## Kit
`grep fit packages/ui/src/primitives/Select/NativeSelect.tsx` found nothing at the start; kit had no `(READY)` commit when I committed. The five commits carry `fit="content"` and need `git merge --no-edit fix/p103-kit` before typecheck. See "Checks" for what ran.

## STACK-ROUND: none (code builder)

## Proposed rows
- SEC-AUD-465 item 1: the Library's five raw selects (Edition, Rankings kind, My library sort, list visibility, review rating) moved to the kit's `NativeSelect fit="content"`; `.lib-select select` deleted.
- SEC-AUD-469: the Library's field type size comes from the label (`.lib-select`, `.lib-sort`, 14 px), not from a select rule.
- Changelog: The Library's last five native selects now use the kit select, and the Library sets its field type size on the label instead of on every select.
- Owner questions: none.

## Checks (own tree, after the last edit, kit merged: `fit` prop present)
- Kit READY merged into my branch (`git merge fix/p103-kit`), 11:58.
- TYPECHECK: `tc.sh run shelves ... tsc --noEmit -p .` exit 0 (about 12:00).
- prettier --check on the six .tsx and library.css: clean. eslint (web package's own) on the six .tsx: exit 0.
- vitest single files, unedited: library.test.tsx 18/18, p13.test.tsx 10/10, p20polish.test.tsx 4/4.
- `check-css-tokens.mjs` exit 0; `i18n-audit.mjs --max 0` exit 0; `check-doc-links.mjs` exit 0.
- Not run: stack, shots, spec (libshots').
- Lock: no heavy lock taken; scratch databases and binaries: none made.
