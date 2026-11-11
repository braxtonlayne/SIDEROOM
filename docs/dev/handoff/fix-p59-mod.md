# fix/p59-mod (wave 59: moderation and admin audit leftovers, D-605, D-606)

Base: `main` `9d83ee2c`. One migration, `2590_audit_append_only.sql`. No new routes, no UI, no new dependencies.

## Done (SEC-AUD-080..088; one checkpoint commit, see `git log fix/p59-mod`)
1. **080 (D-605)** `age.rs::handle_underage`: before anything is suspended, a short transaction loads the case with
   `Access::Work` (NotFound when the caller cannot work it) and runs `graduated::guard_target` (system user, self, and
   staff accounts unless the caller is the Owner). Only then the suspension, then the existing locked transaction.
   **Not atomic**: `CaseActions::suspend_account` is another service's own transaction (its own audit row), so a
   `suspend_account_in(tx)` would need a trait change across crates; the check-before-act order is what closes the hole.
2. **081** `subtree.rs::execute_branch_action`: validates as before, then claims the request with
   `UPDATE moderation_requests SET state='executing' WHERE id=$1 AND state='approved' AND kind='branch_action'`;
   no row = Conflict. The work moved to `run_branch_action`; the final UPDATE requires `state='executing'` (0 rows =
   Conflict, the transaction rolls back). **On an error the claim is handed back to `approved`** so the Operator can
   run it again (the per-node steps skip what is done: banned nodes are no longer in the todo list; a freeze is one
   transaction). A crash mid-run leaves `executing`, which nobody can run twice; an operator resets it by hand.
   The state CHECK is widened in migration 2590 (it did not allow `executing`).
3. **082 (D-605)** classification: a staff account (`is_staff_account`) in a banned subtree gets `review`, never
   `auto_frozen`. Branch freeze and branch ban skip staff unless the caller is the Owner (the node stays as it is).
   The 30-day sweep never removes staff: a due freeze on a staff account (one from an older build) is set `expired`
   and its node goes back to `review`.
4. **083** `holds.rs::mark_hold_reviewed`: one transaction with the UPDATE, a `hold_reviewed` case event (when the
   hold has a case) and a `moderation.hold.reviewed` staff log row.
5. **084** `takedowns.rs::get_notice`: logs `moderation.takedown.read` (Read, target `takedown`) in the same
   transaction as the read.
6. **085** `appeals.rs::decide_appeal_with` (new; `decide_appeal` is a thin wrapper without an unbanner) and the route
   `POST /moderation/appeals/{id}/decision`: the open appeal is locked `FOR UPDATE` for the whole call, the
   `AppealUnbanner` (implemented over `AdminService::unban_person` in the route) lifts the ban inside that lock, and
   the decision is recorded last in the same transaction. A Person who is no longer banned is a no-op
   (`AdminError::Conflict` is swallowed on this path). An error rolls the record back and leaves the ban. If the
   commit itself failed after the unban, the appeal is still open and can be decided again.
7. **086** `search_accounts` always writes an `accounts.search` Read row (terms `q`, `discord_id`, `ip` in the
   detail); the IP search keeps its `accounts.search_ip` row.
8. **087 (D-605)** `link_account`, `unlink_account`, and `move_identity` (target account and the identity's current
   owner, read from `person_detail`) call `guard_account(c, id, false)`.
9. **088 (D-606)** migration 2590: `sr_forbid_truncate()` and `BEFORE TRUNCATE` statement triggers on
   `platform_audit_log`, `audit_log_entries`, `invite_credits_ledger`, `moderation_case_events`,
   `moderation_case_evidence`, `moderation_case_notes`, `takedown_events`, `application_audit`, `payment_audit`
   (`staff_access_log` already had one, 0130); `BEFORE DELETE` on `application_audit` and `payment_audit`.
   - **DELETE stays open only on `audit_log_entries`** (D-563 retention purge, inside the `sr.audit_purge` setting).
     Every other table listed refuses UPDATE and DELETE.
   - Side effect to know: `payment_audit` has `ON DELETE CASCADE` from `payment_invoices`, so deleting an invoice
     now fails. Nothing in the code deletes invoices (checked).
   - `sr-cli restore` drops and recreates; no TRUNCATE anywhere in the code or the test helpers, so no helper
     changed. (`DISABLE TRIGGER staff_access_log_append_only` in the drills is a different trigger and still works.)

