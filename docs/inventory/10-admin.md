# 10 — Staff Server & Admin (`ADM`)

Phase 3. Spec: [admin-staff-server](../specs/admin-staff-server.md), docs 06, 07.

## ADM-STF — Staff mechanics
- [ ] **ADM-STF-001** [DB] `is_staff_guild` partial unique index, `roles.platform_permissions`, staff_appointments, appointment_delegations, staff_access_log, platform_audit_log
- [ ] **ADM-STF-002** [BE] Platform permission resolver (active appointment ∧ staff-guild member ∧ role bits; Owner = all)
- [ ] **ADM-STF-003** [BE] Block server invites to the staff guild; hide Invite to Server there
- [ ] **ADM-STF-004** [BE] Appointment API (appoint, accept/decline via system DM, revoke, delegate)
- [ ] **ADM-STF-005** [BE] Instant revocation on leave/kick (drop staff claims, close admin WS subscriptions)
- [ ] **ADM-STF-006** [BE] Staff-guild seed (channels, roles) in bootstrap
- [ ] **ADM-STF-007** [BE] System user + bot-style card poster for staff channels (components with action handlers)
- [ ] **ADM-STF-008** [BE] Hash-chained access log writer + nightly verifier job + alert
- [ ] **ADM-STF-009** [BE] Re-auth (2FA within 15 min) middleware for sensitive admin routes
- [ ] **ADM-STF-010** [FE] Staff Server rail frame, "SIDE ROOM Staff" profile badge
- [ ] **ADM-STF-011** [FE] "Platform Admin" entry in Staff Server dropdown and user menu

## ADM-UI — Admin pages shell
- [ ] **ADM-UI-001** [FE] `/admin` full-screen layout with grouped nav filtered by platform permissions
- [ ] **ADM-UI-002** [FE] Shared admin table component (server-side pagination, filters, column chooser, CSV export)
- [ ] **ADM-UI-003** [FE] Reason-required action modal pattern + audit toast

## ADM-DASH — Overview
- [ ] **ADM-DASH-001** [FE] Dashboard tiles: accounts vs cap gauge, online now, voice rooms + egress, storage used, job backlog, review queue counts, donations vs hosting cost, recent alerts
- [ ] **ADM-DASH-002** [BE] Stats endpoint (cached 30s)
- [ ] **ADM-DASH-003** [FE] Setup Wizard checklist (SMTP test, Discord OAuth, wallet, storage test, voice/TURN test, default limits)

## ADM-PPL — People
- [ ] **ADM-PPL-001** [FE] Accounts search (username, email, id, Discord id, IP), state filter
- [ ] **ADM-PPL-002** [FE] Account drawer: profile, state, Person link, sessions, IP history, invited by, invites issued, servers, entitlements, flags, audit trail
- [ ] **ADM-PPL-003** [BE] Account actions: deactivate, reactivate (cap check), force password reset, reset 2FA, rename, change email, badges, force logout
- [ ] **ADM-PPL-004** [FE] Persons view: account chain timeline, link/unlink accounts, notes, suspicion level with history
- [ ] **ADM-PPL-005** [BE] Platform bans (reason category serious/standard, note) applied to Person, unban
- [ ] **ADM-PPL-006** [BE] Inviter suspicion propagation on serious bans + thresholds (freeze, escalate) + clear
- [ ] **ADM-PPL-007** [FE] Platform Bans page
- [ ] **ADM-PPL-008** [FE] Staff page (list, roles, appointed by/at, appoint modal, revoke, delegations (Owner))

## ADM-GRAPH — Invite graph
- [ ] **ADM-GRAPH-001** [BE] Graph API (subtree, ancestors path, stats per node, search)
- [ ] **ADM-GRAPH-002** [WASM] Force-directed layout for large graphs (Rust, Barnes–Hut) → positions
- [ ] **ADM-GRAPH-003** [FE] WebGL graph renderer (pan/zoom, collapse subtrees, node colors by state, suspicion halo, banned marker)
- [ ] **ADM-GRAPH-004** [FE] Node side panel (account summary, open codes, credits ledger, grant/revoke credit, freeze invites, clear suspicion)
- [ ] **ADM-GRAPH-005** [FE] Tree/list fallback view + path-to-root breadcrumb

## ADM-CNT — Content admin
- [ ] **ADM-CNT-001** [FE] Review Queues hub (tabs: Cinema, Library, Apps, Packages, Directory) → REV items
- [ ] **ADM-CNT-002** [FE] Cinema Catalog editor (→ CIN-ADM)
- [ ] **ADM-CNT-003** [FE] Library Catalog editor (→ LIB-ADM)
- [ ] **ADM-CNT-004** [FE] Collections & Spotlight: curated rows CRUD (name, items, order, section, schedule), hero picks with carousel order, reading lists featured
- [ ] **ADM-CNT-005** [FE] Ingest Sources: watched folders (path, owner account, kind, enabled, last scan), storage imports wizard, naming tester
- [ ] **ADM-CNT-006** [FE] Transcoding: workers list (name, GPU, status, throughput), create/revoke worker tokens, queue with priorities, failed jobs with logs, re-run

## ADM-COM — Community
- [ ] **ADM-COM-001** [FE] Server Directory approvals (request detail, preview server, approve/deny with note, categories manager, featured)
- [ ] **ADM-COM-002** [FE] Servers search (all guilds), metadata view, transfer ownership, delete
- [ ] **ADM-COM-003** [FE] Private Content Viewer (channel/DM picker, reason + 2FA gate, read-only message list incl. deleted ≤ 30d and edit history, banner "Access is logged")
- [ ] **ADM-COM-004** [BE] Private content read API (deleted + edits), logging every request

## ADM-PLT — Platform settings
- [ ] **ADM-PLT-001** [FE] Features page (all modules, platform settings via SchemaForm, kill switches)
- [ ] **ADM-PLT-002** [FE] Limits page (cap, initial credits, account-age for inviting, credit price, upload limits, message length, rate limits, retention windows, submission rate limits)
- [ ] **ADM-PLT-003** [FE] Entrance page editor (copy blocks, hero image, house notes markdown, no-invitation text)
- [ ] **ADM-PLT-004** [FE] Themes & Defaults (default theme per section)
- [ ] **ADM-PLT-005** [FE] Email & Integrations (SMTP + test send, Discord OAuth app id/secret, GIF provider key, metadata provider keys, unfurl allow/deny lists)
- [ ] **ADM-PLT-006** [FE] System page (health, versions, backups last run/verify, disk, Garage status, coturn status)
- [ ] **ADM-PLT-007** [FE] Platform Audit Log viewer
- [ ] **ADM-PLT-008** [FE] Access Log viewer + chain verification status
- [ ] **ADM-PLT-009** [BE] Platform announcements (banner to all users, optional DM from system user)

## ADM-ALR — Alerts
- [ ] **ADM-ALR-001** [BE] Alert bus → `#alerts` channel cards (suspicion, payments anomalies, transcode failures, disk > 80%, backup failure, access-log chain break, banned-Person registration attempt)
- [ ] **ADM-ALR-002** [BE] `#new-members` and `#donations` feeds
- [ ] **ADM-ALR-003** [BE] Optional email alerts to Owner
