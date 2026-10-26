# Handoff: feat/p26-i18n (wave 26, RDY-I18N-002 second-locale readiness)

Branch `feat/p26-i18n`, worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p26-i18n`, target dir `p26-i18n-target`. No migrations. Nothing pushed, no downloads, no new dependencies.

## Resume here

1. Review and merge. Nothing is blocked on the owner.
2. On CI the `playwright` job runs every spec in `e2e/tests` on three engines; `i18n-pseudo.spec.ts` skips itself outside Chromium, so it only does work on the Chromium leg.
3. Tick RDY-I18N-002 in `docs/13-roadmap-and-phases.md` / the inventory once that Chromium leg is green (it ran green locally, below).

## Done

### (a) `i18n-audit` TOTAL 0 (`node scripts/i18n-audit.mjs apps/web/src`)

The audit counted 41 hits in 24 files, not in the 14 files of the conflict watch alone. Reading each hit:

- 7 were real strings, now in the new fragment `apps/web/src/i18n/messages/en/p26-i18n-1.ts` (10 keys): the `SIDE ROOM` wordmark in `cinema/Shell.tsx` (two places) and `developers/DevelopersRoot.tsx`, the footer `Side Room <em>Cinema</em>` in `cinema/Shell.tsx`, and example placeholders in `developers/tabs/Activities.tsx` (three), `developers/tabs/InteractionsEndpoint.tsx`, `admin/SchemaForm.tsx` and `admin/sections/AuditLog.tsx`. Extraction only; the rendered English is identical.
- The other 34 were false positives of the heuristic. The 14 files of the conflict watch are almost all of this kind (a `>` that closes a comparison or a generic such as `a.length > 0 && b <` or `Record<string, X>`, in `MessageItem`, `MessageList`, `ComposerBackdrop`, `GuideGrid`, `MobileAgenda`, `Overlays`, `Overview`, `lounge/live.tsx`, `DataTable`, `Announcements`; the string `SAMPLE_LINES` in `mockData.tsx`; shell commands `sr-cli ...` in `CliParity`, `Invites`, `Staff`, `LiveTvSection`; the `curl` language label in `Commands.tsx`; invite examples in `AddServerDialog.tsx`; the `mainnet` network names in `OverviewSection.tsx`). None of those 14 files was edited, so the other three agents cannot conflict with this branch there. I taught `scripts/i18n-audit.mjs` to recognise them instead (`CODE_LIKE`, `TECHNICAL`, `TECHNICAL_TOKENS`, documented in the script): code-looking text and a short allowlist of fixed technical names. Anything that reads as a sentence still counts.
- `--ts` mode (the stricter plain-`.ts` scan) is untouched and still reports 336 label-table candidates; it is not part of the acceptance.

### (b) Build-time `en-XA` pseudo-locale

- `scripts/pseudo-locale.mjs` (+ `pseudo-locale.d.mts`) reads every English fragment and writes the accented, about 35 percent longer, bracketed catalog (10353 keys), keeping ICU arguments, plural cases, `#` and quoting. `node scripts/pseudo-locale.mjs --out file.json` prints it.
- `apps/web/vite/pseudoLocale.ts` serves it as the virtual module `virtual:sr-pseudo-locale`: real catalog on the dev server and in a build made with `SR_PSEUDO_LOCALE=1`, an empty module otherwise. `vite.config.ts` defines `import.meta.env.VITE_SR_PSEUDO_LOCALE` from that variable.
- `apps/web/src/i18n/runtime.ts`: `PSEUDO_ENABLED = DEV || VITE_SR_PSEUDO_LOCALE`. The pseudo entries of `LOCALES`, the loader, the `ar-XB` runtime transform and `pseudo.ts` all sit behind it, so a production build contains none of them. `?lang=en-XA` (or `ar-XB`) previews a pseudo-locale for one page load and saves nothing (`setLocale(code, persist=false)`); `adoptAccountLocale` leaves such a preview alone (`isPreviewLocale`). Before this fix the Lounge swapped the preview for the account's English.
- `scripts/check-bundle-budget.mjs` got one assertion: no chunk of `dist` holds the pseudo alphabet or the picker labels, unless the build was made with `SR_PSEUDO_LOCALE=1`. Checked both ways: a normal `vite build` prints `ok pseudo`, and an `SR_PSEUDO_LOCALE=1` build run through the script without the variable prints `FAIL pseudo`.
- `apps/web/src/i18n/pseudoLocale.test.ts` proves the generated catalog and the runtime transform give the same text nodes for every key, and covers the registry, the unsaved preview and `adoptAccountLocale`.

