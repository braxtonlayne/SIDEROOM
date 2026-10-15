# 10 — Staff Server & Admin (`ADM`)

Phase 3. Spec: [admin-staff-server](../specs/admin-staff-server.md), docs 06, 07.

## ADM-STF — Staff mechanics
- [x] **ADM-STF-001** [DB] `is_staff_guild` partial unique index, `roles.platform_permissions`, staff_appointments, appointment_delegations, staff_access_log, platform_audit_log _Migrations 0010 (is_staff_guild unique index, roles.platform_permissions), 0042, 0130 and 0650 (appointment_delegations, staff_appointment_offers, staff_alerts, platform_announcements)._
- [x] **ADM-STF-002** [BE] Platform permission resolver (active appointment ∧ staff-guild member ∧ role bits; Owner = all)
- [x] **ADM-STF-003** [BE] Block server invites to the staff guild; hide Invite to Server there _Create and redeem both refuse the Staff Server: SettingsError::StaffServerInvites, tests in settings/tests.rs and sr-db tests/invite_join.rs. Hiding the Invite to Server button is a client job._
- [x] **ADM-STF-004** [BE] Appointment API (appoint, accept/decline via system DM, revoke, delegate) _Offers: POST/GET/DELETE /admin/staff/offers, appointee answers POST /staff/offers/{id}/respond from the system DM (Accept/Decline buttons carry sr:offer-accept:/sr:offer-decline:), delegations PUT/DELETE /admin/staff/{id}/delegation. sr-admin service_ext.rs, tests_ext.rs, admin/tests.rs. The DM button click handler is a client job._
- [x] **ADM-STF-005** [BE] Instant revocation on leave/kick (drop staff claims, close admin WS subscriptions) _Trigger sr_staff_member_left (0650) closes the appointment when the member row goes; standing is evaluated per request so access ends at once; SR_STAFF_ALERT {revoked} is pushed to the removed member (staff_feed.rs). Test: sr-server staff_feed_pg_tests.rs._
- [x] **ADM-STF-006** [BE] Staff-guild seed (channels, roles) in bootstrap
- [x] **ADM-STF-007** [BE] System user + bot-style card poster for staff channels (components with action handlers) _sr-server staff_feed.rs: StaffVoice posts cards as the system user, POST /admin/card-actions runs sr:ack:, sr:clear-suspicion: and sr:retry-job: with the presser own permissions. Review-queue Approve/Reject buttons belong to the REV items._
- [x] **ADM-STF-008** [BE] Hash-chained access log writer + nightly verifier job + alert
- [x] **ADM-STF-009** [BE] Re-auth (2FA within 15 min) middleware for sensitive admin routes _AdminDeps::with_reauth + StaffCtx::require_reauth; GET/POST /admin/reauth; applied to private reads, email change, forced password reset, reset-2fa, staff appoint/offer/roles/revoke/delegate, announcements. SR_ADMIN_REQUIRE_REAUTH=0 turns it off for dev. Test: admin/tests.rs sensitive_routes_wait_for_a_fresh_second_factor._
- [x] **ADM-STF-010** [FE] Staff Server rail frame, "SIDE ROOM Staff" profile badge *(rail frame and STAFF badge in `lounge/GuildRail.tsx`, driven by `is_staff_guild` on the guild payload (sent since the p10 integration: guild object, GUILD_CREATE, `GET /users/@me/guilds`); there is no profile badge: the user/profile objects carry no staff flag (accounts.flags bits 0/1 are system and app developer, 8 to 11 are staff-granted badges), so p11 left it; needs a BE flag first, see handoff feat-p11-p10-client)*
- [x] **ADM-STF-011** [FE] "Platform Admin" entry in Staff Server dropdown and user menu *(`lounge/shell/StatusMenu.tsx`, the server menus and `platformStaff.ts`; test in `lounge/__tests__/homeFlows.test.tsx`)*

## ADM-UI — Admin pages shell
- [x] **ADM-UI-001** [FE] `/admin` full-screen layout with grouped nav filtered by platform permissions
- [x] **ADM-UI-002** [FE] Shared admin table component (server-side pagination, filters, column chooser, CSV export) *(`admin/DataTable.tsx`: cursor paging, filter slot, column chooser, CSV export; used by Accounts, Library catalog and Platform audit log)*
- [x] **ADM-UI-003** [FE] Reason-required action modal pattern + audit toast

## ADM-DASH — Overview
- [ ] **ADM-DASH-001** [FE] Dashboard tiles: accounts vs cap gauge, online now, voice rooms + egress, storage used, job backlog, review queue counts, donations vs hosting cost, recent alerts *(partly built: review queue sizes, donations against hosting cost, alerts derived from probes, dead jobs, invoices and the chain check; voice rooms, egress and storage used need a stats route)*
- [x] **ADM-DASH-002** [BE] Stats endpoint (cached 30s) _GET /admin/stats cached 30 s in AdminService (cached_counts); adds accounts_cap, review_pending, invites_open, alerts_unacked, storage_bytes; GET /admin/alerts for recent alerts._
- [ ] **ADM-DASH-003** [FE] Setup Wizard checklist (SMTP test, Discord OAuth, wallet, storage test, voice/TURN test, default limits) *(partly built: checklist for storage, wallet, limits and staff; SMTP, Discord OAuth and voice/TURN have no status or test routes)*

