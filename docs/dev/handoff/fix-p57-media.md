# fix/p57-media (wave 57: a silent cartridge shelf is bounded at 7 days)

## Done

### SEC-AUD-060 (D-590)
- `crates/sr-activities/src/arcade.rs`: `DEFER_DELIST_DAYS = 7`. `recheck_cartridges` now also selects whether the
  row's `manifest_deferred_since` is 7 days or older (`overdue`, computed in SQL with `now()`); a transient read on an
  overdue row becomes the new verdict `DelistAged` instead of `Defer`, and is delisted through the existing path (same
  transaction: `delisted_at`, `delist_reason`, `audit_log_entries` action 230). Reason: "its shelf has not answered for
  7 days (last answer: its file <last transient reason>)". `DelistedCartridge` gains `aged` and `why()` (the clause that
  completes "was delisted: "). A successful read at any time marks the row checked, so it never reaches the bound.
  The failed endpoint check stays transient and is now ended by the same bound.
- `DeferredShelf` gains `delist_on` (UTC `YYYY-MM-DD`, earliest deferral of the shelf's rows plus 7 days).
- `crates/sr-server/src/arcade_jobs.rs`: lister DM, staff alert and audit entry use the aged clause; the 24-hour
  "Cartridge shelf not answering" alert now says "Nothing was delisted yet ... they are delisted on <day> (7 days after
  the first failed check)". No new alert between. The hourly re-arm is unchanged and ends once nothing is deferred.
- Docs: doc 15 T59 residual text, doc 11 sentence, inventory row SEC-AUD-060 (ticked).
- No migration, no route/schema change (no OpenAPI or authz matrix change), no web change.

Commits: `SEC-AUD-060: delist a cartridge whose shelf has not answered for 7 days (D-590)` plus a follow-up fixing a test
helper (dynamic SQL refused by sqlx 0.9).

## Not done
Nothing of the row.

## Routes shot
None: no page changed (the audit-log line for action 230 and the alert card are existing components).

## Tests run (DB `sr_p57_media`, own target, inside the heavy lock; DB dropped)
- `cargo test -p sr-server --lib arcade_jobs -- --include-ignored`: 10 passed, including the new
  `a_cartridge_deferred_for_six_days_and_23_hours_stays_listed`,
  `a_cartridge_deferred_for_seven_days_is_delisted_with_dm_audit_and_alert`, `an_answer_in_between_resets_the_clock`,
  `the_24_hour_alert_names_the_delist_day`.
- `cargo test -p sr-activities --test arcade -- --include-ignored --test-threads=1`: 8 passed.
- `cargo clippy -p sr-activities -p sr-server --all-targets -- -D warnings`: clean. `cargo fmt --all` applied.
- `node --test scripts/threat-model.test.mjs`: 5 passed; `check-doc-links`: ok.

## Owner questions (defaults taken)
- The delist day is shown in UTC, as a date only.
- The aged delisting DM says "its shelf has not answered for 7 days (last answer: its file ...)" then the usual "List it
  again once the file is fixed."; the staff alert also names the file path.
- Threat-model: T59's text was updated in place; no new row proposed.
