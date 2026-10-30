# Integration handoff: integration/phase30b (wave 30b, entrance isolation, D-385)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p30b-integ`, branch `integration/phase30b`, base 40d3235e (wave 30 plus
the design branch `docs/p30-entrance`), then origin/main 3f79f4f9 (PR #60) merged in. Env: `CARGO_TARGET_DIR=
C:/Users/braxt/Desktop/SIDEROOM_wt/p30b-ent-gate-target`, `CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`, one cargo command at a
time. No Postgres (the password file `%TEMP%\sr_pg_pw.txt` is missing), no servers, no Playwright. Nothing pushed, no
PR, nothing merged to main.

## Resume here

Everything that needs no database, browser or Caddy is done and committed (see Gates). What remains is CI, under
"Left for CI". Nothing is half-finished.

## Merges (each `--no-ff`, its own commit)

1. `feat/p30b-ent-build` (d32b35a9, SEC-ENT-001), 4fb9ee87: clean. It already carried ent-gate and copies of
   ent-pwa's non-test files.
2. `feat/p30b-ent-gate` (df7b49f5, SEC-ENT-003): already contained in ent-build ("Already up to date"), so there is
   no separate merge commit.
3. `feat/p30b-ent-pwa` (30306261, SEC-ENT-002), 469d973e: clean. `sw.js` keeps ent-build's comment rewording (no
   `SharePage`).
4. `feat/p30b-ent-harness` (022a247c, SEC-ENT-004), 3803fac6: clean.
5. `feat/p30b-mus-launch` (f3e6a003, MUS-PLY-002), b6efe3e8: clean. The rail imports only `player/presence.ts`;
   no player word reaches a public file (the isolation check's denylist names `PlayerHost`, `sr.player`, `mini
   player` and the rest), and the bundle budget's `player` row is green over the app's first load.
6. `origin/main` (3f79f4f9, PR #60: the player spec locator, the authz-matrix music seeds and track-edit body, the
   deny.toml note), 55ef99e6: clean (`authz_matrix.rs` was not touched by 30b).

## Integration changes (each its own commit)

- 8ddff33e `publicDev()` wired into `apps/web/vite.config.ts`; ent-build's interim `vite/devShells.ts` deleted
  (nothing else used it). `launch.test.tsx` (mus-launch) navigated the app router to `/login`, which is no longer an
  app route (tsc failure); it now leaves for `/help/whats-new`, which `player/scope.ts` also treats as no-player.
- dca8a932 CHANGELOG `## Phase 30b - 2026-10-30`; the mus-launch bullet moved there from Phase 30. The
  `handbook.test.tsx` version check (`/^Phase \d+$/`) now accepts a wave letter.
- 0cf814a4, three real faults found by the gates on the integrated dist:
  - **`sw.js` did not understand `public-surfaces.json`.** It matched only the design's `$code` form, while the
    table uses Caddy syntax (`/join/*`). So `/join/*`, `/login/*`, `/reset/*`, `/verify-email/*`, `/invite/*` and
    `/template/*` were app navigations to the worker: their pages would be stored under `sr-app-*` (an anonymous
    visitor creating an app cache) and fall back to `app.html`. A trailing `*` is now a prefix, exactly as
    `pattern_matches` in `web.rs`; a `pwa.test.tsx` case pins it.
  - `entrance-agreement.test.mjs` loaded `dist/sw.js` with `require()`; `apps/web` is `"type": "module"`, so the
    worker's CommonJS export was lost and the test failed before it could find the fault above. It now runs the
    script in a `vm` context.
  - `readAllowedModules` did not follow `...LANDING` spreads, so auth and invite looked like they imported
    `public/surface` without leave (`entrance-allowed.test.mjs` failed). It follows the spread import now; a test
    case pins it.
- 85bcd35a **The public build guard did nothing on Windows.** `surfaceGuard.ts` compared Rollup ids (always
  forward slashes) against a prefix built with `path.sep`, so `inSrc()` was false for every module and neither the
  `resolveId` refusal nor the `buildEnd` walk ran. Linux CI was not affected, but every local "the build passed"
  on Windows this wave proved nothing about the guard. Found by the leak proof below; the clean build still passes
  with the guard really on.

## Leak proof (SEC-ENT-004, scratch branch `scratch/p30b-leak-proof`, deleted afterwards)

One line appended to `apps/web/src/public/landing/main.tsx`: `void import('../../routes/LoungeRoot');`

1. Build guard, `node scripts/build-web.mjs`: exit 1.
   ```
   [commonjs--resolver] [plugin sideroom-surface-guard] public build: routes/LoungeRoot.tsx is not in any public
   surface's ALLOWED.ts (imported by public/landing/main.tsx). A public surface may not contain app code (D-385).
   ```
   (Before 85bcd35a the same build failed only by accident, on `worker.format` "iife" in `voice/e2ee/attach.ts`.)
2. Guard bypassed (plugin removed and `worker: { format: 'es' }` added in the scratch config only), build exit 0,
   then `node scripts/check-entrance-isolation.mjs apps/web/dist`: exit 1.
   ```
   FAIL landing   225 problems
   FAIL auth      43 problems
   FAIL invite    43 problems
   skip profile   reserved (wave 34), no shell yet
   ok   legal     1 document, no script
   ok   error     3 documents, no script
   FAIL static    7 problems
   Entrance isolation: 318 problem(s). A public page may carry only its own code (D-385).
     surface landing | entrance/LoungeRoot-DFM35hfg.js | ageGate | denylist (chunk: dist/app chunk name) ...
     surface landing | entrance/runtime-BjVNb-2l.js | src/voice/e2ee/runtime.ts | rule: module outside src/public/landing/ALLOWED.ts
     surface landing | entrance/e2ee-CAsufnOs.js | ../../packages/wasm/dist/e2ee/e2ee.js | rule: module outside ...
     ... route paths from dist/app/routes.json (/cinema/..., /library/..., /admin, /settings/...) ...
   ```
3. `entrance-isolation.spec.ts` against the leak: not run (needs `sr-server` and Playwright; CI).

The scratch branch was deleted, the clean dist rebuilt and the check is green again.

## Handoff notes applied

- ent-build: `publicDev()` added, `devShells.ts` removed (above).
- ent-pwa's six points: all present on the merged tree (manifest links in the three public shells and `app.html`,
  `PublicUpdateToast` and `registerServiceWorker()` in `src/public/surface.tsx`, the `pwa/*` modules in the landing
  `ALLOWED.ts` and through the spread in auth and invite, `purgeAppCaches()` before both logout calls in
  `settings/user/api.ts`, `moveAppManifest` in `pwaPlugin`, no `app-manifest.webmanifest` in the `dist` root, which
  the isolation check asserts).