## ADM-PPL — People
- [ ] **ADM-PPL-001** [FE] Accounts search (username, email, id, Discord id, IP), state filter *(partly built: username, email, id and state; Discord id and IP search need server support)*
- [ ] **ADM-PPL-002** [FE] Account drawer: profile, state, Person link, sessions, IP history, invited by, invites issued, servers, entitlements, flags, audit trail *(partly built: profile, state, Person link, recent sign-ins, invited by, invites, credits, entitlements, flags, audit trail; sessions list and servers need routes)*
- [x] **ADM-PPL-003** [BE] Account actions: deactivate, reactivate (cap check), force password reset, reset 2FA, rename, change email, badges, force logout _New: rename, change-email, badges, force-password-reset (POST /admin/accounts/{id}/...). Suspend/unsuspend (cap check), reset-2fa and force-logout already existed._
- [ ] **ADM-PPL-004** [FE] Persons view: account chain timeline, link/unlink accounts, notes, suspicion level with history *(partly built: `PersonDrawer.tsx` shows the account chain, ban, notes, suspicion and its log history; link/unlink, note editing and suspicion history need routes)*
- [x] **ADM-PPL-005** [BE] Platform bans (reason category serious/standard, note) applied to Person, unban
- [x] **ADM-PPL-006** [BE] Inviter suspicion propagation on serious bans + thresholds (freeze, escalate) + clear
- [x] **ADM-PPL-007** [FE] Platform Bans page
- [ ] **ADM-PPL-008** [FE] Staff page (list, roles, appointed by/at, appoint modal, revoke, delegations (Owner)) *(partly built: list, roles, appointed by/at, appoint, revoke; delegations (Owner) need routes)*

## ADM-GRAPH — Invite graph
- [x] **ADM-GRAPH-001** [BE] Graph API (subtree, ancestors path, stats per node, search)
- [x] **ADM-GRAPH-002** [WASM] Force-directed layout for large graphs (Rust, Barnes–Hut) → positions _crates/sr-graph-wasm (layout.rs, Barnes-Hut quadtree), loadGraph() in @sideroom/wasm, packages/wasm/test/graph.test.ts._
- [ ] **ADM-GRAPH-003** [FE] Canvas 2D graph renderer over `sr-graph-wasm`, used when the graph exceeds about 500 nodes (pan/zoom, collapse subtrees, node colors by state, suspicion halo, banned marker); no WebGL *(re-scoped 2026-10-15, P10-ADM-8; below 500 nodes the tree and list views of ADM-GRAPH-005 are used)*
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
- [x] **ADM-COM-003** [FE] Private Content Viewer (channel/DM picker, reason + 2FA gate, read-only message list incl. deleted ≤ 30d and edit history, banner "Access is logged") *(p11: `admin/sections/PrivateContent.tsx` (section `private-content`, needs READ_PRIVATE_CONTENT): look up by channel id, by account (its DMs) or by server (its channels), reason required, paging, deleted/edited badges, edit history; the 2FA gate is the re-auth modal (`admin/ReauthHost.tsx`, `withReauth` in `admin/reauth.ts`); tests `admin/__tests__/p11client.test.tsx`)*
- [x] **ADM-COM-004** [BE] Private content read API (deleted + edits), logging every request _GET /admin/private/channels/{id}/messages (deleted within retention.deleted_message_days plus edit history), /admin/private/accounts/{id}/dms, /admin/private/guilds/{id}/channels. Reason required, fresh 2FA, hash-chained private_read row written before content returns._

## ADM-PLT — Platform settings
- [x] **ADM-PLT-001** [FE] Features page (all modules, platform settings via SchemaForm, kill switches) *(`admin/sections/Features.tsx` kill switches plus `admin/SchemaForm.tsx` platform settings from the feature schema)*
- [ ] **ADM-PLT-002** [FE] Limits page (cap, initial credits, account-age for inviting, credit price, upload limits, message length, rate limits, retention windows, submission rate limits) *(partly built: every setting the server registers is editable; rate-limit and submission-rate-limit settings are not in the registry yet)*
- [ ] **ADM-PLT-003** [FE] Entrance page editor (copy blocks, hero image, house notes markdown, no-invitation text)
- [ ] **ADM-PLT-004** [FE] Themes & Defaults (default theme per section)
- [ ] **ADM-PLT-005** [FE] Email & Integrations (SMTP + test send, Discord OAuth app id/secret, GIF provider key, metadata provider keys, unfurl allow/deny lists)
- [ ] **ADM-PLT-006** [FE] System page (health, versions, backups last run/verify, disk, Garage status, coturn status)
- [x] **ADM-PLT-007** [FE] Platform Audit Log viewer *(`admin/sections/AuditLog.tsx`: the `admin_action` rows of the staff access log)*
- [x] **ADM-PLT-008** [FE] Access Log viewer + chain verification status
- [x] **ADM-PLT-009** [BE] Platform announcements (banner to all users, optional DM from system user) _POST/GET/DELETE /admin/announcements, GET /announcements/active for every signed-in user, optional system DMs to every active account._

## ADM-ALR — Alerts
- [x] **ADM-ALR-001** [BE] Alert bus → `#alerts` channel cards (suspicion, payments anomalies, transcode failures, disk > 80%, backup failure, access-log chain break, banned-Person registration attempt) _staff_feed.rs consumes Topic::Platform StaffAlert into #alerts cards (stored in staff_alerts); admin_jobs.rs health job adds disk, backup and transcode alerts. Suspicion, payment, chain-break and banned-Person alerts already published events._
- [x] **ADM-ALR-002** [BE] `#new-members` and `#donations` feeds _StaffFeed events: registration posts to #new-members (usernames only), a credited invoice posts to #donations (no donor name)._
- [x] **ADM-ALR-003** [BE] Optional email alerts to Owner _alerts.email_to_owner setting; StaffFeed::mail_owner, throttled to one mail per kind per 30 minutes._
