# 18 — Cross-cutting (`XC`)

All phases.

## XC-PWA — Progressive Web App
- [x] **XC-PWA-001** [FE] Web app manifest (name, icons, theme colors per section, shortcuts: Lounge, Cinema, Library, Live TV) _(Live TV shortcut waits for its route.)_
- [x] **XC-PWA-002** [FE] Service worker (Workbox): app-shell precache, versioned updates with "Update available" prompt, runtime caches (avatars, emoji, library pages), never cache API mutations _(Own small worker, no Workbox. /cdn avatars, emoji and pages are deliberately not cached: they are signed and access-controlled. See apps/web/public/sw.js.)_
- [x] **XC-PWA-003** [FE] Install prompt UX (desktop + Android) and iOS "Add to Home Screen" instructions
- [x] **XC-PWA-004** [FE] Web Push subscription UI + permission flow + per-device toggle _(Frontend only: per-device switch in Settings, Notifications (pwa/WebPushCard.tsx), typed client pwa/push.ts, `push` and `notificationclick` handlers in sw.js. The server has no push routes yet, so it shows "This server has not turned on push notifications yet" until `GET /push/vapid-key` and `POST/DELETE /users/@me/push-subscriptions` exist.)_
- [x] **XC-PWA-005** [FE] Badging API unread counts _(pwa/useAppBadge.ts mirrors total @mentions from the Lounge read states into the app icon badge, honouring the "unread badge" preference. Mentions only; no count-less dot for plain unread.)_
- [x] **XC-PWA-006** [FE] Share Target (share images/links into SIDE ROOM → pick channel) _(Text and links only: manifest `share_target` (GET) opens /share, the user picks a channel or DM and the text lands in that composer draft. Shared image files are not accepted yet; that needs a POST target handled in the service worker.)_
- [x] **XC-PWA-007** [FE] Safe-area insets, viewport handling, iOS keyboard handling in composer _(lounge/useVisualViewport.ts sizes the Lounge to the visual viewport while the keyboard is open and hides the mobile tab bar; the tab bar honours the bottom inset. Verified by unit test only, not on a real iOS device.)_

## XC-A11Y — Accessibility
- [x] **XC-A11Y-001** [FE] Keyboard navigation map for every section (roving tabindex in lists/grids, skip links) _(The map is in docs/dev/a11y-checklist.md. Roving tabindex (a11y/useRovingTabindex.ts) is applied to the server rail and the channel list; the virtualised member list and message list, Library and Cinema grids keep their own key handling and are listed in the map as follow-ups. Skip link is still only on the entrance shell.)_
- [x] **XC-A11Y-002** [FE] ARIA live regions (new messages, toasts, call events) _(New messages from others in the open channel and voice join/leave/mute/deafen/reconnect/drop now call announce(). Toasts already carry their own live role.)_
- [x] **XC-A11Y-003** [FE] Focus management in modals/popouts/menus _(Audited: Modal, Drawer, Menu and Popover trap focus, restore it and close on Escape, with tests.)_
- [x] **XC-A11Y-004** [TEST] axe CI checks on all routes + manual screen-reader pass checklist (NVDA, VoiceOver) _(Browser axe covers the public routes; jsdom axe over signed-in routes is table-driven in apps/web/src/a11y/signedInRoutes.test.tsx (Lounge home, five Settings sections, Share) next to the existing Library, Cinema, Admin and Developers suites. Contrast and landmarks on signed-in screens still need a browser run with a fixture user (XC-TEST-004). Manual pass: docs/dev/a11y-checklist.md.)_

## XC-I18N — Internationalization
- [x] **XC-I18N-001** [FE] i18n framework (ICU messages), string extraction, English source catalog _(apps/web/src/i18n; entrance extracted. Settings nav keys are defined but the settings screens still use literals.)_
- [ ] **XC-I18N-002** [FE] Locale-aware dates/numbers, 12/24h, RTL layout readiness _(Intl formatters and a 12/24h switch exist in the i18n runtime; existing screens are not migrated and RTL CSS is not audited.)_
- [ ] **XC-I18N-003** [BE] Localized emails and system messages

