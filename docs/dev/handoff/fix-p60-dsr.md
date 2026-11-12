# Wave 60 dsr handoff (SEC-AUD-116..122, D-616..D-618)

Branch `fix/p60-dsr`. All seven items done. No UI, no migration, no route change.

## Done
- 116 (D-616): `run_erasure` is one transaction: lock the request `FOR UPDATE`, require `verified`, `delete_account_in`, `self_delete::request_in`, `flag_held_holds_in`, completion UPDATE, `audit_in`, commit. `delete_account` / `self_delete::request` keep their signatures as wrappers.
- 117 (D-617): deletion removes cinema_progress, reading_progress, reading_bookmarks, scratch_notes, personal loot_tables, saved/private/friends cinema_lists, claims on invite cards not yet started. Report gains `personal_rows_deleted`. Module header and `docs/09` (account row, invite-card row) updated.
- 118: ready exports get `expires_at = now()`, pending/building become `failed` ("account deleted"). Report gains `exports_expired`.
- 119: `data_export::finish` and `fail` are one transaction each.
- 120: `patron_transitions` reads and upserts in one transaction, returns after commit.
- 121 (D-618): `publish_now` claims Draft/ChangesRequested -> Approved (`approved` event, `direct: true`) before sync/move/publish; reuses `publish_approved`; `Approved` is accepted as a retry by the submitter. `machine.rs`: `PublishNow` sources add Approved, result Approved. The submitter gets no "published" notice for their own direct publish.
- 122: staff `rename_account` answers Conflict "That username is reserved until YYYY-MM-DD" for a held name.

## Tests run (owner said: only what proves the change)
Clippy `-D warnings` on sr-db, sr-review, sr-payments, sr-admin (all targets): clean. sr-review: the 5 publish-now tests pass. sr-db Postgres tests (named, `--ignored --test-threads=1`, DB `sr_p60_dsr`): `an_erasure_that_fails_after_the_scrub_leaves_the_account_and_writes_no_row`, `two_concurrent_erasure_runs_do_the_work_once`, `account_deletion_removes_personal_state_and_keeps_contributions`, `deleting_an_account_expires_its_ready_export`, `a_failed_export_hands_the_request_back_atomically`, `a_failed_notice_upsert_advances_no_notice`, `a_staff_rename_to_a_held_username_is_refused`: pass. Not run: whole suites, sr-admin --lib (no change there), fail-on-main runs (argued from the code, not executed). CI runs the rest.

## Proposed inventory rows
- SEC-AUD-116: An erasure run is one transaction with its audit row; a concurrent second run answers Conflict (D-616).
- SEC-AUD-117: Account deletion removes personal state (progress, bookmarks, notes, personal loot tables, saved/private/friends lists, upcoming card claims) and keeps contributions (D-617).
- SEC-AUD-118: A deleted account's data exports expire at once or fail (D-617).
- SEC-AUD-119: Export finish and fail are one transaction each.
- SEC-AUD-120: Patron notice transitions are one transaction.
- SEC-AUD-121: Publish-now claims the submission before publishing; a failure leaves it Approved for a retry (D-618).
- SEC-AUD-122: A staff rename cannot take a username held after an account deletion.

## Proposed threat-model rows
- D-616: Threat: an irreversible erasure exists without its access-log row, or runs twice. Mitigation: single transaction with the audit row, request locked `verified`; tests above.
- D-617: Threat: personal data (progress, notes, private lists, exports) survives account deletion with no purpose. Mitigation: removed in the deletion transaction; contributions stay as Deleted User by decision; bulk self-delete (D-324) covers contributions.

## Doc/copy still listing the old scope
None found. The in-app deletion screen and handbook list only what stays (unchanged). Past-card invite claims remain until the claimants purge job (by design).

## Owner questions
- Should claims on already-finished invite cards also go at deletion (now left to the retention job)?
- A reviewer-approved submission can now also be published by its submitter via publish-now if they hold the publish permission; acceptable?
