# Handoff: feat/p5-xc (cross-cutting, wave 5)

Branch `feat/p5-xc`, based on `integration/phase4`. Not pushed, no PR. Items are in `docs/inventory/18-cross-cutting.md` (19 of 46 ticked, partial ones carry a note) plus FND-OBS-003.

## Done

| Area | What exists | Where |
|---|---|---|
| PWA | Manifest (Lounge, Cinema, Library shortcuts), offline page, hand-written service worker, update toast, Chromium install prompt and iOS "Add to Home Screen" steps, badge helper. Registered in production builds only; `vite dev` unregisters any leftover worker | `apps/web/public/sw.js`, `manifest.webmanifest`, `offline.*`, `apps/web/src/pwa/`, `apps/web/vite/pwa.ts` |
| Service worker boundaries | Never touches non-GET, cross-origin, `/api`, `/gateway`, `/cdn`, `/internal`, `/developers`, `/oauth`, `Authorization`, `Range`. Caches build output and the public app shell only. Unit-tested policy plus a browser test that lists the caches | `src/pwa/pwa.test.tsx`, `e2e/static/pwa-security.spec.ts` |
| Security headers | `sr-api` layer (nosniff, Referrer-Policy, Permissions-Policy, COOP/CORP, default-deny CSP with `frame-ancestors 'none'`, HSTS only when the public URL is https; never overrides a handler's own header). App-shell CSP from one source (`csp.ts`), enforced on `vite preview` and in `Caddyfile.prod`, report-only on `vite dev`. A test pins the Caddyfile to `csp.ts` | `crates/sr-api/src/security_headers.rs`, `apps/web/src/security/`, `deploy/compose/caddy/Caddyfile.prod` |
| CSRF / Origin review | Reviewed, no change needed: `origin_check` covers every cookie-authenticated non-safe request, CORS allows only the public origin, auth endpoints have CSRF tokens. Recorded in the threat model | `docs/15-threat-model.md` |
| Rate-limit review | Every `/api` route sits behind the default bucket (40 per 10 s) and the global limiter; auth routes also throttle inside their handlers. Gaps worth explicit buckets later: upload-slot creation, search, invite creation, reactions. Not changed (other agents own those routes) | n/a |
| i18n | ICU subset on `Intl` (arguments, number/date/time, plural with offset, selectordinal, select, apostrophe quoting), `useT()`, `useLocale()`, lazy locale loaders, `en` catalog, pseudo locales `en-XA` (accented, +35%, bracketed) and `ar-XB` (right-to-left, sets `dir`), 12/24h switch. Catalog test fails on a literal `t('key')` that does not exist, on malformed ICU, and on other catalogs whose arguments differ from English. Entrance pages, shell, passkey button, verify banner, House notes and Help are extracted | `apps/web/src/i18n/` |
| Accessibility | Skip link (first Tab stop, targets `main`), live-region announcer (`announce()`), global reduced-motion and forced-colours and high-contrast CSS, entrance contrast fix, browser axe on 9 public routes, focus-management audit of the UI primitives | `src/components/SkipLink.tsx`, `src/a11y/`, `src/styles/a11y.css`, `e2e/static/a11y.spec.ts`, `docs/dev/a11y-checklist.md` |
| Performance | Route-level splitting (Lounge, Library pages and shell, Admin, User and Server Settings) with `lazyNamed()`. Entry chunk 338 KB to 155 KB gzip. Budget script in CI: entrance 300 KB, each section 600 KB. REST latency is now a real Prometheus histogram | `src/lib/lazyRoute.tsx`, `scripts/check-bundle-budget.mjs`, `scripts/bundle-budget.json` |
| Ops | `backup.sh`, `restore.sh`, `restore-drill.sh` (run against local Postgres 17: dump, retention, checksum mismatch refused, non-empty database refused, drill), systemd service and timers, `prod.yml`, `.env.prod.example`, garage and coturn production templates, Prometheus config and alert rules, Grafana provisioning and "SIDE ROOM overview" dashboard, BACKUP, UPGRADE, RUNBOOK, GARAGE, FAQ, INSTALL pointers | `deploy/` |
| Test infrastructure | `sr_db::testing::TestDb`: a migrated database cloned from a template per test, dropped on `Drop`; flake detector script and nightly workflow | `crates/sr-db/src/testing.rs`, `scripts/flake-detect.mjs` |
| Repo automation | PR title lint, docs link checker (found no broken links), inventory progress summary, issue generator (dry run by default), dependabot sanity and commit prefixes, shellcheck, release dry run (web bundle, image build without push, compose and Caddy validation), weekly backup drill workflow, `permissions: contents: read` on all workflows, CODEOWNERS lines | `.github/`, `scripts/` |
| Entrance leftovers | House notes page with `#rules #privacy #terms #image-credits`, Help page at `/support` (both linked from the Library footer), hover micro-interaction on the three worlds. "A glimpse inside" already existed | `routes/entrance/Static.tsx` |
| Docs | Threat model, accessibility checklist, owner actions in `docs/open-questions.md` | `docs/15-threat-model.md`, `docs/dev/` |

## Not done, and why

| Item | Reason |
|---|---|
| XC-PWA-004 Web Push, XC-PWA-006 Share Target | Push needs VAPID keys and a sender (owner action); share target needs a channel picker in the Lounge (other agents' area) |
| XC-PWA-005 badge wiring, XC-PWA-007 composer keyboard, XC-A11Y-001/002 wiring | Helpers exist (`setAppBadge`, `announce`, `SkipLink`) but wiring them means editing Lounge files other agents are working in |
| XC-A11Y-004 signed-in routes | Needs a fixture session; public routes are covered |
| XC-I18N-002 migrate existing time/number formatting, RTL CSS audit | Large cross-file change; formatters exist |
| XC-I18N-003, XC-SEC-002/003/004/005, XC-PERF-003/004 | Backend features or load tooling beyond this slice |
| XC-OPS-001 end-to-end, XC-OPS-002 `install.sh`, WAL archiving | No Docker on the machine; installer needs a real host |
| XC-TEST-002/003/004/005 | Not started: gateway client utilities, MSW from OpenAPI, Playwright fixtures, visual regression |
| XC-CI-005 preview deployments | Optional |
| ENT-UI-022, ENT-UI-025, `sr-cli backup/restore` | Need account state or admin UI work; scripts cover backup |
| Live TV manifest shortcut | The route does not exist yet |

## Decisions

1. **Own service worker, no Workbox.** Keeps the dependency list unchanged and makes "never cache API/gateway/auth" a short, testable function. `/cdn` avatars, emoji and library pages are not cached by the worker even though the inventory lists them: those URLs are signed and access-controlled; HTTP cache headers already cover them.
2. **No new runtime dependencies.** i18n is about 300 lines on `Intl`. Added dev dependency: `axe-core` in `e2e`.
3. **Dev CSP is report-only** so a header can never break another agent's dev or e2e session. Enforcement is on `vite preview` (what the browser tests use) and in production.
4. **Entrance muted text** uses `--ent-muted: #8c8878` (about 5.4:1) because the calibrated `--sr-text-3` (#6f6c60) is about 3.6:1 and fails WCAG AA. The token is untouched; owner decides (see open-questions human action 4).
5. **Tests that render lazy routes** wait up to 5 s (`configure({ asyncUtilTimeout })` in `test/setup.ts`) because the first render of a code-split route waits for a cold transform.
6. **Library page `Browse`** stays in the main chunk because `validateBrowseSearch` is imported by the route table; moving it to its own file would shave more.

## For the other wave-5 agents

- **Strings:** `import { useT } from '../i18n'`, `const t = useT()`, add the key to `apps/web/src/i18n/messages/en.ts` (keys `area.screen.thing`). Settings keys `settings.user.*`, `settings.server.*` and `settings.language.*` already exist. p5-user-settings: for the Language section use `const { locale, locales, setLocale, hour12, setHour12 } = useLocale()`; the choice persists in `localStorage` (`sr.locale`, `sr.hour12`). Run the app with `en-XA` to find hard-coded strings.
- **New routes:** wrap heavy screens with `lazyNamed(() => import('./Foo'), 'Foo')` (`src/lib/lazyRoute.tsx`).
- **Voice client:** if voice signalling or media uses another origin, add it to `connect-src` in `csp.ts` **and** in `Caddyfile.prod` (the test fails until both agree), or set `SR_CSP_CONNECT` for `vite preview`.
- **Announcements:** call `announce('3 new messages')` (polite) or `announce('Call dropped', 'assertive')`; `<LiveRegions />` is mounted once in `main.tsx`.
- **Rust DB tests:** `let db = sr_db::testing::TestDb::new().await?; db.pool()`; needs `SR_TEST_DATABASE_URL` for a role that can `CREATEDB`. The template database `sr_tpl_*` persists between runs.
- **Service worker and e2e:** it is not registered under `vite dev`.

## Verification

| Check | Result |
|---|---|
| `cargo fmt --all --check`, `clippy --workspace --all-targets -D warnings`, `cargo deny check` | clean |
| `cargo test --workspace` | 1092 passed, 0 failed, 109 ignored (database suites) |
| `sr-db` ephemeral database tests (`--ignored`, local Postgres 17) | 2 passed |
| Web: tsc, eslint, prettier, vitest | clean; 664 tests in 60 files |
| `node --test scripts` | 5 passed |
| Browser suite `e2e/static` (production build, Chromium) | 18 passed (9 axe routes, skip link, landmarks, reduced motion, forced colours, headers, CSP, manifest, service worker, offline) |
| `check-bundle-budget`, `check-ops-files`, `check-doc-links`, `check-dependabot` | ok |
| Not run | Docker (compose, Caddy validation), GitHub Actions (all workflows), `pnpm audit`, `shellcheck`, Firefox and WebKit |

Local setup used and removed: throwaway databases `sr_xc_src` and the drill database were dropped; the `sr_tpl_*` template was dropped; no server processes were left running.