## XC-SEC — Security
- [x] **XC-SEC-001** [BE] CSP, HSTS, Permissions-Policy, COOP/COEP where needed (WASM threads), X-Content-Type-Options _(sr-api layer, Caddyfile.prod, Vite dev and preview. COEP is not needed yet: no SharedArrayBuffer.)_
- [x] **XC-SEC-002** [BE] Upload content sniffing + MIME allowlist per surface, SVG sanitization or rejection _(crates/sr-storage/src/upload/surface.rs: per-surface allowlist; SVG/HTML/XML rejected for inline images, kept as octet-stream downloads for attachments, no sanitizer by design. Video/library ingest keep their own decoders.)_
- [x] **XC-SEC-003** [BE] SSRF-safe HTTP client shared crate (unfurls, RSS, metadata lookup, webhooks out) _(new crate sr-net; unfurl, image proxy and payments use it. Discord OAuth talks only to fixed discord.com URLs and keeps its own no-redirect client. Outbound webhooks do not exist yet.)_
- [x] **XC-SEC-004** [BE] Secrets management (file-based, rotation docs), token hashing everywhere _(SR_*_FILE secrets, credential-column test, rotation docs in doc 12 section 3.1. Invite and template codes stay plaintext by design.)_
- [x] **XC-SEC-005** [BE] Audit of permission checks (test matrix generator per route) _(partial: crates/sr-api/tests/route_matrix.tsv with a 401 test for every authenticated route; AUTHZ column is heuristic-seeded from path/security and needs owner review; it is not a per-handler proof of each permission bit.)_
- [x] **XC-SEC-006** [OPS] Dependency audit in CI (`cargo-deny`, `pnpm audit`) _(The pnpm audit job is advisory and has not run on GitHub yet.)_
- [x] **XC-SEC-007** [DOC] Threat model document (accounts, invites, payments, E2EE, staff access) _(docs/15-threat-model.md)_

## XC-PERF — Performance
- [x] **XC-PERF-001** [FE] Route-level code splitting (Lounge, Cinema, Library, Admin, Developer bundles) _(Lounge, Library pages, Admin, both Settings screens.)_
- [x] **XC-PERF-002** [FE] Performance budgets (JS < 300 KB gz for Entrance, < 600 KB for Lounge initial) enforced in CI _(scripts/check-bundle-budget.mjs, wired into web.yml.)_
- [ ] **XC-PERF-003** [BE] Load test suite (k6 or Rust `goose`): 2,000 concurrent gateway sessions, 200 msg/s, REST p95 < 100 ms
- [x] **XC-PERF-004** [BE] DB query review: EXPLAIN on hot paths, indexes per doc 04 §13 _(EXPLAIN harness `crates/sr-db/tests/explain_hot_paths.rs` (`just explain-hot-paths`) found no seq scan on a large table, so no migration was added. Message search is Tantivy, not SQL; member search is covered. See doc 04 section 13.)_

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
- [x] **XC-TEST-002** [TEST] Gateway test client utilities (connect as N users, await dispatches) _(`sr_gateway::test_support` (feature `test-support`): TestWorld, TestClient, connect_users, await_dispatch. In-process, no sockets; real-WebSocket clients are not included.)_
- [x] **XC-TEST-003** [TEST] Frontend MSW mocks generated from OpenAPI _(`packages/api-client/scripts/gen-msw.mjs` emits one handler per operation with schema-shaped placeholder bodies; `mocks:check` is the drift check, wired into the OpenAPI drift workflow. Existing hand-written handlers in apps/web/src/test/server.ts were not migrated.)_
- [x] **XC-TEST-004** [TEST] Playwright fixtures (seeded users, logged-in contexts, multi-user scenarios) _(`e2e/support/fixtures.ts`: seedUser, signedIn, scenario.ownerAndMember. Typechecks and lists; not run against a live stack in this wave. TOTP and passkey users are not covered by signedIn.)_
- [ ] **XC-TEST-005** [TEST] Visual regression snapshots for Entrance/Cinema/Library vs mockups

## XC-CI — Repo automation
- [x] **XC-CI-001** [OPS] Issue generation script: inventory items → GitHub issues (labels by area/phase, milestone by phase) _(Dry run by default; the owner runs it with --create.)_
- [x] **XC-CI-002** [OPS] PR title lint (must contain inventory ID or `docs:`/`chore:`)
- [x] **XC-CI-003** [OPS] Docs link checker
- [x] **XC-CI-004** [OPS] Decisions-log guard (changes to 01-decisions-log.md require owner review via CODEOWNERS) _(CODEOWNERS lines added; making them binding is the "Require review from Code Owners" branch rule, an owner action.)_
- [ ] **XC-CI-005** [OPS] Preview deployments for PRs (optional, ephemeral compose on a runner)
- [x] **XC-CI-006** [OPS] Inventory progress summary in PR checks

## XC-MOD — Moderation hooks reserved now (full build in Phase 7)
- [x] **XC-MOD-001** [DB] moderation_holds + hold_id on messages/attachments + reason_category on platform bans
- [x] **XC-MOD-002** [FE] Report menu items present but behind `moderation.reports` flag (off)
- [x] **XC-MOD-003** [DOC] Phase 7 spec placeholder (reports, cases, takedowns, data-subject requests)