- ent-harness's first-run findings (Cinema/Library/Lounge in the surface chunk, `/moderation/appeals` everywhere,
  `/sr/appearance/defaults`, `fonts/README.md`, the `SharePage` comment): none remain; the check on the integrated
  dist is clean with no new allow entry.
- ent-gate's contract (the `# surfaces:begin/end` block, `csp.test.ts` pinning it and the Rust `PUBLIC_CSP`):
  `render-caddyfile.mjs --check` and `csp.test.ts` pass.

## Gates (all on the merged tree, after the last merge)

| Gate | Result |
|---|---|
| web `tsc --noEmit`, `eslint .`, `prettier --check .` | clean |
| e2e `tsc --noEmit` | clean |
| Vitest, whole web package, `--maxWorkers=2` | 355 files, 3114 tests pass (then `src/help`, `src/pwa` again after the last edits) |
| `node scripts/build-web.mjs` | ok (both builds, render-legal, split-notices) |
| `check-bundle-budget.mjs` | ok: landing 113.6, auth 119.2, invite 111.4, app 236.7 KB gzip; `player`, `ffmpeg`, `music`, `pseudo` rows ok |
| `check-css-tokens.mjs`, `check-notices-copy.mjs` | ok |
| `check-entrance-isolation.mjs apps/web/dist` | every surface ok (landing, auth, invite, legal, error, static; profile reserved) |
| `node --test "scripts/*.test.mjs"` | 82 pass, 0 fail, 0 skipped |
| `deploy/scripts/render-caddyfile.mjs --check` | ok |
| Workflow YAML, js-yaml 4.3.2 | all 18 files parse |
| `i18n-audit.mjs` | TOTAL 0 |
| `check-doc-links.mjs` | ok |
| `cargo fmt --check` sr-core, sr-api, sr-preflight, sr-server | clean |
| `cargo clippy --all-targets -D warnings` same four | clean (sr-server re-run after the main merge) |
| `cargo test -p sr-server --test web_gate` | 18 pass |
| `cargo test -p sr-api --test route_matrix --test openapi_names` | 6 + 2 pass |

