# Handoff: feat/p30b-ent-build (wave 30b, SEC-ENT-001)

Branch `feat/p30b-ent-build`, base `integration/phase30b`. One item: SEC-ENT-001, the public build with one entry per surface and the gated app build (`docs/dev/entrance-isolation.md`, D-385).

## Resume here
Everything is committed. Nothing is in progress. Remaining work is listed under "Still open".

## Dist layout contract (fixed by the design; held to)
```
dist/
  index.html auth.html invite.html [profile.html later]   public shells (one per bundle surface)
  entrance/…                                              public build output (JS/CSS, legal-<hash>.css, door image)
  .vite/manifest.json                                     public build manifest
  fonts/ icons/ sw.js manifest.webmanifest offline.html offline.css robots.txt 404.html 50x.html   public files
  THIRD_PARTY_NOTICES.md                                  public copy (split-notices.mjs)
  legal/house-notes.<locale>.html                         templates with {{operator_country}} / {{operator_contact}}
  surfaces.json                                           copy of apps/web/public-surfaces.json (reserved surfaces keep "reserved": true, no shell)
  app.html                                                the gated app shell
  app/                                                    gated: hashed assets, manifest.webmanifest, THIRD_PARTY_NOTICES.md (full)
  app/csp.txt                                             the app CSP, one line, sent verbatim by sr-server
  app/routes.json                                         JSON array of the app router's route paths
```
Public build base `/`, assets in `/entrance/`; app build base `/app/`. `dist` root has no `app-manifest.webmanifest`.

## Merges done on this branch (for the integrator)
- `feat/p30b-ent-gate` (df7b49f5) was merged (no conflicts) to get the new `Caddyfile.prod` and `web.rs`.
- `feat/p30b-ent-pwa` was NOT merged. Its non-test files that the build needs were checked out into this branch with identical content (so the later merge is clean): `apps/web/public/{sw.js,manifest.webmanifest,app-manifest.webmanifest,404.html,50x.html}`, `src/pwa/{purge.ts,PublicUpdateToast.tsx,pwa.css}`, `vite/pwa.ts`, and its three test files. One edit on top: the `sw.js` comment at line 87 no longer names `SharePage` (the isolation check flagged the word).
- `feat/p30b-ent-harness` was not merged. Its `scripts/check-entrance-isolation.mjs` was run from its worktree against this branch's dist (`node <harness>/scripts/check-entrance-isolation.mjs apps/web/dist --repo=.`): every public surface is clean. `vite/publicDev.ts` is not on this branch: **add `publicDev()` to the plugins of `apps/web/vite.config.ts` at integration**. Until then `vite/devShells.ts` (mine) serves the shells by path in `vite dev` without a gate decision; delete it once `publicDev` is in.

