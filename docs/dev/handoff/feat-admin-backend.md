# Handoff: feat/admin-backend (ADM-* Phase 3 backend)

Worktree: C:/Users/braxt/Desktop/SIDEROOM_wt/admin, branch feat/admin-backend, base integration/phase1.
CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/admin-target. Migrations owned: 0130-0139 (0130, 0131 used).
Local only: nothing pushed or merged. docs/open-questions.md untouched.

## DONE and verified
- crates/sr-admin (new): model, error, chain (sha256 hash chain + verifier), settings catalog, rules (Owner-only bits, assignability, default staff roles), AdminRepository trait, AdminService (permission checks, reasons, protections, audit, alerts via bus), MemoryAdminRepo. `cargo test -p sr-admin`: 25 pass.
- sr-events: StaffAlertKind gained InviterSuspicion, InviterEscalated, StaffRevoked, AccessLogChainBroken.
- sr-db: migrations 0130 (staff_appointments, staff_role_templates seeded, staff_access_log + append-only/no-truncate triggers) and 0131 (person_bans, suspicion_events, account_delete_requests); admin_repo.rs (PgAdminRepo, append_in, verify_chain); accounts::reset_2fa_in; registration also refuses a Person with an open person_bans row.
- Postgres tests `cargo test -p sr-db --test admin_repo -- --ignored --test-threads=1` against sr_test: 8 pass (incl. tamper test in rolled-back tx, concurrent appends, trigger blocks, registration blocking).
- Last commit state: tree clean, `cargo check -p sr-admin -p sr-db` ok.

## REMAINS (checklist)
- [ ] bootstrap: instantiate staff_role_templates as Staff Server roles (crates/sr-cli/src/guilds.rs create_staff_guild), give the Owner the Owner role + an appointment row; update the println there.
- [ ] sr-cli: `staff transfer-owner|appoint|revoke|list`, `verify-access-log` (use AdminService with Caller::operator(1)).
- [ ] sr-api routes/admin/ (utoipa, staff session only via Principal; Caller from AdminService::resolve; NotStaff -> SrStaffOnly 403, MissingPermission -> MissingPermissions 403, ReasonRequired -> SrReasonRequired 400, NotFound -> 404, Invalid/Conflict -> 400/409-ish). Endpoints: staff, accounts (search/view/suspend/unsuspend/force-logout/reset-2fa/delete-request/ban), persons, bans, invite tree/descendants/ledger/adjust/invites/suspects, settings GET/PATCH, features list/toggle (FeatureService via Ext; add sr-features dep; register it as extension in sr-server), stats, system (needs probe for gateway sessions and readiness), jobs, access-log + verify. Reason may also come from X-Audit-Log-Reason header. Client IP for audit.
- [ ] sr-server app.rs: build PgAdminRepo/AdminService (with bus), register extensions, nightly verify job (optional).
- [ ] oneshot tests with 401/403 matrices (non-staff, staff lacking perm, Owner) for every endpoint; routes::all/openapi merge.
- [ ] docs/07 section 4: add default role bits table marked "proposed defaults - owner to confirm".
- [ ] Live run (database sr_admin, ports 8105/9105) with a node script; regenerate OpenAPI client (bash: cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json, then npm_config_manage_package_manager_versions=false npx pnpm@10.12.1 --filter @sideroom/api-client generate); cargo fmt/clippy -D warnings/test --workspace; cargo deny; tick ADM-* inventory items truly done (docs/inventory/10-admin.md).
- Not wired / flag in report: platform_settings values (accounts.cap etc.) are stored but registration still uses AuthConfig; invite-creation freeze enforcement not wired; private-content read API (ADM-COM-004) only has log_private_read; appointment accept/decline DM flow skipped (direct appoint); delegations table skipped.

## OWNER decisions needed
Default staff roles (proposed, owner to confirm; seeded in staff_role_templates, mirrored in sr_admin::rules::default_staff_roles, asserted equal by a PG test). Bits: STAFF_BASIC=1 ... see doc 07.
- Support (pos 1) = 10241: STAFF_BASIC, MANAGE_ACCOUNTS, MANAGE_INVITES
- App Reviewer (2) = 769: STAFF_BASIC, REVIEW_APPS, REVIEW_PACKAGES
- Library Curator (3) = 85: STAFF_BASIC, REVIEW_READING, PUBLISH_READING, MANAGE_COLLECTIONS
- Media Curator (4) = 203: STAFF_BASIC, REVIEW_MEDIA, PUBLISH_MEDIA, MANAGE_COLLECTIONS, MANAGE_TV
- Moderator (5) = 7169: STAFF_BASIC, MANAGE_DIRECTORY, MANAGE_ACCOUNTS, PLATFORM_BAN
- Senior Staff (6) = 1818623: everything except OWNER, MANAGE_DONATIONS, VIEW_ACCESS_LOG, OPERATOR (includes APPOINT_STAFF, READ_PRIVATE_CONTENT)
- Owner (7) = bit 63
Also: Owner-only grants are READ_PRIVATE_CONTENT, APPOINT_STAFF, VIEW_ACCESS_LOG, OPERATOR. Staff accounts can only be suspended/banned by the Owner. Access log records mutations, account/person/invite-graph reads, staff denials.

## Resume
Env (no secrets): SR_TEST_DATABASE_URL=postgres://postgres:<pw from %TEMP%\sr_pg_pw.txt>@localhost:5432/sr_test. Live DB name planned: sr_admin (not yet created). Last commands: cargo test -p sr-db --test admin_repo -- --ignored --test-threads=1.