## Left for CI

- Postgres suites: `authz_matrix` (with main's new music seeds), `immutable_rules`, sr-preflight `pg`, the rest of
  the Rust job.
- Playwright, three engines: `entrance-isolation.spec.ts` (server mode, `SR_E2E_WEB=server`), `pwa-update.spec.ts`
  and `static/pwa-security.spec.ts` on the real two-build dist, `entrance-smoke`, `noindex` (the `X-Robots-Tag` on the
  gate's 302/401), the `built-bundle` job of `e2e.yml`, `player.spec.ts`.
- voice-soak (eight members) in server mode; prod-smoke with the `entrance-isolation` step, through Caddy (watch
  `handle_errors` keeping the status code, and the dotted-name `path_regexp`).
- The Web and repo-checks jobs with the new isolation steps visible in their logs.

## Owner's hosting steps (also `deploy/UPGRADE.md`, "the web app is served by the server")

1. Compose: take the new `compose/prod.yml`; `SR_WEB_DIR` is now mounted read-only into `sr-server` too, with
   `SR_WEB__ROOT=/srv/web`. Plain binary: `SR_WEB__ROOT=/opt/sideroom/web` in `server.env` (the unit sets it).
2. Re-render the Caddyfile (`node deploy/scripts/render-caddyfile.mjs --public-url https://<domain> ...`) or take the
   release's file; `$dc up -d --force-recreate caddy`.
3. Unpack the web build before restarting the server: it refuses to start without `app/csp.txt`. `sr-cli preflight`
   shows the new `web` row.
4. Check signed out: `/channels` is `302 /login?next=/channels` and `/app/anything.js` an empty `401`; signed in, the
   app opens.

## Open owner questions (already collected; not re-asked)

1. Should an OAuth2 bearer token be allowed to read `openapi.json`? Today: session or bot only (a one-line
   `auth/policy.rs` change plus a matrix cell).
2. The gate's `429` bucket (240 `401`s per address per minute) is a fixed default, not an operator setting.
3. The D-239 admin-editable `house-notes` copy no longer reaches a page (legal is build-time HTML): drop the key, or
   have `sr-server` substitute it into the template?
4. Should `/app` remember the member's last section? `lastSection` is never written, so `/app` always opens
   `/channels`.

Also from the agents' handoffs, for the record: ent-harness asked whether `/sr/appearance/defaults` is public API
(moot now: the public surfaces no longer call it), whether the ban appeal stays public on the auth surface (as
designed; its allow entry is scoped to `auth`), and whether vendor strings with a section word get an allow entry
(none found; the rule stays "no entry"). mus-launch would like a music-note icon in `@sideroom/ui`.

## Follow-ups (new work, not blockers)

- `player/scope.ts` still calls `/help` a public path; it is an app route now. Harmless (no player on help pages)
  but the comment is stale.
- `PublicUpdateToast` pulls about 0.5 KB of unused share-page CSS from `pwa.css`.
- Where `sr-api` sends a member after `DiscordCallback` (should be `/app`) was not checked.
- `route_matrix.tsv` has no row type for `/app`, `/app/{*path}` and `/house-notes`; `web_gate.rs` pins them.
