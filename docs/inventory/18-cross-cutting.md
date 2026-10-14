# 18 — Cross-cutting (`XC`)

All phases.

## XC-PWA — Progressive Web App
- [x] **XC-PWA-001** [FE] Web app manifest (name, icons, theme colors per section, shortcuts: Lounge, Cinema, Library, Live TV) _(Live TV shortcut waits for its route.)_
- [x] **XC-PWA-002** [FE] Service worker (Workbox): app-shell precache, versioned updates with "Update available" prompt, runtime caches (avatars, emoji, library pages), never cache API mutations _(Own small worker, no Workbox. /cdn avatars, emoji and pages are deliberately not cached: they are signed and access-controlled. See apps/web/public/sw.js.)_
- [x] **XC-PWA-003** [FE] Install prompt UX (desktop + Android) and iOS "Add to Home Screen" instructions
- [ ] **XC-PWA-004** [FE] Web Push subscription UI + permission flow + per-device toggle
- [ ] **XC-PWA-005** [FE] Badging API unread counts _(Helper pwa/badge.ts exists; wiring it to the Lounge unread store is open.)_
- [ ] **XC-PWA-006** [FE] Share Target (share images/links into SIDE ROOM → pick channel)
- [ ] **XC-PWA-007** [FE] Safe-area insets, viewport handling, iOS keyboard handling in composer _(viewport-fit=cover and safe-area padding on the PWA prompts are in; composer keyboard handling is open.)_

## XC-A11Y — Accessibility
- [ ] **XC-A11Y-001** [FE] Keyboard navigation map for every section (roving tabindex in lists/grids, skip links) _(Skip link on the entrance pages; the per-section keyboard map is open.)_
- [ ] **XC-A11Y-002** [FE] ARIA live regions (new messages, toasts, call events) _(announce() and the live regions exist; the message list and call events do not call them yet.)_
- [x] **XC-A11Y-003** [FE] Focus management in modals/popouts/menus _(Audited: Modal, Drawer, Menu and Popover trap focus, restore it and close on Escape, with tests.)_
- [ ] **XC-A11Y-004** [TEST] axe CI checks on all routes + manual screen-reader pass checklist (NVDA, VoiceOver) _(Browser axe covers the public routes; signed-in routes are open. Manual pass: docs/dev/a11y-checklist.md.)_

## XC-I18N — Internationalization
- [x] **XC-I18N-001** [FE] i18n framework (ICU messages), string extraction, English source catalog _(apps/web/src/i18n; entrance extracted. Settings nav keys are defined but the settings screens still use literals.)_
- [x] **XC-I18N-002** [FE] Locale-aware dates/numbers, 12/24h, RTL layout readiness _(Screens format dates, numbers, currency and clock times through formatDate/formatDateTime/formatTime/formatNumber in apps/web/src/i18n, which follow the UI language, the Language region and the 12/24 hour preference (settings/user/prefs.ts keeps them in step). Hand-built plurals in the extracted modules are ICU plurals. Physical CSS margin/padding/border/text-align/float/offset properties in apps/web, packages/ui and packages/markdown-react are logical properties; 15 left/right offsets stay physical where a transform centres the box or a keyframe slides it. Pseudo-RTL (ar-XB, dir=rtl) smoke and a CSS regression guard are in i18n/format.test.tsx. Not done: shorthand 4-value margins/paddings, icon mirroring, entrance screens still hold some literals.)_
- [x] **XC-I18N-003** [BE] Localized emails and system messages _(Email text is a locale-keyed catalog in crates/sr-api/src/routes/auth/mail_text.rs; mail.rs builds layout and escaping from it, and every sender passes the account locale (BCP 47, primary-language match, English fallback per message). English only today; adding a language is one Catalog entry. Chat system messages (pins, joins) are structured events rendered by the client from its catalog, so the server has no system text to translate.)_

## XC-SEC — Security
- [x] **XC-SEC-001** [BE] CSP, HSTS, Permissions-Policy, COOP/COEP where needed (WASM threads), X-Content-Type-Options _(sr-api layer, Caddyfile.prod, Vite dev and preview. COEP is not needed yet: no SharedArrayBuffer.)_
- [ ] **XC-SEC-002** [BE] Upload content sniffing + MIME allowlist per surface, SVG sanitization or rejection
- [ ] **XC-SEC-003** [BE] SSRF-safe HTTP client shared crate (unfurls, RSS, metadata lookup, webhooks out)
- [ ] **XC-SEC-004** [BE] Secrets management (file-based, rotation docs), token hashing everywhere
- [ ] **XC-SEC-005** [BE] Audit of permission checks (test matrix generator per route)
- [x] **XC-SEC-006** [OPS] Dependency audit in CI (`cargo-deny`, `pnpm audit`) _(The pnpm audit job is advisory and has not run on GitHub yet.)_
- [x] **XC-SEC-007** [DOC] Threat model document (accounts, invites, payments, E2EE, staff access) _(docs/15-threat-model.md)_

