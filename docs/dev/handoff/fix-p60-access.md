# fix/p60-access (wave 60: sign-in abuse, ops footguns, audit-first staff changes, D-612 to D-615)

Base: main `34912d1a`. No migrations, no new dependencies, no UI, no route shape changes (OpenAPI unchanged).

## Done
- **SEC-AUD-109 (D-612).** `second_factor` (`routes/auth/login.rs`) also checks, fails and clears an account-wide key
  `mfa:<user>` (no address) on `account_lock`, next to the per-address `mfa:<user>\n<ip>` key. It is the only route that
  consumes a pending login with a guess: `passkeys::login_begin` only issues options and the proof is checked through
  the same `second_factor`; step-up (`identity.rs`) keeps its own lock. Tests (`auth/tests.rs`):
  `second_factor_guesses_are_limited_per_account_across_addresses`, `a_correct_second_factor_clears_the_account_budget`.
- **SEC-AUD-110.** `reset` loads the account after consuming the link; anything but `Active` answers the same
  "invalid or expired" form error and changes nothing. Test
  `a_reset_link_minted_before_a_suspension_does_not_change_the_password`.
- **SEC-AUD-111 (D-613).** `DisabledMailer` (`routes/auth/mail.rs`): sends nothing, logs subject and the recipient's
  domain at warn. `sr-server/src/app.rs` `pick_mailer(debug, smtp_host) -> MailerKind`: SMTP when a host is set, else
  `LogMailer` in a debug build and `DisabledMailer` in a release build, with one startup warning. Tests:
  `disabled_mailer_logs_no_body` (tracing capture; the link, the local part and the host are absent),
  `release_without_smtp_uses_the_disabled_mailer`. `sr-cli` keeps its own `LogMailer` (operator terminal, not a server log).
- **SEC-AUD-112 (D-614).** `sr-cli seed --password` is now optional. Pure helper `seed_password(local, force, given)`:
  local keeps the dev default; a non-local instance needs `--password` of 12+ characters other than the dev password
  (the existing "non-local needs `--force`" refusal stays). Used by `seed` and `seed_scale`. Tests:
  `a_forced_remote_seed_without_a_password_is_refused`, `..._with_the_dev_password_is_refused`,
  `a_local_seed_keeps_the_dev_default`.
- **SEC-AUD-113 (D-615).** `PATCH /admin/features/{id}`: `FeatureService::validate_platform` (unknown feature, scope,
  schema) runs first, then the `features.patch` row, then `FeatureService::set_platform(id, enabled, settings)` applies
  both as ONE `write` of one record. If the apply fails a `features.patch_failed` row is written (best effort) and the
  error answered. Reason stays optional. Tests (`routes/admin/tests.rs`):
  `a_feature_patch_with_a_bad_setting_changes_nothing`, `a_feature_patch_writes_its_audit_row_before_applying`.
  Outside my files, additive: `sr-admin` `MemoryAdminRepo::set_log_failing(bool)` (test double: makes `append_log` fail).
- **SEC-AUD-114 (D-615).** `PublisherCtx` (`routes/library/mod.rs`): `audit_first` (row before the change, request fails
  if it cannot be written), `audit_failed` (`<action>.failed`, best effort), `audited` / `audited_with` (row, change,
  failed row on error), `audit_created` (row after a create, 500 if it cannot be written), `audit_note` (extra
  best-effort row). The old swallow-the-error `audit` is gone. Every staff write in `library/{admin,comments,social,sources,uploads}.rs`
  uses one of them (25 call sites).
  Audited FIRST: series update/delete/publish/unpublish/merge, edition update/delete, volume delete, chapter
  update/delete/publish/unpublish, page replace, pages replace, list feature, ingest source create (id is allocated
  before the call), update, delete, scan (a best-effort `ingest_source.scan.result` row carries the counts), upload begin.
  Row AFTER the change, 500 if it cannot be written (target id or detail exists only after the change):
  `library.series.create`, `library.edition.create`, `library.volume.create`, `library.chapter.create`,
  `library.upload.complete`, `library.upload.abort`, `library.comment.report.resolve`.
  Note: the row is now written before the permission/not-found check inside `LibraryService`, so a refused request leaves
  a row plus a `.failed` row (staff-only routes; harmless per D-615).
  Test (`routes/library/tests.rs`): `a_library_staff_delete_is_refused_when_the_access_log_cannot_be_written` (delete
  refused with 500 and the stub never saw the delete; a create gives 500; with the log back the delete works and is logged).
