# Handoff: feat/p18-mod-dsr (M-25 step c, MOD-DSR)

Base: integration/phase18 (= main). Migration used: `1100_data_requests.sql` (range 1100-1109).

## Done (MOD-DSR items)
- **MOD-DSR data_requests table**: kind (`export` | `erasure`), subject Person (`person_id`) and the account, `received_at`, `due_at` computed at creation from `compliance.data_request_deadline_days` (read from `platform.compliance`, default 30; never recomputed), `extended_until` + reason + who/when, `verified_by`, `export_id`, `outcome`, `retained` ids, `completed_at`. CHECK constraints keep the extension all-or-nothing and forward-only; one active request per account and kind (unique index); no cascade, no delete path.
- **MOD-DSR lifecycle**: `open` -> `verified` (staff record how identity was checked) -> `running` (export) -> `completed`, or `declined` with a reason. Every change writes the hash-chained staff access log in the same transaction. Denied attempts are logged.
- **MOD-DSR extend**: `POST /admin/data-requests/{id}/extend {days, reason}`: 1 to 60 days (GDPR 12(3)), once only (second call 409), reason mandatory, access-logged with the reason.
- **MOD-DSR queue view**: new Admin page `data-requests` ("Data requests", `MANAGE_TAKEDOWNS`) with due date, "due in N days", "Overdue by N days" badge, the extension and original date, outcome and the named retained records; actions Verify, Run, Extend, Decline, and "Enter a request". It is its own page (no Moderation section existed), its nav group label is the string "Moderation" via `admin.dataRequests.group` so the integrator can move it into the other agent's section by changing the `group` in `ADMIN_SECTIONS` (apps/web/src/admin/AdminApp.tsx).
- **MOD-DSR export run**: reuses the self-service `user.data_export` job (`data_export::queue_in`, shared with the route) and its README (D-124 retention paragraph already in it). Skips the 24-hour cooldown, still one open build per account. Completion/failure of the job moves the request to `completed` (outcome points the member at Settings) or back to `verified` (so staff can rerun). The member gets a system DM (best effort). The package is downloaded by the member from Settings (their session), it is not handed to staff.
- **MOD-DSR erasure run**: calls `account_deletion::delete_account` (same media clean-up and live notices as the self-service route), completes the request with an outcome naming the retained records (`sr_admin::RETAINED`: Person record, invite graph, invoices, staff-side moderation history, content under a hold, linked Discord identity, the request log itself) and counts held rows (messages/attachments with `hold_id`) for the case decision. Blocked accounts (owns servers, staff, Owner) give 409 with the reason and the request stays `verified`.
- Routes (all `MANAGE_TAKEDOWNS`, session only, re-auth required for the two runs when the platform asks for it): `GET/POST /admin/data-requests`, `GET /admin/data-requests/{id}`, `POST .../verify`, `.../decline`, `.../extend`, `.../run`. Route matrix rows added (authz `staff`, no `?`), OpenAPI, api-client types and MSW mocks regenerated; `KNOWN_CLASHES` still empty (`AdminDataRequestList`, `AdminCreateDataRequestBody`, `AdminExtendDataRequestBody` are explicitly named).
- Docs: spec `moderation-compliance.md` section 6 table and section 14 item 6 (queue built), doc 09 section 4 paragraph (extension, overdue, rows kept).

