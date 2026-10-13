# Handoff: feat/admin-backend (ADM-* Phase 3 backend) - COMPLETE (pending owner review)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/admin, branch feat/admin-backend (base integration/phase1). Local commits only.
CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/admin-target. Migrations used: 0130, 0131.
Live database left in place: sr_admin (ports 8105/9105; env in C:/Users/braxt/Desktop/SIDEROOM_wt/admin-run/env.sh, contains a password, not committed).

## Done
sr-admin crate (rules, service, chain, memory repo), Postgres repo + migrations, /api/v1/admin/* (37 operations) with 401/403 matrix tests, registration reads platform_settings accounts.cap and refuses invites of frozen issuers, sr-cli staff list/appoint/revoke/transfer-owner + verify-access-log, bootstrap creates the default roles, sr-server wiring + nightly chain-verify job (id 130), OpenAPI client regenerated, docs/07 role table, ADM ticks (STF-002, STF-006, STF-008, PPL-005, PPL-006, GRAPH-001).
Live check: `node e2e/admin-smoke.mjs <sr-cli path> <owner password>` against sr-server on 8105: 50 checks passed.

## Not done / unverified
Appointment accept/decline DM, delegations table, private-content read API (only AdminService::log_private_read), user-facing invite creation gate (no such endpoint exists; freeze enforced at redemption), account rename/change email/badges/password reset, stats caching, platform announcements, directory approvals, block guild invites to the Staff Server, closing admin WS on revoke. Storage probe in /admin/system reports the missing local S3 (expected locally).

## OWNER decision: proposed default staff-role bits (docs/07 section 4)
Support 10241, App Reviewer 769, Library Curator 85, Media Curator 203, Moderator 7169, Senior Staff 1818623 (all but OWNER, MANAGE_DONATIONS, VIEW_ACCESS_LOG, OPERATOR), Owner bit 63. Owner-only grants: READ_PRIVATE_CONTENT, APPOINT_STAFF, VIEW_ACCESS_LOG, OPERATOR.
