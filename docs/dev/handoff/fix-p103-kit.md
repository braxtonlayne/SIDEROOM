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
