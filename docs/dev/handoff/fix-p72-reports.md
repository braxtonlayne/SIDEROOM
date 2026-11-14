# Wave 72, reports (SEC-AUD-277..279; D-758, D-749)

Branch `fix/p72-reports`. No migration, no new route, no new string (the page already had "Unknown").

## Done (commits)
- `3be82f27` SEC-AUD-277: a missing or non-boolean `online` is stored as `null` (`allow_list.rs`, module comment, `docs/05-api-and-gateway.md`); `contract.ts` says `boolean | null`; `BugReportsSection.tsx` reads `null` and `undefined` as "Unknown". Follow-ups: the shots spec `e2e/tests/p72-reports-shots.spec.ts` (local only, `SR_P72_REPORTS=1`) and a rustfmt commit.
- SEC-AUD-279: `deploy/INSTALL.md` ("Running it again") and `deploy/FAQ.md` carry D-749's sentence; `CHANGELOG.md` has a Phase 72 section and `deploy/UPGRADE.md` a wave 72 section.
- SEC-AUD-278: handoff only (table below); nothing else committed.

## Item 1: confirm tests and proof
- Rust: `allow_list/tests.rs`: `a_report_with_only_a_description_is_stored_with_every_key` and `an_invalid_part_is_stored_as_the_neutral_value_not_dropped` (their `online == true` assertions turned to `is_null()`), new `online_is_null_when_missing_or_not_a_boolean_and_as_sent_otherwise` (missing, `"true"`, `1` give null; `true` and `false` as sent); `bugs/tests.rs`: the route test stores `null` and staff read the report back. `cargo test -p sr-api --lib -- routes::bugs`: 42 passed. **I did not run the Rust tests on the base**: the old line was `unwrap_or(true)`, which the turned-round assertions contradict by reading.
- Web: `p71BugReports.test.tsx`, new describe: `online: null`, a row without the key, `true` (Yes), `false` (No). **Watched failing on the base** (BugReportsSection change taken out): the `null` test failed (the page printed Yes). 9 tests pass with the change.
- Other readers of `online`: only `bugs/mod.rs:278` (the request body type `Option<bool>`; sanitize reads raw JSON) and the staff page; no export or mail uses it. Nothing fails on `null`.
- `openapi.json` and the client: regenerated (`sr-server openapi`), **byte-identical** (the bundle is free JSON there): nothing changed, nothing committed.
- clippy `-p sr-api --all-targets -D warnings`: clean. `cargo fmt --check` found one line, fixed. tsc, eslint, prettier on my files and `i18n-audit --max 0`: clean.
- Shots: `visual-sweep-shots/p72-reports`, 18 PNGs, all opened. Detail view of reports without `online` (description only, `online` missing, `online` as a string: Editorial desktop and phone), `online` true (Editorial, Lounge Light, Lounge Dark, desktop and phone), `false` (Editorial, desktop and phone), and "missing" in all three themes at both sizes. The Online row reads Unknown / Yes / No as expected in every one; layouts match the Admin area. The spec asserts the text too and passed on the real stack.

## Item 2 (SEC-AUD-278): wave 71's library tests against the old source
Source taken out in my worktree, tests kept, migration kept, then restored (`git status` clean).

