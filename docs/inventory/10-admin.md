# 10 — Staff Server & Admin (`ADM`)

Phase 3. Spec: [admin-staff-server](../specs/admin-staff-server.md), docs 06, 07.

## ADM-STF — Staff mechanics
- [ ] **ADM-STF-001** [DB] `is_staff_guild` partial unique index, `roles.platform_permissions`, staff_appointments, appointment_delegations, staff_access_log, platform_audit_log
- [x] **ADM-STF-002** [BE] Platform permission resolver (active appointment ∧ staff-guild member ∧ role bits; Owner = all)
- [ ] **ADM-STF-003** [BE] Block server invites to the staff guild; hide Invite to Server there
- [ ] **ADM-STF-004** [BE] Appointment API (appoint, accept/decline via system DM, revoke, delegate)
- [ ] **ADM-STF-005** [BE] Instant revocation on leave/kick (drop staff claims, close admin WS subscriptions)
- [x] **ADM-STF-006** [BE] Staff-guild seed (channels, roles) in bootstrap
- [ ] **ADM-STF-007** [BE] System user + bot-style card poster for staff channels (components with action handlers)
- [x] **ADM-STF-008** [BE] Hash-chained access log writer + nightly verifier job + alert
- [ ] **ADM-STF-009** [BE] Re-auth (2FA within 15 min) middleware for sensitive admin routes
- [x] **ADM-STF-010** [FE] Staff Server rail frame, "SIDE ROOM Staff" profile badge *(rail frame and STAFF badge in `lounge/GuildRail.tsx`, driven by `is_staff_guild` on the guild payload; the API does not send that field yet and there is no profile badge, see handoff admin-fe)*
- [x] **ADM-STF-011** [FE] "Platform Admin" entry in Staff Server dropdown and user menu *(`lounge/shell/StatusMenu.tsx`, the server menus and `platformStaff.ts`; test in `lounge/__tests__/homeFlows.test.tsx`)*

## ADM-UI — Admin pages shell
- [x] **ADM-UI-001** [FE] `/admin` full-screen layout with grouped nav filtered by platform permissions
- [x] **ADM-UI-002** [FE] Shared admin table component (server-side pagination, filters, column chooser, CSV export) *(`admin/DataTable.tsx`: cursor paging, filter slot, column chooser, CSV export; used by Accounts, Library catalog and Platform audit log)*
- [x] **ADM-UI-003** [FE] Reason-required action modal pattern + audit toast

## ADM-DASH — Overview
- [ ] **ADM-DASH-001** [FE] Dashboard tiles: accounts vs cap gauge, online now, voice rooms + egress, storage used, job backlog, review queue counts, donations vs hosting cost, recent alerts *(partly built: review queue sizes, donations against hosting cost, alerts derived from probes, dead jobs, invoices and the chain check; voice rooms, egress and storage used need a stats route)*
- [ ] **ADM-DASH-002** [BE] Stats endpoint (cached 30s)
- [ ] **ADM-DASH-003** [FE] Setup Wizard checklist (SMTP test, Discord OAuth, wallet, storage test, voice/TURN test, default limits) *(partly built: checklist for storage, wallet, limits and staff; SMTP, Discord OAuth and voice/TURN have no status or test routes)*

## ADM-PPL — People
- [ ] **ADM-PPL-001** [FE] Accounts search (username, email, id, Discord id, IP), state filter *(partly built: username, email, id and state; Discord id and IP search need server support)*
- [ ] **ADM-PPL-002** [FE] Account drawer: profile, state, Person link, sessions, IP history, invited by, invites issued, servers, entitlements, flags, audit trail *(partly built: profile, state, Person link, recent sign-ins, invited by, invites, credits, entitlements, flags, audit trail; sessions list and servers need routes)*
- [ ] **ADM-PPL-003** [BE] Account actions: deactivate, reactivate (cap check), force password reset, reset 2FA, rename, change email, badges, force logout
- [ ] **ADM-PPL-004** [FE] Persons view: account chain timeline, link/unlink accounts, notes, suspicion level with history *(partly built: `PersonDrawer.tsx` shows the account chain, ban, notes, suspicion and its log history; link/unlink, note editing and suspicion history need routes)*
- [x] **ADM-PPL-005** [BE] Platform bans (reason category serious/standard, note) applied to Person, unban
- [x] **ADM-PPL-006** [BE] Inviter suspicion propagation on serious bans + thresholds (freeze, escalate) + clear
- [x] **ADM-PPL-007** [FE] Platform Bans page
- [ ] **ADM-PPL-008** [FE] Staff page (list, roles, appointed by/at, appoint modal, revoke, delegations (Owner)) *(partly built: list, roles, appointed by/at, appoint, revoke; delegations (Owner) need routes)*