## XC-PERF — Performance
- [x] **XC-PERF-001** [FE] Route-level code splitting (Lounge, Cinema, Library, Admin, Developer bundles) _(Lounge, Library pages, Admin, both Settings screens.)_
- [x] **XC-PERF-002** [FE] Performance budgets (JS < 300 KB gz for Entrance, < 600 KB for Lounge initial) enforced in CI _(scripts/check-bundle-budget.mjs, wired into web.yml.)_
- [ ] **XC-PERF-003** [BE] Load test suite (k6 or Rust `goose`): 2,000 concurrent gateway sessions, 200 msg/s, REST p95 < 100 ms
- [ ] **XC-PERF-004** [BE] DB query review: EXPLAIN on hot paths, indexes per doc 04 §13

## XC-OPS — Operations
- [ ] **XC-OPS-001** [OPS] Production compose + systemd units + Caddyfile (incl. activities wildcard) _(prod.yml, systemd units and Caddyfile.prod exist but are untested end to end; the activities wildcard is not included.)_
- [ ] **XC-OPS-002** [OPS] `install.sh` interactive installer
- [ ] **XC-OPS-003** [OPS] Backups (pg_dump + WAL archive, rustic snapshots) + restore drill in CI _(pg_dump, rustic snapshots, retention and a weekly CI drill are in; WAL archiving is not.)_
- [x] **XC-OPS-004** [OPS] coturn config template with REST secret _(Template only; not run against a real coturn.)_
- [x] **XC-OPS-005** [OPS] Garage cluster config (single node + multi-node guide)
- [x] **XC-OPS-006** [DOC] `deploy/INSTALL.md`, `deploy/UPGRADE.md`, `deploy/BACKUP.md`, operator FAQ _(BACKUP, UPGRADE, RUNBOOK, GARAGE and FAQ under deploy/.)_
- [x] **XC-OPS-007** [OPS] Grafana dashboards JSON _(deploy/monitoring; metric names are checked against the server by scripts/check-ops-files.mjs.)_

## XC-TEST — Testing infrastructure
- [ ] **XC-TEST-001** [TEST] Rust test harness: ephemeral Postgres per test (template DB), Garage test bucket, fake clock _(sr_db::testing::TestDb gives each test a template-cloned database; the Garage bucket helper is not included.)_
- [ ] **XC-TEST-002** [TEST] Gateway test client utilities (connect as N users, await dispatches)
- [ ] **XC-TEST-003** [TEST] Frontend MSW mocks generated from OpenAPI
- [ ] **XC-TEST-004** [TEST] Playwright fixtures (seeded users, logged-in contexts, multi-user scenarios)
- [ ] **XC-TEST-005** [TEST] Visual regression snapshots for Entrance/Cinema/Library vs mockups

## XC-CI — Repo automation
- [x] **XC-CI-001** [OPS] Issue generation script: inventory items → GitHub issues (labels by area/phase, milestone by phase) _(Dry run by default; the owner runs it with --create.)_
- [x] **XC-CI-002** [OPS] PR title lint (must contain inventory ID or `docs:`/`chore:`)
- [x] **XC-CI-003** [OPS] Docs link checker
- [x] **XC-CI-004** [OPS] Decisions-log guard (changes to 01-decisions-log.md require owner review via CODEOWNERS) _(CODEOWNERS lines added; making them binding is the "Require review from Code Owners" branch rule, an owner action.)_
- [ ] **XC-CI-005** [OPS] Preview deployments for PRs (optional, ephemeral compose on a runner)
- [x] **XC-CI-006** [OPS] Inventory progress summary in PR checks

## XC-MOD — Moderation hooks reserved now (full build in Phase 7)
- [ ] **XC-MOD-001** [DB] moderation_holds + hold_id on messages/attachments + reason_category on platform bans
- [ ] **XC-MOD-002** [FE] Report menu items present but behind `moderation.reports` flag (off)
- [ ] **XC-MOD-003** [DOC] Phase 7 spec placeholder (reports, cases, takedowns, data-subject requests)