| Fix | Test | Without the fix | Message |
|---|---|---|---|
| 253 `normalise_tag` (made old-like: no short form, no script case; the function is new, so a reverse patch would not compile) | `sr_core language::tests::normalise_tag_cases` | FAILED | left `Some("eng")`, right `Some("en")` |
| 253 `sr_library::rules::language` old body | `rules::tests::languages_normalise` | FAILED | left `"eng"`, right `"en"` |
| 253 same revert | `sr-db library_repo::the_language_filter_finds_a_series_whose_edition_was_stored_as_eng` (Postgres) | FAILED | last step (a new `eng` edition must meet the old one): left `[]`, right `[series id]` |
| 253 `sr_cinema::rules::language` old body | `a_three_letter_language_code_is_stored_in_the_short_form` | FAILED | left `"zh-hant"`, right `"zh-Hant"` |
| 253 migration 3050 | `migration_3050_rewrites_languages_...`, `the_sql_pairs_and_the_rust_table_are_the_same_list` | could not be watched this way | The fix is the SQL and the test runs the SQL itself. Not run. |
| 256 `S3Multipart::abort` (reverse patch) | `abort_of_an_upload_the_store_no_longer_knows_is_done` | FAILED | `unwrap()` on an error for the 404 (`s3_multipart_mock.rs:179`) |
| 256 | `abort_that_fails_for_another_reason_is_an_error`, `abort_sends_a_signed_delete` | passed | Guards of behaviour that did not change: expected. |
| 254 `package_upload.rs` (reverse patch; the constant `CHECK_ALLOWANCE_HOURS` kept, the tests import it, so the first attempt did not compile) | `complete_can_be_asked_again_after_the_row_failed_to_move` | FAILED | `Err(Activity(Storage("NoSuchUpload")))` |
| 254 | `two_complete_calls_at_once_both_succeed_and_queue_one_check` | FAILED | same `NoSuchUpload` |
| 254 | `giving_an_upload_up_leaves_nothing_in_the_store_in_every_state` | FAILED | "the assembled object is deleted" |
| 254 | `the_sweep_frees_the_object_of_an_upload_the_row_never_moved_on`, `a_check_that_cannot_be_queued_frees_the_staged_object` | FAILED | `!r.there(&key)` (object still in the store) |
| 254 | `a_check_queued_late_is_not_expired_under_it` | FAILED | "0.1666 hours left, wanted about 6" |
| 254 | `complete_with_a_part_missing_is_still_refused_and_the_upload_stays_open` | passed | Guard (a refusal that stays); not a finding. |
| 255 `sr-cli bootstrap` (hand-made: the guard back to "any real account is refused", the advisory lock removed; the tests live in the same file, so a reverse patch was impossible) | `a_stopped_bootstrap_is_finished_by_the_next_run` | FAILED | "this instance is already bootstrapped (accounts exist)" |
| 255 | `a_half_made_staff_server_is_finished_not_duplicated` | FAILED | same |
| 255 | `two_bootstraps_at_once_one_wins` | FAILED | "username or email already in use" (the second run lost on a unique key, not on the lock) |
| 255 | `only_the_first_runs_owner_can_finish_a_half_made_installation`, `a_weak_password_writes_nothing` | passed | The first asserts refusals, and the old code refused everyone; the second: the old code also checked before writing. They guard the new door and the ordering; they do not detect the old fault. By design, but they are not "watched failing". |

Finding: no library test passes where it should fail. Caveat: the bootstrap "old" is my reconstruction of the old behaviour, not the old file.

## Item 3 (SEC-AUD-279) and docs
- `deploy/INSTALL.md` ("Running it again") and `deploy/FAQ.md` ("stopped part way"): D-749's sentence (password asked again; lost and no data yet: start again with an empty database), once in each.
- Greps for the 6 hours (`docs/05` already says six hours), the typing 204 for a blocked DM (`docs/05` line 628 already says it) and `has_more` (documented): nothing disagrees.
- `check-doc-links`: ok.
- Changelog clauses and sources: Members (prose and status handoff sentences), Staff (my item 1), Developers and Library (uploads handoff sentences), Operators (my item 3; "no migration" from every handoff). Read from `p72-integ/docs/dev/handoff/fix-p72-{prose,status,uploads}.md`. Missing: none.

## Proposed inventory rows
- SEC-AUD-277: a bug report whose browser sent no `online` is stored as `null` and shown as Unknown (D-758). Done; tests `online_is_null_when_missing...`, `p71BugReports.test.tsx`.
- SEC-AUD-278: wave 71's library tests watched failing without their fixes: table above. Done.
- SEC-AUD-279: wave 72 changelog and upgrade notes; D-749 install sentence. Done.
- Threat model: none.

## Notes
- A cross-worktree `git stash` put the status builder's `packages/ui/src/tokens/themes.ts` into my tree; I never committed it and restored the file.
- Cleanup: `sr_p72_reports` and `sr_p72_reports_ui` dropped; `p72-reports-target` and `p72-reports-bin` deleted. Scratch notes in `p72-reports-scratch` (untracked, outside the repo).

## Owner questions
1. Old rows stored as `online: true` that were really unknown stay so (D-758); there is no way to tell which. Accept?