## ADM-GRAPH — Invite graph
- [x] **ADM-GRAPH-001** [BE] Graph API (subtree, ancestors path, stats per node, search)
- [ ] **ADM-GRAPH-002** [WASM] Force-directed layout for large graphs (Rust, Barnes–Hut) → positions
- [ ] **ADM-GRAPH-003** [FE] WebGL graph renderer (pan/zoom, collapse subtrees, node colors by state, suspicion halo, banned marker)
- [ ] **ADM-GRAPH-004** [FE] Node side panel (account summary, open codes, credits ledger, grant/revoke credit, freeze invites, clear suspicion) *(partly built: summary, open codes, credits ledger, grant/revoke credit, clear suspicion; freeze invites needs a route)*
- [x] **ADM-GRAPH-005** [FE] Tree/list fallback view + path-to-root breadcrumb *(`admin/sections/InviteGraph.tsx`: collapsible tree, flat list and path-to-root breadcrumb)*

## ADM-CNT — Content admin
- [ ] **ADM-CNT-001** [FE] Review Queues hub (tabs: Cinema, Library, Apps, Packages, Directory) → REV items *(partly built: `ReviewHubSection`: Cinema reports, Library and Apps tabs work; Packages and Directory show a not-built note)*
- [x] **ADM-CNT-002** [FE] Cinema Catalog editor (→ CIN-ADM) *(built as CIN-ADM-001 in `admin/cinema/`)*
- [ ] **ADM-CNT-003** [FE] Library Catalog editor (→ LIB-ADM) *(partly built: `admin/library/`: series form, editions, chapters, publish, merge, delete; page reorder/replace/re-convert are not built)*
- [ ] **ADM-CNT-004** [FE] Collections & Spotlight: curated rows CRUD (name, items, order, section, schedule), hero picks with carousel order, reading lists featured
- [ ] **ADM-CNT-005** [FE] Ingest Sources: watched folders (path, owner account, kind, enabled, last scan), storage imports wizard, naming tester
- [x] **ADM-CNT-006** [FE] Transcoding: workers list (name, GPU, status, throughput), create/revoke worker tokens, queue with priorities, failed jobs with logs, re-run *(`admin/cinema/TranscodingSection.tsx` now talks to the real `/admin/transcode/*` routes and shows queue counts and priority; worker throughput is not reported by the API)*

## ADM-COM — Community
- [ ] **ADM-COM-001** [FE] Server Directory approvals (request detail, preview server, approve/deny with note, categories manager, featured)
- [ ] **ADM-COM-002** [FE] Servers search (all guilds), metadata view, transfer ownership, delete
- [ ] **ADM-COM-003** [FE] Private Content Viewer (channel/DM picker, reason + 2FA gate, read-only message list incl. deleted ≤ 30d and edit history, banner "Access is logged")
- [ ] **ADM-COM-004** [BE] Private content read API (deleted + edits), logging every request

## ADM-PLT — Platform settings
- [x] **ADM-PLT-001** [FE] Features page (all modules, platform settings via SchemaForm, kill switches) *(`admin/sections/Features.tsx` kill switches plus `admin/SchemaForm.tsx` platform settings from the feature schema)*
- [ ] **ADM-PLT-002** [FE] Limits page (cap, initial credits, account-age for inviting, credit price, upload limits, message length, rate limits, retention windows, submission rate limits) *(partly built: every setting the server registers is editable; rate-limit and submission-rate-limit settings are not in the registry yet)*
- [ ] **ADM-PLT-003** [FE] Entrance page editor (copy blocks, hero image, house notes markdown, no-invitation text)
- [ ] **ADM-PLT-004** [FE] Themes & Defaults (default theme per section)
- [ ] **ADM-PLT-005** [FE] Email & Integrations (SMTP + test send, Discord OAuth app id/secret, GIF provider key, metadata provider keys, unfurl allow/deny lists)
- [ ] **ADM-PLT-006** [FE] System page (health, versions, backups last run/verify, disk, Garage status, coturn status)
- [x] **ADM-PLT-007** [FE] Platform Audit Log viewer *(`admin/sections/AuditLog.tsx`: the `admin_action` rows of the staff access log)*
- [x] **ADM-PLT-008** [FE] Access Log viewer + chain verification status
- [ ] **ADM-PLT-009** [BE] Platform announcements (banner to all users, optional DM from system user)

## ADM-ALR — Alerts
- [ ] **ADM-ALR-001** [BE] Alert bus → `#alerts` channel cards (suspicion, payments anomalies, transcode failures, disk > 80%, backup failure, access-log chain break, banned-Person registration attempt)
- [ ] **ADM-ALR-002** [BE] `#new-members` and `#donations` feeds
- [ ] **ADM-ALR-003** [BE] Optional email alerts to Owner