## Done
- `apps/web/public-surfaces.json` (first commit).
- Two builds: `vite.public.config.ts` and `vite.config.ts` (app), run by `scripts/build-web.mjs` (`pnpm --filter @sideroom/web build` calls it). `pwaPlugin()` runs in the public build; `vite/appContract.ts` writes `csp.txt` and `routes.json` (evaluates `routePaths()` of `router.tsx` in Vite's SSR loader), copies the gated manifest, moves `app.html` up.
- Guard `vite/surfaceGuard.ts`: `resolveId` stubs `i18n/messages/en.rest` for the public build and refuses any `src/` module in no surface's list; `buildEnd` walks each shell's module graph (static and dynamic) and fails naming surface, module and importer. `ALLOWED.ts` per surface; `public/allowed.test.ts` checks the same rule on the import graph.
- Surfaces `src/public/{landing,auth,invite}/{main,router}.tsx` over `src/public/surface.tsx`. Cross-surface navigation is a plain `<a>` or `nav.assign/replace` (document load). `profile` is reserved for wave 34.
- Neutral public theme (O-7): `vite/neutralTheme.ts` gives `@sideroom/ui` a one-theme table in the public build. The public surfaces no longer call `/sr/appearance/defaults` and no longer read saved theme prefs.
- App: `src/app/main.tsx` (installs the 401 exit), `src/app/AppDoor.tsx` (`/` and `/app` go to the last section or a safe `?goto=`), `AppServerInvite`/`AppServerTemplate`. The public `ServerInvite`/`ServerTemplate` show a visitor only the login link and hand a member to `/app?goto=/invite/<code>` (or `/template/<code>`). `router.tsx` has no public route; `/help` and `/help/whats-new` stay app routes (behind login).
- `lib/enterApp.ts`: `safeNext` (same rule as the server), `enterApp`, `exitToLogin`, `nav`. All sign-in success paths and `useRedirectIfLoggedIn` end in `enterApp()`. API client `setUnauthorizedHandler`; the app's handler probes `/users/@me` and leaves for `/login?next=` only on a second 401. App pages that linked `/login` use `exitToLogin`/anchors. App logout awaits `purgeAppCaches()` before `logout`/`logout-all` (`settings/user/api.ts`).
- O-7: landing has no section teasers, strip, "glimpse" button, icons, CSS or catalog keys.
- Help behind login: `components/SignInHelp.tsx` (collapsible, under the sign-in form); the Help page links `/app/THIRD_PARTY_NOTICES.md`.
- Ban appeal stays public, on the auth surface only (`api/appeal.ts`).
- Catalog: app-only entrance strings moved to `i18n/messages/en/p30b-ent-build-1.ts`; the few new strings public pages need are in `entrance.ts` (public build registers entrance/a11y/pwa only). Public copy reworded: no "staff"/"lounge".
- `scripts/render-legal.mjs`, `scripts/split-notices.mjs`, `scripts/check-bundle-budget.mjs` (per-surface rows, `app` over `app.html`, no `.wasm` in public closures, pseudo rule over `dist/entrance`), `scripts/bundle-budget.json` (`public` rows).
- `csp.ts`: `PUBLIC_CSP`, `NO_SCRIPT_CSP`, `renderSurfacesBlock`, `renderCaddyfile(caddyfile, table)` rewriting only the `# surfaces:begin/end` block; `deploy/scripts/render-caddyfile.mjs` calls it (`--check` passes on ent-gate's Caddyfile). `csp.test.ts` pins the block, the shell policies, and equality with the Rust `PUBLIC_CSP`/`NO_SCRIPT_CSP`.
- Measured, gzip first load: landing 113.6 KB, auth 119.2 KB, invite 111.4 KB, app 236.7 KB. No `.wasm` or section word in any public file.

## Still open
- `lastSection` is never written anywhere (as before), so `/app` always opens `/channels`.
- The D-239 admin `house-notes` copy key no longer reaches a page (legal is build-time HTML).
- `DiscordCallback` ends in a server redirect; where `sr-api` sends a signed-in member afterwards (should be `/app`) was not checked (not my file).
- `PublicUpdateToast` pulls ~0.5 KB of unused share-page CSS (ent-pwa noted it).
- Not run: Playwright, Rust, Caddy (not allowed or not installed).

## Tests run
- `tsc --noEmit`, `eslint .`, prettier on touched files: clean.
- Vitest `--maxWorkers=2` on router, public, lib, i18n, help, library, settings, routes, pwa, components, security: all green (new: `public/surfaces.test.tsx`, `public/allowed.test.ts`, `lib/enterApp.test.ts`; rewritten: `router.test.tsx`, `routes/entrance/static.test.tsx`, `security/csp.test.ts`). A full-suite run was started last; if it showed failures they are listed in the lead's report.
- `node scripts/build-web.mjs`, `check-bundle-budget.mjs`, `check-css-tokens.mjs`, `check-notices-copy.mjs`, `render-caddyfile.mjs --check`: pass. Harness isolation check on this dist: clean.

## Decisions
- Cross-surface links are `<a>`/`nav.*`, never router links.
- `@sideroom/ui` is neutralised by a build transform, not by editing the package.
- New core strings went into `entrance.ts`, not a new fragment (the public build registers only the core fragments).

## Open owner questions
1. The D-239 admin-editable `house-notes` copy has nowhere to show. Drop the key, or have `sr-server` substitute it into the template?
2. Should `/app` remember the last section across visits?
