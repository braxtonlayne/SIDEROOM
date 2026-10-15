# Handoff: feat/p12-admin-ui (phase 12, admin web UI)

Worktree p12-admin-ui, branch feat/p12-admin-ui. Web only (`apps/web/src/admin`), no backend or API change. New strings: `apps/web/src/i18n/messages/en/p12-adminui-1.ts`.

## Done

| Item                                             | What                                                                                                                                                                                                                                                                                                                                                                                              |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Appointment offers (ADM-STF-004 UI, ADM-PPL-008) | Staff page: "Send appointment offer" is the primary action (`POST /admin/staff/offers`), a card lists offers with state and Withdraw (`DELETE /admin/staff/offers/{id}`). The direct appoint stays (backend still has it, behind re-auth) but only the Owner sees it, as "Appoint directly". `sections/Staff.tsx`, `sections/StaffOffers.tsx`.                                                    |
| Delegations (Owner)                              | Owner-only card: list, add (staff member + permission ceiling checkboxes, Owner-only permissions are not offered), remove. `PUT/DELETE /admin/staff/{id}/delegation`.                                                                                                                                                                                                                             |
| Announcements (ADM-PLT-009 UI)                   | New section "Announcements" (MANAGE_FEATURES): list with status, composer (title, message, severity, optional end time, system-DM toggle). Turning the DM on shows a warning that it messages every active account and changes the confirm button to "Publish and message everyone". End now with confirmation. The server has no edit route, so "Reuse" prefills the composer; the page says so. |
| Account actions (ADM-PPL-003 UI)                 | Drawer buttons Rename (sends only changed fields), Change email, Badges (checkboxes from `flags` bits 8 to 11; sends grant/revoke diff), Force password reset. All go through `withReauth` (tested with the 900004 retry). Flags row also lists held badges.                                                                                                                                      |
| Dashboard (ADM-DASH-001, partly)                 | Tiles for storage used and open invites (`/admin/stats`), and voice rooms / egress that appear only if the server sends `voice_rooms` / `egress_bytes`. Alerts card now shows the recorded alerts (`/admin/alerts`) with Acknowledge, next to the derived ones.                                                                                                                                   |

Inventory: ADM-PPL-008 ticked; notes added to ADM-STF-004, ADM-PPL-003, ADM-PLT-009; ADM-DASH-001 note updated and left open.

## Still open

- Voice rooms and egress dashboard tiles: `/admin/stats` has no such fields (no voice stats route). The UI is ready and shows them as soon as the fields exist.
- Review queue Approve/Reject on `#review-queue` cards (item 5): the backend card-action registry only knows `sr:ack`, `sr:clear-suspicion` and `sr:retry-job` (staff_feed.rs), so there is nothing to call. The Library tab in Review already approves/rejects through `/admin/review/...`. Needs a backend card action first.
- Announcement edit: no PUT route; see above.

## Decisions / owner questions

- Direct appoint is shown to the Owner only (conservative reading of doc 07: the appointee accepts). Remove it entirely or keep it? (Same question as in the admin-be handoff.)
- Delegation ceiling excludes Owner and the Owner-only permissions (READ_PRIVATE_CONTENT, APPOINT_STAFF, VIEW_ACCESS_LOG, OPERATOR) in the UI; the server also refuses them.
- The announcement composer sends `reason` only when typed (optional), matching the server.

## Tests run (apps/web, one process at a time)

- `tsc --noEmit`, `eslint src/admin src/i18n`, `prettier --check`: clean.
- vitest `--maxWorkers=2` on `src/i18n src/admin src/lounge/announcements src/lounge/shell`: 16 files, 181 tests passed, incl. 19 new in `admin/__tests__/p12adminui.test.tsx` (offers, withdraw, direct-appoint visibility, delegation, announcements incl. DM warning and end/reuse, rename/email/badges/force reset with re-auth, dashboard tiles, alert acknowledge) with axe checks on staff, announcements, badges dialog, DM warning and alerts.
- `admin.test.tsx` updated: nav list now has Announcements; the direct-appoint test clicks "Appoint directly".
- `vite build` + `scripts/check-bundle-budget.mjs`: all ok (admin 285.8 KB gzip of 600). `node scripts/i18n-verify-diff.mjs --cached`: MISSING 0.
- Playwright not run.
