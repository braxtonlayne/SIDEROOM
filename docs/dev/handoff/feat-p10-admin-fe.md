# Handoff: feat/p10-admin-fe (ADM-\* front end, phase 10)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p10-admin-fe, branch feat/p10-admin-fe (base integration/phase10). Local commits only, web only (`apps/web/src/admin`, a little of `apps/web/src/lounge`). No crate was touched.

## Done (ticked in docs/inventory/10-admin.md)

- ADM-UI-002 `admin/DataTable.tsx`: cursor "Load more" paging, a filter slot, a column chooser that remembers its choice per table, CSV export of the loaded visible columns (formula text is defused with a leading quote). Used by Accounts, Library catalog and the Platform audit log.
- ADM-GRAPH-005 `sections/InviteGraph.tsx` (new section "Invite graph"): collapsible tree (`role=tree`), flat list, path-to-root breadcrumb, re-root, hop depth.
- ADM-CNT-002 already built as CIN-ADM-001 (ticked).
- ADM-CNT-006 the transcoding screens were written against an assumed contract. `cinema/transcodeApi.ts` now maps the real `/admin/transcode/*` and `/admin/cinema/assets/{id}` routes (array bodies, `job_id`, `PUT drain`, `before` cursor, `POST retranscode {scope:"asset"}`), shows queue counts and a priority column. Tests in `cinema.test.tsx` were moved to the real wire shapes.
- ADM-PLT-001 Features page now has a Settings drawer rendered from the feature schema (`admin/SchemaForm.tsx`), saved as platform overrides (only values that differ from the schema default are sent). Kill switches already existed.
- ADM-PLT-007 "Platform audit log" (`sections/AuditLog.tsx`): the `admin_action` rows of the staff access log, filter by action/staff/target, column chooser, CSV.
- ADM-STF-010 / 011 Staff Server rail frame plus a STAFF badge and "(SIDE ROOM Staff)" tooltip (`lounge/GuildRail.tsx`), "Platform Admin" in the user status menu, the rail context menu and the server header menu (`lounge/shell/platformStaff.ts` reads `/admin/me` once per session).

## Partly built (left unticked, reason in the inventory line)

- ADM-DASH-001 tiles added: review queue sizes (Library, Apps, Cinema reports), donations credited this month against hosting cost, alerts derived from failing probes, dead jobs, invoices needing attention and a broken access-log chain. Missing: voice rooms + egress, storage used (no stats route).
- ADM-DASH-003 setup checklist (storage probe, wallet, limits at defaults, second staff member). SMTP test, Discord OAuth and voice/TURN test have no routes and show "Cannot check yet".
- ADM-PPL-001 search covers username, email, id, state. Discord id and IP are not searched by `GET /admin/accounts`.
- ADM-PPL-002 drawer adds entitlements (needs MANAGE_DONATIONS), audit trail (VIEW_ACCESS_LOG), raw flags and an "Open person record" button. No sessions list or servers list route.
- ADM-PPL-004 `sections/PersonDrawer.tsx` (opened from an account and from the Bans page): account chain oldest first, ban, notes, suspicion, clear suspicion, lift ban, access-log history. No link/unlink, note editing or suspicion history route.
- ADM-PPL-008 shows appointed by (account id) and at; no delegations route.
- ADM-GRAPH-004 side panel: summary, open codes, credit ledger, grant/revoke credit, clear suspicion. "Freeze invites" has no route (freeze only happens through suspicion thresholds).
- ADM-CNT-001 "Review queues" is now a tabbed hub: Cinema (open report queue, hide content, dismiss; DELETE_CONTENT), Library (the existing reviewer workspace), Apps (developer applications and version queue with approve/reject/request changes; REVIEW_APPS). Packages and Directory tabs say the routes are not built.
- ADM-CNT-003 `admin/library/` (new "Library catalog" section): series list and form, editions, chapters, publish/unpublish, merge, delete. Page reorder/replace/re-convert are not built (they need the upload pipeline; that is LIB-ADM-001).
- ADM-PLT-002 Limits edits every setting the server registers; rate limits and submission rate limits are not in the registry.

## Not done

- ADM-GRAPH-003 WebGL renderer (and -002 layout): too large for this pass; the tree/list view is the fallback it specifies.
- ADM-CNT-004 collections: the cinema rows/heroes editor exists but has no schedule, and library reading lists featured has no route.
- ADM-CNT-005 Ingest sources, ADM-COM-001..003 (directory approvals, all-guilds search/transfer/delete, private content viewer), ADM-PLT-003 (entrance editor), -004 (default themes), -005 (email and integrations), -006 (backups, disk, Garage, coturn on the System page): no server routes. The System page still shows version, probes, gateway sessions and job counts.

## Backend gaps for admin-be (routes the FE would use)

1. Guild payload (READY / GUILD_CREATE / `GET /users/@me/guilds`) must include `is_staff_guild: true` for the Staff Server, or the rail frame and the server-menu Platform Admin entry never show. A staff profile badge also needs a user-profile field.
2. `GET /admin/accounts`: search by Discord id and IP. `GET /admin/accounts/{id}`: sessions list, guild memberships, flags names. Account actions in ADM-PPL-003.
3. Persons: link/unlink accounts, edit notes, suspicion history; invite freeze/unfreeze per account.
4. Staff delegations (`/admin/staff/delegations`), appointment accept/decline state.
5. Stats: voice rooms + egress, storage used (cached 30s), plus system details: backups last run/verify, disk, Garage, coturn.
6. Guilds admin: search all guilds, metadata, transfer ownership, delete; server directory requests/categories/featured; private content reader (reason + 2FA, deleted and edit history).
7. Ingest sources (watched folders, imports, naming tester); entrance copy, default themes, SMTP/Discord/GIF/metadata keys with test send.
8. Rate-limit and submission-rate-limit keys in the settings registry; library collections schedule.

## Tests run

`cd apps/web`: `npx tsc --noEmit`, `npx eslint .`, `npx prettier --check src`, `npx vitest run --maxWorkers=3` (1402 tests; a handful of Lounge tests time out at 5 s when 8 agents share the machine and pass on re-run alone), `npx vite build` and `node scripts/check-bundle-budget.mjs` (admin 273 KB gzip of 600, lounge 303 of 600). New tests: `admin/__tests__/p10.test.tsx` (CSV, DataTable, account drawer extras, person record, invite graph, review hub, library catalog, dashboard tiles and checklist, feature settings form, audit log) and a describe block in `lounge/__tests__/homeFlows.test.tsx`. No Rust, no route matrix or OpenAPI change.

## Decisions

- Platform audit log = the `admin_action` rows of the one hash-chained access log (no second table), so chain verification stays on the Access log page.
- CSV export covers the rows loaded so far (server paging is cursor based), not the whole result.
- Alerts on the dashboard are derived from existing routes until the alert bus (ADM-ALR) exists.
- New i18n fragments `apps/web/src/i18n/messages/en/p10-admin-fe-1..10.ts`. `admin.dashboard.notTracked*` keys are now unused (replaced by `admin.dashboardExtras.*`); fragment owners can drop them.

## Owner questions

- Should the Staff Server profile badge be a user-profile badge (needs a badges column and API) or only the rail frame?
- Is "Merge series" meant for PUBLISH_READING staff only? The API also requires DELETE_CONTENT, so the dialog shows the server error otherwise.
