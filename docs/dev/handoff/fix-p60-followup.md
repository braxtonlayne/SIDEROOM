# fix/p60-followup handoff

Done:
- SEC-AUD-127 (D-624): `publish_now` in `crates/sr-review/src/service.rs` skips `check_ready`, `sync_items` and both item moves when the submission is already Approved and `decided_by` is not the caller (a reviewer's approval), and goes straight to `publish_approved`. The direct case (decided by the submitter) still re-runs the item steps. Test `a_submitter_retry_of_a_reviewer_approval_keeps_rejected_items_out` (fails without the fix).
- SEC-AUD-128 (D-625): `install` in `crates/sr-db/src/apps_repo.rs` takes `lock_membership(tx, guild, bot_user)` after the `app_install` lock and before the ban read. Test `a_ban_racing_a_bot_install_never_leaves_a_banned_member` (Postgres, `#[ignore]`): holds the membership lock in a test transaction, checks the install waits, commits a ban, checks the install is refused and the bot is not a member.

Not done: nothing. Left to CI: full suites, fmt check on the whole workspace.

Tests run locally: clippy `-D warnings` on sr-review and sr-db (all targets); the two tests above.

Proposed inventory rows (after SEC-AUD-098 block):
- SEC-AUD-127 | A submitter's publish-now retry of a reviewer's approval publishes only what the reviewer approved (no re-ready, no item re-approval) | D-624 | sr-review
- SEC-AUD-128 | Bot install takes the per-(guild, user) membership lock before the ban check, so a racing ban never leaves a banned bot as a member | D-625 | sr-db apps_repo

Owner questions: none.
