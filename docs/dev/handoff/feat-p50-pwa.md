# Wave 50 handoff: pwa (SEC-ENT-002 / SEC-ENT-004)

Branch `feat/p50-pwa`. No UI change (so no visual sweep), no new strings, no migration, no dependency.

## Found first

`e2e/static/pwa-security.spec.ts` already existed (wave 30b) and `web.yml` already runs it (`static-browser-checks`,
`pnpm --filter @sideroom/e2e test:static`, Chromium, on the `web-dist` artifact). The inventory and the p40 handoff said
"in no workflow" because they looked under `e2e/tests/` and in `e2e.yml`. What was really missing was coverage of
several things the entrance design lists, so the spec was extended rather than created.

## Done

- `e2e/static/pwa-security.spec.ts`: five new checks on the real two-build dist, in addition to the seven there:
  worker scope is the origin root and an anonymous visitor has exactly one cache `sr-static-<build>`; the worker's
  precache list holds no `/app` file and no `.wasm`; the public pages (landing, invite, login, register,
  no-invitation, first pass and a pass under the worker) request and cache no `/app/*`, `app.html` or `.wasm`; the
  signed-in manifest `/app/manifest.webmanifest` shares `id`, `scope`, `start_url` with the public one and adds
  shortcuts, every public shell links `/manifest.webmanifest` and `app.html` links the app one; the plain error pages
  carry no script, no link into the app and a way back. The purge on sign-out stays in `pwa-update.spec.ts`
  (`PURGE_APP` drill and the signed-in update drill).
- `e2e/tests/pwa-update.spec.ts` flake: the app pages of the drill have no API behind them, so the app leaves for
  `/login` on its own at an unannounced moment. Two races came from that: an in-flight `page.evaluate` (cache reads
  inside `expect.poll`, which does not retry on a thrown error) died with "Execution context was destroyed", and a
  `page.goto('/channels')` was reported as "interrupted by another navigation". Fix, no timeout bump: `steady()`
  (waits for the `load` event, repeats an idempotent read cut short by a navigation), `visitApp()` (repeats a goto
  that lost the race), and `waitForWaitingWorker()` shared by both deploy drills (the anonymous drill called a bare
  `reg.update()` that races the new server's start and the browser's own update check; the first drill already
  polled for this).
- `.github/workflows/voice-soak.yml`: the two missing SEC-ENT-004 steps: `check-entrance-isolation.mjs` on the soak's
  dist, and the anonymous and gate cases of `entrance-isolation.spec.ts` (`--grep-invert "signed in"`, Chromium)
  against the sr-server the soak uses, before the soak. The signed-in cases are left out on purpose: they register
  accounts and the soak needs its allowance of 20 per address and day.

## Inventory

SEC-ENT-002 and SEC-ENT-004 stay unticked; both rows now say "Waits on: the first CI run of ...".

## Local environment note

`packages/wasm/dist` copied into the worktrees lacks the `music` and `tools` modules (the web build fails with
"Could not resolve ../dist/tools/tools.js"). I built them with `wasm-pack` (release, `--no-opt`) inside the lock.
Other builders hit the same wall; the commander's copy source should be rebuilt.

## Tests run

Inside the lock, on a fresh `node scripts/build-web.mjs` (two builds) of this branch:
- `node scripts/check-entrance-isolation.mjs`: every public surface clean.
- `playwright test -c playwright.static.config.ts` (Chromium, `vite preview` with `e2e/vite.static.config.ts`): 26 passed (axe 14, pwa-security 12 incl. the 5 new).
- `pwa-update.spec.ts`, Chromium, `--repeat-each=4` (no API, a temporary config without global setup, deleted): 12/12 passed. The race is not reproducible on a quiet machine, so this proves no regression rather than the fix; the fix follows from the failure modes named above.
- All workflows parse with js-yaml 4.3.2; `scripts/check-ops-files.mjs` ok; `playwright test --list --project chromium` loads 903 tests in 67 files; prettier --check and `tsc --noEmit` on e2e clean (eslint is not installed in e2e).
- Not run: `voice-soak.yml` itself (owner dispatch, needs the SFU stack), `entrance-isolation.spec.ts --grep-invert "signed in"` against sr-server (needs the API stack; it is the spec e2e.yml's built-bundle job already runs in full).

## Owner questions

None.
