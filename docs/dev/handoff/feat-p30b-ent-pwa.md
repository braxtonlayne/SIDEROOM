# Handoff: feat/p30b-ent-pwa (wave 30b, SEC-ENT-002)

Branch `feat/p30b-ent-pwa`, base `integration/phase30b`. One item: SEC-ENT-002, the service worker and the manifests of the entrance isolation design (`docs/dev/entrance-isolation.md` section 5, D-385). Only the files of the ent-pwa row of the Split table were touched.

## Done

- `apps/web/public/sw.js`: two caches. `sr-static-<build>` precaches the public shells (from the injected surface table), `offline.html`, `404.html`, `50x.html` and the public assets the shells reference. `sr-app-<build>` holds `/app/*` cache-first on first use and `/app.html` as the signed-in navigation fallback; the worker never requests an `/app/` URL on its own.
  - Classification is bypass by default: only a public file, a public surface page, an `/app/*` file, a navigation or the Share Target POST is taken. `BYPASS_PREFIXES` is gone, so no API, gateway, media or developer path is named. A navigation to a path with a file extension is left to the browser.
  - Navigation: network first. A public path is stored as, and falls back to, its surface's shell, then `/offline.html`; any other page uses the signed-in shell, then `/offline.html`. The `id="root"` shell mark rule stays; plain pages (legal, offline, errors) are never stored as shells.
  - Purge of every `sr-app-*` cache on `PURGE_APP` (answers `PURGED` on the message port, or to `event.source`), on a `401`, an unfollowed redirect (`opaqueredirect`) or a followed redirect to `/login` for an app file or an app navigation, and on activation. `sr-static-<current>` is never deleted.
  - Push default URL and notification click default are `/app`; a payload `url` is used only when it starts with a single `/`.
  - Placeholders: `__SR_BUILD__`, `/*__SR_PRECACHE__*/ []`, `/*__SR_SURFACES__*/ []`.
- `apps/web/vite/pwa.ts`: `pwaPlugin()` now calls the exported `finishServiceWorker(dist, surfacesFile)`. It reads every public shell that exists in `dist`, builds the precache list (never an `/app/` file), the build id (hash over all shells, the list and the table) and the surface table, and calls `moveAppManifest(dist)`, which moves `dist/app-manifest.webmanifest` to `dist/app/manifest.webmanifest` so it never stays in the public root. Surfaces come from `apps/web/public-surfaces.json` when present (array or `{ surfaces: [...] }`; `shell` or `html`, `paths` or `path`), else `DEFAULT_SURFACES` (the design's section 2 table).
- `apps/web/public/manifest.webmanifest`: `start_url: /app`, no `shortcuts`. `apps/web/public/app-manifest.webmanifest`: the same plus the shortcuts (same `id`, `scope`, icons, `share_target`). `404.html`, `50x.html` added (plain HTML, no script, entrance palette through `offline.css`); `offline.html` unchanged.
- `apps/web/src/pwa/purge.ts`: `purgeAppCaches()` posts `PURGE_APP` with a reply port to the controlling worker, waits for `PURGED` (1.5 s timeout), then deletes `sr-app-*` from the page too. Never rejects.
- `apps/web/src/pwa/PublicUpdateToast.tsx` (plus two button rules in `pwa.css`): the update toast for public surfaces, same `pwa.update.*` strings (already in the `pwa` core catalog, so no new catalog fragment), no install prompt.
- Tests: `swFetch.test.ts` (rewritten for the two caches: install precache, shell per surface, an anonymous visitor never creates `sr-app-*`, offline fallbacks, 401 / unfollowed redirect / redirect-to-login purges, a 401 on a public page leaves the app cache alone, `PURGE_APP`, activation purge), `pwa.test.tsx` (classification, path patterns, manifests, no section words in the worker, manifest or plain pages), new `entrancePwa.test.tsx` (purge helper, toast, build step, surface table forms). `e2e/tests/pwa-update.spec.ts` (port 4174 default; the two new drills plus a real-browser `PURGE_APP` test; lays out the public shells itself when `dist` is still the single-build layout) and `e2e/static/pwa-security.spec.ts` (neutral manifest, no section words in worker, offline and error pages, no `sr-app-*` for an anonymous visitor, signed-in manifest not served from the root).

## For ent-build to apply (shells and app)

1. Every public shell (`index.html`, `auth.html`, `invite.html`, `profile.html`): `<link rel="manifest" href="/manifest.webmanifest" />` (as today).
2. `app.html`: `<link rel="manifest" href="/app/manifest.webmanifest" />`.
3. Toast mount, in each `src/public/<surface>/main.tsx`, beside the router (and call `registerServiceWorker()` from `pwa/register`): `import { PublicUpdateToast } from '../../pwa/PublicUpdateToast';` and render `<PublicUpdateToast />` next to the router provider. Add `pwa/PublicUpdateToast`, `pwa/state`, `pwa/register` and `pwa/pwa.css` to each surface's `ALLOWED.ts`.
4. App logout (the sign-out call in `settings/user/api.ts` or wherever it is posted): `await purgeAppCaches()` from `src/pwa/purge` before `POST /sr/auth/logout`. That file is not in my row.
5. Build wiring: the public build keeps `pwaPlugin()`. `public/app-manifest.webmanifest` is copied to the public `dist` root by Vite's public dir and then moved by the plugin into `dist/app/`; make sure the app build (or `build-web.mjs`) does not empty `dist/app/` afterwards, or call `moveAppManifest(dist)` after both builds. `check-entrance-isolation.mjs` should assert there is no `app-manifest.webmanifest` in the `dist` root.
6. `PwaHost` stays in the app build only; its `QUIET` path list is now moot.

## Tests run (all on this branch)

- `vitest run src/pwa src/styles --maxWorkers=2`: 65 tests green.
- `tsc --noEmit` for `apps/web` and `e2e`; `eslint src/pwa vite/pwa.ts`; prettier check on all touched files; `node scripts/check-css-tokens.mjs` and `node scripts/check-bundle-budget.mjs` green on a fresh `vite build`.
- Playwright, Chromium, `vite preview` on 4174 only: `static/pwa-security.spec.ts` 7/7 and `tests/pwa-update.spec.ts` 3/3, both with the spec laying out the shells itself and with a `dist` that already had the extra shells. These ran on the current single-build `dist` with `auth.html`, `invite.html` and `app.html` made as copies of `index.html` and `sw.js` finished from the template (not committed; `dist` is ignored). They must be re-run on the real two-build output once ent-build merges.
- The wasm package was not buildable (no cargo): `packages/wasm/dist` was copied from the `p30-integ` worktree (ignored build output) so that `vite build` could run.

## Still open / notes

- The `/help/whats-new` step of `pwa-update.spec.ts` runs only when the spec had to synthesise the layout; in the real layout that page is app-only and belongs to ent-harness's signed-in run.
- `pwa-security.spec.ts` no longer asserts `wasm-unsafe-eval` or the microphone permission on `/` (the public policy drops them); ent-harness's entrance spec should assert the public CSP precisely.
- `pwa.css` still carries the share-page rules, so the public closure pulls about 0.5 KB of unused CSS; splitting it is a small follow-up.
- A navigation fetch returns `opaqueredirect`, so the worker cannot see where a redirect goes; it treats any such response to an app navigation as a lost session. The design has no other redirect for app paths.

## Owner questions

None new. The install prompt stays app-only as designed.

## Resume here

Nothing in progress. Next: after ent-build merges, rerun `pwa-security` and `pwa-update` on the real `dist` (port 4174) and apply the six points above.
