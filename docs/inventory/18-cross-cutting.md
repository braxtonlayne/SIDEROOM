# 18 — Cross-cutting (`XC`)

All phases.

## XC-PWA — Progressive Web App
- [ ] **XC-PWA-001** [FE] Web app manifest (name, icons, theme colors per section, shortcuts: Lounge, Cinema, Library, Live TV)
- [ ] **XC-PWA-002** [FE] Service worker (Workbox): app-shell precache, versioned updates with "Update available" prompt, runtime caches (avatars, emoji, library pages), never cache API mutations
- [ ] **XC-PWA-003** [FE] Install prompt UX (desktop + Android) and iOS "Add to Home Screen" instructions
- [ ] **XC-PWA-004** [FE] Web Push subscription UI + permission flow + per-device toggle
- [ ] **XC-PWA-005** [FE] Badging API unread counts
- [ ] **XC-PWA-006** [FE] Share Target (share images/links into SIDE ROOM → pick channel)
- [ ] **XC-PWA-007** [FE] Safe-area insets, viewport handling, iOS keyboard handling in composer

## XC-A11Y — Accessibility
- [ ] **XC-A11Y-001** [FE] Keyboard navigation map for every section (roving tabindex in lists/grids, skip links)
- [ ] **XC-A11Y-002** [FE] ARIA live regions (new messages, toasts, call events)
- [ ] **XC-A11Y-003** [FE] Focus management in modals/popouts/menus
- [ ] **XC-A11Y-004** [TEST] axe CI checks on all routes + manual screen-reader pass checklist (NVDA, VoiceOver)

## XC-I18N — Internationalization
- [ ] **XC-I18N-001** [FE] i18n framework (ICU messages), string extraction, English source catalog
- [ ] **XC-I18N-002** [FE] Locale-aware dates/numbers, 12/24h, RTL layout readiness
- [ ] **XC-I18N-003** [BE] Localized emails and system messages

## XC-SEC — Security
- [ ] **XC-SEC-001** [BE] CSP, HSTS, Permissions-Policy, COOP/COEP where needed (WASM threads), X-Content-Type-Options
- [ ] **XC-SEC-002** [BE] Upload content sniffing + MIME allowlist per surface, SVG sanitization or rejection
- [ ] **XC-SEC-003** [BE] SSRF-safe HTTP client shared crate (unfurls, RSS, metadata lookup, webhooks out)
- [ ] **XC-SEC-004** [BE] Secrets management (file-based, rotation docs), token hashing everywhere
- [ ] **XC-SEC-005** [BE] Audit of permission checks (test matrix generator per route)
- [ ] **XC-SEC-006** [OPS] Dependency audit in CI (`cargo-deny`, `pnpm audit`)
- [ ] **XC-SEC-007** [DOC] Threat model document (accounts, invites, payments, E2EE, staff access)

## XC-PERF — Performance
- [ ] **XC-PERF-001** [FE] Route-level code splitting (Lounge, Cinema, Library, Admin, Developer bundles)
- [ ] **XC-PERF-002** [FE] Performance budgets (JS < 300 KB gz for Entrance, < 600 KB for Lounge initial) enforced in CI
- [ ] **XC-PERF-003** [BE] Load test suite (k6 or Rust `goose`): 2,000 concurrent gateway sessions, 200 msg/s, REST p95 < 100 ms
- [ ] **XC-PERF-004** [BE] DB query review: EXPLAIN on hot paths, indexes per doc 04 §13

## XC-OPS — Operations
- [ ] **XC-OPS-001** [OPS] Production compose + systemd units + Caddyfile (incl. activities wildcard)
- [ ] **XC-OPS-002** [OPS] `install.sh` interactive installer
- [ ] **XC-OPS-003** [OPS] Backups (pg_dump + WAL archive, rustic snapshots) + restore drill in CI
- [ ] **XC-OPS-004** [OPS] coturn config template with REST secret
- [ ] **XC-OPS-005** [OPS] Garage cluster config (single node + multi-node guide)
- [ ] **XC-OPS-006** [DOC] `deploy/INSTALL.md`, `deploy/UPGRADE.md`, `deploy/BACKUP.md`, operator FAQ
- [ ] **XC-OPS-007** [OPS] Grafana dashboards JSON

## XC-TEST — Testing infrastructure
- [ ] **XC-TEST-001** [TEST] Rust test harness: ephemeral Postgres per test (template DB), Garage test bucket, fake clock
- [ ] **XC-TEST-002** [TEST] Gateway test client utilities (connect as N users, await dispatches)
- [ ] **XC-TEST-003** [TEST] Frontend MSW mocks generated from OpenAPI
- [ ] **XC-TEST-004** [TEST] Playwright fixtures (seeded users, logged-in contexts, multi-user scenarios)
- [ ] **XC-TEST-005** [TEST] Visual regression snapshots for Entrance/Cinema/Library vs mockups

## XC-CI — Repo automation
- [ ] **XC-CI-001** [OPS] Issue generation script: inventory items → GitHub issues (labels by area/phase, milestone by phase)
- [ ] **XC-CI-002** [OPS] PR title lint (must contain inventory ID or `docs:`/`chore:`)
- [ ] **XC-CI-003** [OPS] Docs link checker
- [ ] **XC-CI-004** [OPS] Decisions-log guard (changes to 01-decisions-log.md require owner review via CODEOWNERS)
- [ ] **XC-CI-005** [OPS] Preview deployments for PRs (optional, ephemeral compose on a runner)
- [ ] **XC-CI-006** [OPS] Inventory progress summary in PR checks

## XC-MOD — Moderation hooks reserved now (full build in Phase 7)
- [ ] **XC-MOD-001** [DB] moderation_holds + hold_id on messages/attachments + reason_category on platform bans
- [ ] **XC-MOD-002** [FE] Report menu items present but behind `moderation.reports` flag (off)
- [ ] **XC-MOD-003** [DOC] Phase 7 spec placeholder (reports, cases, takedowns, data-subject requests)
