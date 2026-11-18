# Wave 103 handoff: kit (SEC-AUD-462, D-971)

Clock: started 11:56, `(READY)` commit 11:57 (wave 103 clock as `date` showed it).

## Done
- `SEC-AUD-462 NativeSelect fit="content" (READY)`: `fit?: 'fill' | 'content'` (default `fill`), not passed to the `<select>`.
- `SEC-AUD-462 NativeSelect fit story`: `NativeSelectFitContent` in `Primitives.stories.tsx` (inline-flex label; flex-wrap row with a button).

## The rules and why
- Wrapper `.wrapFit`: `display: inline-block; inline-size: max-content; max-inline-size: 100%; vertical-align: middle`. Select `.selectFit`: `inline-size: 100%; max-inline-size: 100%`.
- The wrapper carries the size, the select fills the wrapper. So: (1) the chevron (anchored to the wrapper) is always inside the box; (2) no percentage-width select sits in a shrink-to-fit parent (the Season 46 -> 18 px collapse); (3) an explicit inline size means a column flex or grid parent cannot stretch it, so no `align-self`/`justify-self` is needed and centred inline-flex rows keep their centring; (4) `max-inline-size: 100%` caps it at the parent (the select then ellipsizes).
- Measured in Chromium on a static page with the kit's field CSS: inline-flex label 115 px (Season 46), flex-wrap row beside a button and input 154 px (not a whole line), column-flex parent 85 px (not stretched), 100 px parent with a long option 100 px (capped). Height 40 in all. Pods' shots builders measure on real pages.
- `multiple`/`size`: `fit` puts `selectFit` on the select only (no wrapper).
- No class or style prop for the wrapper.

## Tests
- `npx pnpm@10.12.1 --filter @sideroom/ui exec vitest run src/primitives/native-select.test.tsx`: 9 passed. prettier, eslint, `tsc --noEmit` (ui package): exit 0.

## TYPECHECK
`bash cli-agents/tc.sh run kit bash -c "cd .../p103-kit/apps/web && npx tsc --noEmit -p ."`, finished 12:04, exit 0. Kit package `tsc --noEmit`: exit 0. `check-css-tokens` (73 files clean), `i18n-audit --max 0` (TOTAL 0), `check-doc-links` (ok): all exit 0.

## Build (heavy.sh run kit, finished 12:33, exit 0; earlier tries exit 75 busy)
- First paint: `app/app-1awzrHvz.js` 218 409 B gzip, room 37 591 B (before: 218 419 B, room 37 581 B).
- `check-bundle-budget`, `check-entrance-isolation`, `check-staff-delivery`: all exit 0.
- The select's sheet and code are in their own lazy chunk `app/NativeSelect-*.css` / `.js`, not in first paint. No rule of mine names an app section or a theme (D-385).

## STACK-ROUND
none (code builder; one build under `heavy.sh run`).

## heavy.sh status (12:34)
`mediashots 12:33` (not mine); no hold in my name.

## Closing text for SEC-AUD-462
`NativeSelect` takes `fit="content"` (wrapper `max-content`, capped at its parent, select fills the wrapper so the chevron stays in the box). Close once the pods have moved their selects.

## Changelog sentence
The kit's `NativeSelect` gains `fit="content"`: a select as wide as its longest option instead of filling its parent, for inline-flex labels, wrap rows and chips.

## AGENTS.md section 4 sentence
Use `<NativeSelect fit="content">` where the select sits in a shrink-to-fit place (inline-flex label, flex-wrap row beside buttons, chip); leave the default `fit="fill"` for a select that fills its field; never style the kit's wrapper from an area sheet.

## Owner questions
None. Note: the wrapper is `max-content` so a column-flex/grid parent never stretches it; a `fit="content"` select that should fill a column must use the default.
