# Handoff: feat/p4-admin-ui (Admin / Staff Server web section)

Worktree `SIDEROOM_wt/p4-admin-ui`, branch `feat/p4-admin-ui` (base `integration/phase3`). Local commits only; nothing pushed. No backend, migration or OpenAPI changes (the 37 existing admin operations were enough).

## What exists
`apps/web/src/admin/` (route `/admin` and `/admin/:section`, replacing the placeholder page):

| Area | Files | Notes |
|---|---|---|
| Shell | `AdminApp.tsx`, `routes.tsx`, `permissions.ts` | Full-screen `SettingsLayout` in the `staff` theme. Reads `GET /admin/me`, parses the platform-permission bitfield (BigInt) and lists only sections the role can use. 401 shows "Sign in", 403 shows "Staff only". An unknown or forbidden section redirects to the first allowed one. Hiding is a courtesy; the API stays the boundary. |
| Shared | `ui.tsx`, `api.ts`, `admin.css` | `ReasonDialog` (reason required unless the API treats it as optional, errors shown in the dialog), cursor `usePaged`, `StateBadge`, viewer context. Reuses the server-settings helpers (`useResource`, `Async`, `useAction`, `ss-*` classes). `api.ts` is a typed wrapper over `apiFetch` (the web app does not consume `@sideroom/api-client` yet). |
| Dashboard | `sections/Dashboard.tsx` | Counts from `/admin/stats`; online-now from `/admin/system` (OPERATOR); accounts-vs-cap gauge from the `accounts.cap` setting (MANAGE_FEATURES). Tiles whose data does not exist yet are named in a notice, not faked. |
| Accounts | `sections/Accounts.tsx` | Search (q, state), paged table, drawer with profile, sessions, last sign-ins, invited-by, invite lineage, credits ledger. Actions with mandatory reasons: suspend, reactivate (409 on the cap is shown), revoke sessions, reset 2FA, platform-ban (standard/serious), grant/revoke credits. Buttons follow MANAGE_ACCOUNTS, PLATFORM_BAN, MANAGE_INVITES. |
| Platform bans | `sections/Bans.tsx` | List (active only toggle) and lift with reason; outcome counts reported. |
| Invites | `sections/Invites.tsx` | List with state/issuer filters, revoke with confirm, emergency tag, suspicion list with clear-with-reason. |
| Staff & roles | `sections/Staff.tsx` | List, appoint (account id + roles + reason), change roles, remove with confirm. Owner row has no actions. Role checkboxes are disabled for roles the viewer may not assign (Owner role never; READ_PRIVATE_CONTENT, APPOINT_STAFF, VIEW_ACCESS_LOG, OPERATOR only for the Owner; a delegate only roles whose bits it holds), mirroring `canAssignRole`. |
| Access log | `sections/AccessLog.tsx` | Newest-first table (kind filter, paged), "Verify chain" calling `/admin/access-log/verify`; a break is an alert naming the first bad row. |
| Limits & settings | `sections/Limits.tsx` | Renders whatever `/admin/settings` returns, grouped (so `moderation.audit_retention_days` appears under Retention). Client-side range checks, one save with a reason, only changed keys sent. |
| Features | `sections/Features.tsx` | Kill-switch toggles and reset-to-default, optional reason. |
| System / Jobs | `sections/System.tsx` | Probes and version (OPERATOR); jobs list with retry and delete of dead jobs (MANAGE_TRANSCODE). Review queues is an honest "not built yet" stub (no submissions endpoints exist). |
| Command line | `sections/CliParity.tsx` | Read-only `sr-cli` parity: emergency invite, staff list/appoint/revoke, transfer-owner, verify-access-log. |

Tests: `src/admin/__tests__/admin.test.tsx` (25 tests, MSW + Testing Library, axe on dashboard, account drawer and access log) and `e2e/tests/admin-accounts.spec.ts` (suspend/reactivate and access-log verify against a real server).

Inventory ticked: ADM-UI-001, ADM-UI-003, ADM-PPL-007, ADM-PLT-008.

## Not done / gaps
- **Invite create** in the web UI: there is no admin or member endpoint for it (CLI only), so the Invites page lists, revokes and explains the CLI. No credits "grant" outside the account drawer.
- Not built because no backend exists: account rename/change email/badges/password reset, Persons view and link/unlink, invite graph (WebGL), platform audit log viewer, private content viewer, servers/directory, donations, entrance/themes/email editors, setup wizard, alerts, delegations, appointment accept/decline.
- ADM-UI-002 (generic table with column chooser and CSV export), ADM-DASH-001 (voice, egress, storage, donations, review counts), ADM-PPL-001 (Discord id and IP search), ADM-PPL-002 (IP history, entitlements, audit trail in the drawer), ADM-PLT-001 (SchemaForm for per-feature settings) and ADM-PLT-002 (rate limits, upload tiers) are only partly covered and left unticked.
- `library.ingest.rar_enabled` is not in the admin settings catalog (`sr_admin::settings::CATALOG`) and `sr-library` does not read it from `platform_settings` yet, so it cannot appear on Limits. Adding it needs a catalog entry plus the ingest job reading the setting. `moderation.audit_retention_days` is in the catalog and shows.
- The Staff appoint form takes a raw account id (find it in Accounts); an account picker needs a lookup endpoint that APPOINT_STAFF alone can call.
- No Persons or audit trail means "account audit trail" is only reachable through the Access log.
- Firefox does not launch locally; the Playwright spec was run on chromium only (2 passed against a real sr-server on a throwaway database, since dropped). Locally /readyz fails without S3, so poll /healthz instead.

## Decisions
- The page uses the `staff` theme section and the Server Settings layout (spec section 1), and does not mount the gateway connection; admin screens only use REST.
- Reason fields: required for suspend, reactivate, revoke sessions, reset 2FA, ban, unban, credits, staff appoint/roles and settings saves (the service refuses an empty reason); optional for feature toggles and job retry (the service uses `optional_reason`).
- Removed the `/admin` row from `router.test.tsx` (the placeholder heading it asserted is gone); the new tests cover the route.
