# Handoff: feat/p30b-ent-harness (SEC-ENT-004)

Agent ent-harness, wave 30b. Worktree `p30b-ent-harness`, branch `feat/p30b-ent-harness` (base `integration/phase30b`; `feat/p30b-ent-build` at `54b3aa04` and `feat/p30b-ent-gate` at `87f7f633` merged in). No push, no PR.

## Resume here

1. When `feat/p30b-ent-build` has its final commit: `git merge feat/p30b-ent-build` (and `feat/p30b-ent-gate`, `feat/p30b-ent-pwa` when they finish), `pnpm install --offline --frozen-lockfile`, `pnpm --filter @sideroom/wasm build`, `node scripts/build-web.mjs` (or `pnpm --filter @sideroom/web build`), then `node scripts/check-entrance-isolation.mjs` and `node --test "scripts/*.test.mjs"`.
2. Fix nothing in other agents' files: send the findings below to their owners, add a tolerated match to `scripts/entrance-allow.json` only with a real reason (never for Lounge, Cinema, Library: O-7).
3. Run the deliberate-leak proof on the real tree (see "Leak proof") and record it here.
4. Run `entrance-isolation.spec.ts` in server mode once the Postgres password file exists (`%TEMP%\sr_pg_pw.txt` was missing this session) and tick SEC-ENT-004 in `docs/inventory/21-feature-phase.md`.

## Done (committed)