## Follow-ups N-01 to N-03 (D-316 to D-318)
- **N-01** cap stated in days everywhere (code comments, 400 message, dialog hint, doc 09, spec).
- **N-02** extension DM with the staff reason and new date; `data_requests.member_notified_at` (migration 1101); `extended_late`; queue shows "Member told by DM on ..." or "Not told in-app: reply by email" and an "Extended late" badge. The access log gets the usual `data_request.extend` row and a second `data_request.extend_notice` row carrying `member_notified` true/false (the DM is sent after the extension commits, so the outcome cannot be in the first row).
- **N-03** eight retained records with the D-318 wording; the export README is generated from `RETAINED`; the deletion-screen string lists the same eight labels in order (Rust source test compares both); "Messages you leave in place stay..." line added.
- **Finding, `delete_account` and the account row:** it already scrubs username, display name, email (archived for staff in `deleted_accounts`), password hash, avatar, banner, bio, pronouns, colours and profile document, and deletes credentials, devices, settings and relations. It left: (a) `sessions.ip`, `user_agent`, `device_label` on the revoked rows and (b) `login_events.ip`: both are now set to NULL in the same transaction. Still on the row, not identifying on their own and kept as part of the Person record/accounting trail: `locale`, `timezone`, `flags`, `age_declared`/`age_confirmed*`, `created_at`, `invited_by_invite_id`. The staff-only `deleted_accounts` archive (original username, email, display name) is not named in the retained list (owner question 3).
- **No member bulk self-delete exists**, so the deletion screen has the sentence but no link; open item **N-04** added to answers-followups-p16.md.

## Follow-ups N-05 to N-09 and N-13 (D-324 to D-327)
- **N-05** member bulk self-delete: migration 1102, `sr_db::self_delete`, job `user.messages_self_delete` (sr-server `userset_jobs.rs`), `GET`/`POST /sr/users/@me/messages/delete-all`, Settings, Privacy card, link from the deletion screen. Limits are tested (one run at a time, once per 24 h for members, held rows keep `purge_after` null). Not done: the case note on an open moderation case (the cases tables are on another branch; the `messages.self_delete` access-log row is written) and the sr-chat repo upper bound (done in `sr_db::self_delete::hide` instead).
- **N-06** erasure run option `delete_messages`; queued after the account is erased (a deviation from "before": a blocked erasure then leaves nothing half done).
- **N-07** `compliance.invoice_retention_years` (default 10), `retained(years)`, README and deletion screen carry the number.
- **N-08** migration 1103 and monthly job `payments.detach_expired_invoices` (job id 1105); a detached invoice reads as user 0 in Rust and "Detached" in Admin. A late payment to a detached invoice's address would try to credit user 0: detach only happens for final states, so the watcher no longer polls them.
- **N-09** nine retained records; `GET /admin/accounts/{id}/deleted-archive` with a logged read; account drawer block.
- **N-13** two-row extension log documented and tested (D-327).
- Migrations used: 1100, 1101, 1102, 1103. New job ids: 1105. New routes (matrix rows added): the two `/sr/users/@me/messages/delete-all` and the deleted-archive read.

## Follow-ups N-14 and N-15 (D-332)
- **N-14** the member self-delete (and the staff erasure option) also removes reactions and poll votes, same scope and cut-off, same run. Closed-poll votes become `anonymous_counts` on the poll (added into `poll_tallies`, shown inside the poll object, no migration); held messages keep reactions and votes. Live `MESSAGE_REACTION_REMOVE` per reaction. Counts in the `GET` response (`reactions`, `poll_votes`), card, access-log row, DM and the erasure outcome ("N messages, M reactions, K poll votes"). Retained label is now "Messages, uploads, reactions and poll votes you did not delete". `Poll` gained `anonymous_counts` (OpenAPI regenerated).
- **N-15** export: `reactions.json` and `poll_votes.json` (own rows, ids only, no message text) plus README lines.
- The integrator note about the moderation-case note is untouched.

## Still open / not done
- Doc 13 Phase 7 "in progress" line and `docs/inventory/19-moderation.md` ticks: left to the integrator (the other agent owns the inventory file, and doc 13 is touched by both). The MOD-DSR ids to tick are the six bullets above.
- Placing the page under Admin > Moderation (see above).
- Restriction/objection staff tool (spec section 6 row 4) and the rectification staff correction are not part of the queue (not requested in step c).
- Requests are entered by staff; there is no member-facing "ask staff" form (the self-service export and deletion remain). Entering requests via email/form is the documented intake.
- Playwright was not run (forbidden this wave).