### (c) `e2e/tests/i18n-pseudo.spec.ts`

Entrance (landing, login, register), Lounge channel, Settings (account, language), Library home, Cinema title, Admin home, each at 1280 and 375 pixels wide, Chromium only (the spec skips other engines). Asserts the page renders in `en-XA` (so it cannot pass vacuously), the page does not scroll sideways, and no control clips its text (`overflow-x: hidden|clip`, no ellipsis, `scrollWidth > clientWidth`; inputs and visually hidden helpers excluded). A self-test proves the scanner finds a clipped button and leaves an ellipsis and a non-control alone. A `KNOWN` array (empty) takes findings owned by other screens, with a staleness check.

Findings and fixes:

- Library home at 375: the footer link row did not wrap, the page scrolled sideways by 106 px. Fixed: `.lib-footer-links` wraps (`library/library.css`).
- Cinema title at 375: the "The lounge / The cinema / The library" bar overflowed by 69 px. Fixed: the bar's nav scrolls inside the bar on phones (`cinema/cinema.css`, mobile block).
- No control clipped its text on any of the nine routes at either width. Truncation by ellipsis (designed) is not counted.

### (d) `docs/dev/i18n.md`

How a locale is added: the fragment folder and merge, `corePaths.ts`, registering the locale and its loader, the two audits, dates and numbers (`formatNumber`, `formatDate`, `formatTime`, `formatDateTime`, `formatRelative`, `setFormatLocale`, `setHour12`), the pseudo-locales and how to run the spec. Checked against the code as written (`runtime.ts`, `react.ts`, `accountLocale.ts`, `corePaths.ts`, `store.ts`, the picker in `settings/user/sections/App.tsx`).

## Still open

- Nothing from RDY-I18N-002. A real second language (a `de.ts` catalog, its registration and the server-side mail templates) is out of scope; the doc says how.

## Tests run (all on this branch)

- `node scripts/i18n-audit.mjs apps/web/src`: TOTAL 0. `node scripts/i18n-verify-diff.mjs`: MISSING 0.
- `vitest run src/i18n` (7 files, 49 tests): passed. Earlier slices of `src/admin`, `src/developers`, `src/cinema` passed except for timeouts of heavy screens while three cargo builds and three other agents ran (`firstpaint.test.tsx` "holds a non-entrance route", and some `src/admin/__tests__` files); `firstpaint` fails the same way on the untouched base under that load, and the admin files pass when run alone. They need one quiet re-run by the integrator.
- `tsc --noEmit` (apps/web and e2e), `eslint` on `src/i18n` and `vite`, prettier on every touched file: clean.
- `vite build` then `node scripts/check-bundle-budget.mjs`: all budgets ok (entrance 232.2 KB of 300) and `ok pseudo`.
- Playwright, Chromium, `sr-server` (debug build, memory storage, scratch database `sr_p26_i18n`, since dropped) on 8080 and Vite on 5173: `i18n-pseudo.spec.ts` 20 passed against the dev server, and 20 passed against `vite preview` of a build made with `SR_PSEUDO_LOCALE=1`. The first run found the two overflows above and the preview bug; all three fixed before the green runs. The servers were stopped by port owner.

## Decisions

- The pseudo-locales now exist only on dev hosts and in `SR_PSEUDO_LOCALE=1` builds, so the pseudo entries of the Language picker are gone from a normal production build, including for people in app test mode. That is what the task asks for ("never in a production bundle").
- `SR_PSEUDO_LOCALE` is read at build time (`vite build`), because the point is that the code is absent otherwise. "Under `vite preview` with `SR_PSEUDO_LOCALE=1`" therefore means: build with it, then preview.
- Audit false positives are fixed in the audit, not by touching 14 files that other agents own.
- The generated catalog is read when first requested; after adding a fragment, restart the dev server.

## Open owner questions

None. (Conservative defaults taken above: pseudo-locales out of production including app test mode; audit allowlist for technical literals.)

## Environment notes

- `sr-server` and `sr-cli` were built once with `CARGO_TARGET_DIR=...\p26-i18n-target -j 2`. The wasm packages (`packages/wasm/dist`, untracked) were copied from the `p26-lounge-followups` worktree instead of rebuilding them; the wasm crates are the same on this base.
- `pnpm` is not on PATH here; Vite was started directly (`apps/web/node_modules/.bin/vite`) and Playwright reused it.