- `scripts/check-entrance-isolation.mjs`: per public surface of `dist/surfaces.json` it checks the first-load budget, the allowed modules (Vite manifest against `src/public/<surface>/ALLOWED.ts`), the generated denylist (chunk references `Name-hash.js` and compound chunk names, route paths from `dist/app/routes.json`, `openapi.json` paths outside the public API, non-core catalog prefixes) plus `scripts/entrance-denylist.json`, source maps, `<link rel=preload|modulepreload>` and `import()` outside `/entrance/` and `/fonts/`, any `.wasm`, any `/app/` or `app.html` reference, files shared with `app.html`; the plain-HTML surfaces (no `<script`, no `on*=`, no `fetch(`, no preload); the public static files (`sw.js`, manifests, `offline.css`, `robots.txt`, notices, `fonts/README.md`); a top-level `dist/*.html` not in the table, a table surface with no shell (reserved surfaces skip). Output lists every surface with `ok`/`FAIL`. `--repo=<checkout>` points it at another worktree's sources.
- `scripts/entrance-denylist.json` (hand-kept; `Lounge`, `Cinema`, `Library` first per O-7, then the other section names, `MiniPlayer`, `PlayerHost`, `PlayerGate`, `PlayerSkin*`, `usePlayerStore`, `sr.player`, `mediaSession`, `Now playing`, `mini player`, `music player`, the app mounts, `ffmpeg`, `jassub`, `hls`, app path prefixes). `scripts/entrance-allow.json`: reasons mandatory (an empty reason fails); no entry for the three section names. The public API (sign-in family, registration helpers, entrance copy, invite and template lookups, `/users/@me`) is built in (`PUBLIC_API_PREFIXES`), citing design section 7.
- `scripts/check-entrance-isolation.test.mjs` (15 cases, synthetic dist: clean pass; each of Lounge/Cinema/Library fails; reasonless allow fails; app chunk, route, API path, catalog prefix leaks; `import()` outside `/entrance/`; dynamic import inside followed and scanned; source map; preload; `.wasm`; budget; module outside ALLOWED.ts; plain-HTML script/handler/fetch; shell not in the table; surface without a shell; file shared with `app.html`; `sw.js`/manifest scan). `scripts/entrance-agreement.test.mjs` (public-surfaces.json vs the Caddyfile matchers, samples reach only their own matcher, no `try_files`, the finished `dist/sw.js` routes every sample to its shell, dist `surfaces.json` equals the source). `scripts/entrance-allowed.test.mjs` (each `src/public/<surface>` imports only from its `ALLOWED.ts`; `src/main.tsx` gone). All three run in `repo-checks.yml` (the existing `node --test "scripts/*.test.mjs"` step plus a named step).
- `e2e/tests/entrance-isolation.spec.ts`: iterates `public-surfaces.json`; per surface anonymous network logs, own-files-only, no other surface's entry, smoke denylist words in the shell and loaded scripts, plain HTML loads no script; the gate (302 with `next`, hostile `next`, `/app/*` empty 401 identical for real and absent files, openapi/docs/unknown API 401 or 302, 405s, `/house-notes` no-script CSP and readable with JavaScript off); signed in (sign-in through the UI is a document load into `/app`, `/app/*` is `private` + `Vary: Cookie`, logout response carries `Clear-Site-Data` cache and storage, no `sr-app-*` cache, `/channels` is a 302 again); a real app file name read from that run is a 401 anonymously; revoked elsewhere (`logout-all` from a second context: next `/app/` fetch is 401, app caches purged, `/channels` lands on `/login`). Skips itself unless `SR_E2E_WEB=server` or `SR_E2E_ISOLATION=1`.
- `SR_E2E_WEB=server` in `e2e/playwright.config.ts` (default base URL `http://localhost:8080`, `webServer` is `sr-server` with `SR_WEB__ROOT`/`SR_WEB__SERVE_PUBLIC=true`, reuses a running one, which is how `deploy/compose/e2e-run.sh` provides it). `dev` stays default; `preview` kept as deprecated.
- `apps/web/vite/publicDev.ts` (dev middleware: surface shell by path, `/house-notes` to the server, other navigations ask `GET /api/v1/users/@me` and answer 302 `/login?next=`), `apps/web/vite/previewGate.ts` (shell by path for `vite preview`, ungated), `apps/web/vite/publicDev.test.ts` (8 Vitest cases), `e2e/vite.static.config.ts` + `playwright.static.config.ts` (the static suite previews with it, so ent-build does not have to wire the gate plugin).
- Workflows (all parse with js-yaml 4.3.2): `web.yml` runs the isolation check after the budget; `e2e.yml` new `built-bundle` job (three engines, sr-server serving dist, the check, then isolation + smoke + journey specs); `voice-soak.yml` builds first and runs `SR_E2E_WEB=server` on :8080 (the soak spec reads `SR_E2E_BASE_URL`); `prod-smoke.yml` adds `entrance-isolation` (with `SR_E2E_ISOLATION=1`, the Owner password file for the signed-in cases because the runner cannot reach the stack's database); `repo-checks.yml` names the isolation tests.
- `noindex.spec.ts` sample extended (`/app/assets/x.js`, `/channels`, `/house-notes`, `/developers/docs/api`, `/api/v1/openapi.json`): a 302/401 and the legal page must carry `X-Robots-Tag: noindex, nofollow` (ent-gate's `web.rs` sets it in four places; unverified until a live run). `entrance-smoke.spec.ts`: `/board` now expects the redirect to `/login` (the gate answers 302, not an SPA 404).
- Docs: 05 (openapi needs a principal), 10 (6.1 legal stylesheet), 12 (who serves what), 15 (T5a), `specs/entry-landing-and-login.md` section 1 (surface prose, `/help` signed in), `dev/getting-started.md` (surfaces, `SR_E2E_WEB`, dev/preview gates, denylist upkeep), `inventory/21-feature-phase.md` (Wave 30b rows SEC-ENT-001..004, all unticked).
- Sign-in specs sweep: no spec uses `waitForURL`; the specs that sign in through the form assert `toHaveURL`, which retries across the document load, and `global-setup.ts`/`signIn` use the API. Nothing to change.

## First real run (against ent-build's WIP dist at `p30b-ent-build/apps/web/dist`, 23:45, not their final)

The check works on a real build and found real problems; they belong to other agents and are not fixed here:

- ent-build, entrance copy: `entrance/surface-*.js` (shared by landing, auth and invite) carries `Cinema`, `Library` and `Lounge` (a theme map `entrance:...,cinema:...,library:...`; the copy "from the banner in the lounge"), `Staff` ("only senior staff can see it"), the call `/moderation/appeals` in every surface (BanAppeal should be auth-only) and `/sr/appearance/defaults` (not in the design's public API list: owner/ent-build to decide whether it joins section 7). `entrance/auth-*.css` carries `[data-theme^=lounge-dark]`. `public/fonts/README.md` (a public file) says "Inter (Lounge sans stand-in)". O-7 says none of the three words may appear and there is no allow entry.
- ent-pwa, `sw.js`: only its comment "SharePage parses both" trips the denylist (the file otherwise names only `/app/`, `/app.html`, `/share`, each allowed with a reason). Reword the comment or say so and I allow it.
- The legal and error surfaces and `THIRD_PARTY_NOTICES.md` split were not yet in that dist (`legal/*.html` missing; the notices file still the full one naming ffmpeg, jassub, library).
- Neither `src/public/*/ALLOWED.ts` nor the manifest entries were final: `src/public/stubs/enRest.ts` and `.html` manifest entries are treated as public by directory.

## Leak proof

- Unit level, run: `node --test scripts/check-entrance-isolation.test.mjs` plants each of Lounge, Cinema, Library, an app chunk name, route, API path and catalog key, an `import()` outside `/entrance/`, a source map, a preload outside `/entrance/` and a `.wasm` into a synthetic dist and each fails with the surface, file, token and rule (15 of 15 pass).
- Real tree, still to do (needs ent-build final): on a scratch branch add `void import('../../lounge/shell/LoungeRoot')` (one line) to `src/public/landing/main.tsx`; expect (1) the public build to fail in ent-build's `resolveId` guard, (2) with the guard bypassed, `check-entrance-isolation.mjs` to fail the landing, (3) `entrance-isolation.spec.ts` to fail on a request outside the surface (needs the server). Record the three outputs here.

## Tests run

- `node --test "scripts/*.test.mjs"`: 77 pass, 0 fail, 5 skipped (the agreement test's service-worker and dist parts and the allowed-import cases skip until a dist and `src/public` exist).
- `vitest run` (apps/web, `--maxWorkers=2`): the new `vite/publicDev.test.ts` plus the suite files vitest matched: 4 files, 30 tests pass.
- `tsc --noEmit` for `e2e/` and `apps/web/`: clean. Prettier check on every file touched: clean. ESLint on `apps/web/vite/*.ts`: clean.
- Workflow YAML parse (js-yaml 4.3.2) for web, e2e, voice-soak, prod-smoke, repo-checks: all parse. `node scripts/check-doc-links.mjs`: ok.
- Not run: Playwright (no Postgres password file in `%TEMP%`, no Garage; ports 8081/5174 never used), `cargo` (nothing in my files is Rust), the full `pnpm --filter @sideroom/web build` + bundle budget (ent-build's build; I ran the check on their existing dist instead).

## Decisions

- The check keys chunk names by file reference (`Name-hash.js`) and only matches a bare name when it is a compound identifier; lone words (`Account`, `Home`, `theme`) are English and produced hundreds of false positives on the first real run.
- An allow entry excuses a match only inside an occurrence of the allowed string (`/users/@me` excuses the `/users` prefix there and nowhere else); entries are scoped by surface (`*` for all) and optional file substring.
- The static suite gets the preview gate through `e2e/vite.static.config.ts` (an e2e file) rather than editing `apps/web/vite.config.ts` (ent-build's). `publicDev.ts` is not wired into `vite.config.ts` for the same reason: add `publicDev()` to `plugins` there at integration (one line); until then `vite dev` serves the app shell for every path.
- `built-bundle` in `e2e.yml` runs three specs, not the whole suite, so the existing 3-engine job on the dev server stays the broad net.

## Open owner questions

1. `/sr/appearance/defaults` called by the entrance: public API (add to section 7) or removed from the public surfaces?
2. A banned member's appeal page lives in the auth surface per the design; confirm it stays public (the allow entry for `/moderation/appeals` is scoped to `auth`).
3. Third-party code containing `Library`/`Lounge`/`Cinema` as a plain word (none found so far): the rule is no allow entry; confirm that holds even for vendor strings.

## Still open

- SEC-ENT-004 acceptance items needing a live run: the Web, Playwright (three engines), Rust, repo-checks, voice-soak (eight members) and prod-smoke jobs green with the new steps present in the logs; the real-tree leak proof; `prod-smoke.sh` passing `SR_WEB_DIR` to both containers is `deploy/**` (ent-gate's).