## Tests (fail on main)
- `sr-moderation/tests/age.rs::underage_checks_the_case_and_the_staff_guard_before_acting`
- `sr-moderation/tests/graduated.rs::automatic_moderation_leaves_staff_for_review`,
  `a_branch_action_runs_once_under_a_race`
- `sr-moderation/tests/db.rs`: asserts added to the hold test (one log row, one event);
  `an_overturned_appeal_and_its_unban_are_one_decision`
- `sr-moderation/tests/takedowns.rs`: asserts added to the export test (one Read row per `get_notice`)
- `sr-admin/src/tests.rs::every_account_search_is_logged`, `tests_ext.rs::person_graph_actions_honour_the_staff_guard`
- `sr-db/tests/audit_append_only.rs` (TRUNCATE on all ten tables, DELETE/UPDATE on the two audit tables); the
  retention purge test is `settings_repo.rs::an_audit_purge_that_removed_rows_is_itself_audited`.

## Tests run (all green, Postgres scratch DB `sr_p59_mod`)
- `cargo fmt --all -- --check`; `cargo clippy -j 4 -p sr-moderation -p sr-admin -p sr-api -p sr-db --all-targets -- -D warnings`.
- `cargo test -p sr-moderation -- --include-ignored --test-threads=1` (every test file; age 3, db 17, graduated 11,
  takedowns 17, others), `-p sr-admin` (80), `-p sr-db --test audit_append_only --test settings_repo --test admin_repo
  --test admin_ext_repo --test apps_repo --test invoice_retention` (incl. the D-563 purge tests),
  `-p sr-api --lib` (845) and `routes::moderation` with `--include-ignored` (19, the appeal route over Postgres),
  `-p sr-api --test route_matrix` (6), `-p sr-server --test immutable_rules` (25, runs the new migration).
- Not run: the full authz matrix, the whole sr-db suite, web gates (no web code touched). I did not run the new tests
  against `main` to watch them fail; each asserts a behaviour that did not exist (a missing log row, a missing guard,
  a second Ok), so they fail by construction, but that was not demonstrated.

## Proposed threat-model rows (the commander numbers them)
- **Staff self-protection.** Threat: automatic or tool-driven moderation (invite-subtree freeze, branch freeze, age
  tools, person-graph unlink/move) disables or locks a staff account. Mitigation: D-605 (`review` instead of auto
  freeze, `guard_target`/`guard_account` on every tool, sweep skips staff); only the Owner acts on staff.
- **Audit tamper.** Threat: an attacker with SQL access truncates or deletes audit rows. Mitigation: D-606 (UPDATE and
  DELETE refused on all append-only tables, TRUNCATE refused everywhere, the retention purge is the one audited delete);
  residual: a superuser can still drop triggers (the hash chain of `staff_access_log` and `verify-access-log` detect
  it; the other tables have no chain).
- **One-shot decisions.** Branch actions and appeal decisions run once (claim, then act, then record).

## Owner questions
1. A staff account in a banned subtree now waits in `review` forever unless the Owner acts; fine, or should the
   Owner get a notice? (Default: no notice.)
2. `payment_audit` blocks deleting an invoice (cascade). Default: keep the audit rows; a future invoice-erasure path
   would need a sanctioned purge like D-563.
3. The web type `ModRequest.state` (`apps/web/src/moderation/graduated.ts`) lacks `executing`; the value is
   transient. Not changed (no UI in this task).

## Not done
- `suspend_account_in(tx)` (item 1, see above): needs a cross-crate trait change.