- **SEC-AUD-115.** Comment on `reauthenticate` (`routes/auth/mfa.rs`) says it is a separate per-account key `reauth:<id>`
  with the same back-off policy.

## Not done
- Nothing dropped. See the test section for what was run.

## Tests run
Per the owner's instruction (CI runs the suites), only the added/changed tests by name: `cargo test -p sr-api -p sr-server -p sr-cli -p sr-features --lib --bins -- <names>` (all 13 pass: 9 sr-api, 3 sr-cli, 1 sr-server) plus the existing `second_factor_needs_pending...` and `reset_flow...` tests; `cargo clippy -p sr-api -p sr-server -p sr-cli -p sr-features -p sr-admin --all-targets -- -D warnings` clean; `cargo fmt --all -- --check` clean. Not run locally: full suites, route_matrix, authz matrix (no route signature changed).

## Proposed inventory rows (after SEC-AUD-098)
- [x] **SEC-AUD-109** [BE] Wrong second-factor answers also count against the account, whatever address or pending login they come from; a correct answer clears it (D-612). Handoff `fix-p60-access.md`.
- [x] **SEC-AUD-110** [BE] A password-reset link only changes the password of an active account; a link minted before a suspension or ban answers as expired. Handoff `fix-p60-access.md`.
- [x] **SEC-AUD-111** [BE] A release build with no `[smtp]` host uses a mailer that sends nothing and never logs a mail body (subject and recipient domain only, with a startup warning); debug builds keep the log mailer (D-613). Handoff `fix-p60-access.md`.
- [x] **SEC-AUD-112** [BE] `sr-cli seed` on a non-local instance needs `--force` and its own `--password` (12+ characters, not the built-in development password) (D-614). Handoff `fix-p60-access.md`.
- [x] **SEC-AUD-113** [BE] A platform feature change is validated completely, audited, then applied as one write of one record; a failed apply adds `features.patch_failed` (D-615). Handoff `fix-p60-access.md`.
- [x] **SEC-AUD-114** [BE] Library staff changes write their access-log row first and are not made if it cannot be written; creates write it right after and answer 500 if it is missing (D-615). Handoff `fix-p60-access.md`.
- [x] **SEC-AUD-115** [BE] The re-auth lock comment names its own `reauth:<id>` key. Handoff `fix-p60-access.md`.

## Proposed threat-model updates (docs/15, the commander numbers them)
- **T1 (brute force), D-612:** add to the mitigation: "wrong second-factor answers also count against the account across
  addresses and pending logins (`crates/sr-api/src/routes/auth/tests.rs::second_factor_guesses_are_limited_per_account_across_addresses`,
  `crates/sr-api/src/routes/auth/tests.rs::a_correct_second_factor_clears_the_account_budget`, SEC-AUD-109)". Residual: an
  attacker who knows the password can lock the owner out of the second factor for up to the back-off cap (15 minutes);
  the owner can still reset the password. Accepted.
- **T13 (secrets in logs), D-613:** add: "a release build without `[smtp]` uses `DisabledMailer`, which logs only the subject and the recipient's domain, so
  reset and verification links never reach a log (`crates/sr-api/src/routes/auth/mail.rs::disabled_mailer_logs_no_body`,
  `crates/sr-server/src/app.rs::release_without_smtp_uses_the_disabled_mailer`, SEC-AUD-111)". Residual: such an install sends no mail.
- **T10 (staff abuse), D-615:** add: "a platform feature change and every Library staff change write their access-log row before the
  change and are not made without it (`crates/sr-api/src/routes/admin/tests.rs::a_feature_patch_writes_its_audit_row_before_applying`,
  `crates/sr-api/src/routes/library/tests.rs::a_library_staff_delete_is_refused_when_the_access_log_cannot_be_written`,
  SEC-AUD-113/114)". Residual: creates and upload complete/abort/report-resolve write the row right after the change (the target id exists only then) and answer 500 if it is missing.

## Owner questions
- None blocking. Choices made: the per-address second-factor key stays next to the new account-wide one; the library
  audit-first rows are written before the in-service permission check (a refused staff request leaves a row and a
  `.failed` row); the feature-patch reason stays optional (a required reason is a UI change).