## Tests run (N-05 to N-13)
- sr-admin 60, sr-features 109, Postgres `self_delete` 3, `invoice_retention` 1, `data_requests` 9, `data_export` 3, sr-api `routes::admin` 27 plus the self-delete route test, route matrix and openapi_names; web p18 tests. Full gates in the final report.

## Tests run
- `cargo test -p sr-admin` (6 new `tests_dsr`: due date from deadline, permission and denial log, extend once and bounds and log reason, overdue follows the effective date, run needs verification and one active per kind, erasure names retained records and a blocked account stays verified).
- Postgres (scratch DB, `--include-ignored --test-threads=1`): `cargo test -p sr-db --test data_requests` 6 passed: lifecycle with dates, one extension, DB CHECKs and access-log chain; export run without cooldown and job queued, completion and failure hooks; erasure retention guard (Person, invite graph both sides, invoice, ban, audit row, held message, staff archive, Discord link, request row all survive; account row cannot be deleted); owner-of-server block; source guard that nothing in the workspace deletes/truncates `data_requests` and the migration has no `ON DELETE`.
- `cargo test -p sr-api --lib routes::admin` 26 passed (new flow test + the auth matrix with the 7 new endpoints), `--test route_matrix`, `--test openapi_names`.
- clippy `-D warnings` and fmt on sr-admin, sr-db, sr-api, sr-server (see the final report for the result of the last runs).
- Web: `p18dsr.test.tsx` 5 passed; prettier, eslint, tsc clean; full vitest, build and bundle budget results in the final report.

## Decisions
- Permission is `MANAGE_TAKEDOWNS` (D-210, spec section 5 item 7), not `MANAGE_ACCOUNTS`; Moderator cannot see the queue, Senior Staff can.
- Extension cap 60 days: GDPR 12(3) allows two further months; the spec leaves the cap open, this is the conservative reading. A request that is closed cannot be extended.
- Export on a person's behalf is queued as the person's own package; staff never download it (privacy), the member does, from Settings.
- `received_at` may be backdated up to a year (requests that arrived before staff entered them count from arrival).
- The erasure of an account that was already deleted by its owner completes the request with a note instead of failing.
- The i18n group string "Moderation" is repeated from a key of mine so the page lands in the same nav group as the other agent's pages when they use the same label.

- Extension log: two rows (`data_request.extend`, then `data_request.extend_notice`), accepted by D-327; the notice row is written also when the DM fails.
- Self-delete runs in sr-server over Postgres (not through the chat service), so it does not touch the sr-chat repo trait.

## Owner questions (updated)
- Answered by D-324 to D-327 (member self-delete, invoice years, archive, two-row log). Answered 2026-10-20 by D-332: (1) the self-delete and the erasure run with `delete_messages` also remove reactions and poll votes (closed polls keep an anonymous tally; held content stays), open item N-14; (2) yes, the case note on an open moderation case is the integrator's once the cases branch merges (noted under N-14 in answers-followups-p16.md).
- Answered 2026-10-20 by D-324 (member bulk self-delete, DMs included, N-05 and N-06), D-325 (invoice retention in years, `compliance.invoice_retention_years`, N-07 and N-08), D-326 (`deleted_accounts` archive named, N-09) and D-327 (the two-row extension log is accepted, N-13). Earlier: - Answered by D-316 (60 days, counted in days), D-317 (DM on extension), D-318 (retained list). New: (1) should the member bulk self-delete (N-04) be built, and across DMs too? (2) should the invoice retention period be a stated number of years in the README? (3) should the staff-only `deleted_accounts` archive (original username, email, display name) be named in the retained list? Still: is 60 days the right ceiling for the single extension, and should the extension be disclosed to the member by system DM automatically? (Not done: no member notice on extend.)
- The retained-record reasons in `RETAINED` restate D-124/D-295 defaults; D-295's lawyer review still applies.
