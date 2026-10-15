# feat/p12-firstpaint

## Done
First paint no longer waits for the `en.rest` catalog chunk (~92 KB gzip, 377 KB raw).

- `main.tsx` renders as soon as the locale is restored. `initLocale()` starts `loadFullCatalog()` in parallel and does not await it (`startFullCatalog` in `i18n/runtime.ts`).
- `i18n/messages/store.ts` tracks "rest catalog loaded" (`isRestLoaded`, `markRestLoaded`, `onRestLoaded`); `en.rest.ts` calls `markRestLoaded()`. The runtime emits on it, so every `useT` subscriber re-renders when the catalog grows. `useFullCatalogReady()` (react.ts) exposes it. Tests get the full catalog synchronously as before (`test/setup.ts` imports `en.rest`, which marks it loaded).
- `i18n/CatalogGate.tsx` renders children only when the full catalog is ready (or its load failed, so the app opens degraded rather than blank). `RootLayout` in `router.tsx` wraps the `Outlet` in it with `bypass={isCorePath(pathname)}`; `i18n/corePaths.ts` lists the entrance paths. Unknown paths wait (safe by default). `bypass` is a prop, not a second tree, so navigating entrance to Lounge does not remount the route (an earlier two-tree version re-posted the login-link token; tests caught it).
- Global overlays in `main.tsx` (ReportUnavailableDialog, VoiceBridge, IncomingCallOverlay, MiniPlayerMount) and the root `ReauthHost` are behind the gate too. PwaHost and LiveRegions use only core keys and stay ungated.
- Dev guard: `t()` warns once per key in dev when a key is missing before the rest catalog loaded ("used before the full catalog loaded"). The new test fails on any such warning for entrance routes.
- Tests: `i18n/firstpaint.test.tsx` resets modules to get a core-only catalog and checks: 8 entrance routes render their heading with no key-shaped text and no i18n warning; `/channels` shows the busy fallback, no keys, then renders after `en.rest` loads; `useT` re-renders on growth; `isCorePath`; and a static scan that every literal `t('...')` key in the transitive static imports of `routes/entrance/*.tsx` is in the `entrance`/`a11y`/`pwa` fragments (passed with no catalog changes).

## Why this design
Route-level gate in the root layout rather than `await loadFullCatalog()` inside each `lazyNamed` import: one place covers lazy and eager non-entrance routes, the not-found page and future routes with no per-route discipline, while `lazyNamed` stays catalog-agnostic. Cost: the gate sits above the Outlet, so on a cold direct load of a Lounge URL the catalog and the lazy route chunk are fetched in series (same total time as before; the catalog request starts at boot). A possible follow-up is to start the route chunk import in parallel.

## Measurements
- Entrance chunk gzip: 209.1 KB before, 209.4 KB after (budget 300). Other budgets unchanged (+0.1-0.3 KB). `index.html` has no modulepreload of `en.rest`.
- Critical path to the login h1: before, main bundle then a second serial request for the 92 KB `en.rest` chunk; after, only the main bundle. Static preview build, Chromium, cache off, throttled to 1.6 Mbps / 100 ms latency, 4 fresh contexts each: 3.75 s before (3759/3771/3747/3748 ms) vs 2.98 s after (2981/2980/2984/2964 ms), about -0.77 s (-21%). Unthrottled localhost differences are noise.

## Tests run
tsc, eslint (src), prettier --check, full vitest `--maxWorkers=2` (142 files, 1647 tests), web build + `check-bundle-budget.mjs` (all ok), `i18n-verify-diff.mjs` (MISSING 0), static Playwright on port 4173 (18 passed). The wasm dist was copied from the main checkout (not built) for the web build.

## Still open / notes
- Entrance components still contain some hard-coded English strings (e.g. LoginLink); out of scope here.
- If a future entrance route needs a non-core key, add it to the `entrance` fragment (the scan test fails otherwise) and add the path to `corePaths.ts`.
- No owner questions.
