# fix/p56-media (wave 56: the cartridge manifest re-check tells a definite answer from a transient one)

## Done

### SEC-AUD-052 (D-582)
- `CartridgeObjects::read` (`crates/sr-activities/src/arcade.rs`) now returns `Result<Vec<u8>, CartridgeReadError>`,
  `Definite(reason)` or `Transient(reason)`. The shelf reader (`crates/sr-api/src/routes/arcade.rs`, `ShelfFiles`):
  no mount and `StorageError::NotFound` are definite; a failed mount lookup, a failed endpoint check (`vet_endpoint`), an
  unbuildable client and any other backend error are transient. A listing (`add_cartridge`) refuses either one with the
  reason, as before.
- `ArcadeService::recheck_cartridges(objects, limit, run)`: no object on the shelf, empty or oversized file, not a
  zip, no manifest and no tier are definite (delisted as built). A transient read **defers** the row: it stays
  unchecked and listed (`manifest_defer_shelf`, `manifest_deferred_since` kept from the first deferral,
  `manifest_defer_run`, `manifest_defer_reason`; migration `2500_arcade_cartridge_deferral.sql`). `run` names one run
  of the job so a row deferred in it is skipped by that run's later passes (passes still end on an empty one).
  `RecheckPass.deferred` counts them; `deferred_cartridges()` says whether any remain.
- `stale_deferred_shelves()`: shelves whose rows have been deferred 24 hours (`DEFER_ALERT_HOURS`) and not yet told
  about; marks every deferred row of the shelf told in the same statement, so a shelf is returned once.
- Delisting (same transaction as the `delisted_at` update): a `audit_log_entries` row for the server, new
  `AuditAction::ArcadeCartridgeDelisted = 230` (sr-chat; the system user acts, target = the cartridge id, options
  `{event: "arcade.cartridge_delisted", cartridge_id, title, source_ref, listed_by}`, reason "its file ..."). The web
  audit-log text for 230 is in `apps/web/src/i18n/messages/en/p56-media-1.ts` + `auditText.ts`.
- Job (`crates/sr-server/src/arcade_jobs.rs`): each delisted cartridge's lister gets one system DM (via the arcade DM
  slot, the Staff Server voice): cartridge, server, reason, "List it again once the file is fixed." The staff alert
  per delisted cartridge stays. After the passes, one new staff alert per stale shelf
  (`StaffAlertKind::CartridgeShelfDeferred`, "Cartridge shelf not answering", links `/admin/arcade`). While any row is
  deferred the handler calls `jobs::reschedule_running(.., 1 hour)`, so the singleton re-arms hourly; with none left it
  ends as before. `register` now also takes the pool and the DM slot (`app.rs`).
- Docs: doc 11 (the D-579 sentence), threat row T59 residual-risk text, inventory row SEC-AUD-052.

Tests (names that fail without the change):
- `sr-server arcade_jobs::tests` (Postgres): `a_missing_object_is_delisted_with_a_dm_and_an_audit_row` (NotFound
  delists, one DM to the lister with cartridge/server/reason/list-again, one audit row action 230, nothing repeated on
  the next run), `a_backend_error_defers_the_cartridge_and_delists_nothing` (still listed, unchecked, no DM, no audit,
  no alert; a later definite answer delists), `a_shelf_deferred_for_a_day_raises_exactly_one_alert` (two cartridges on
  one shelf, nothing at day 0, exactly one alert after 25 hours, none on the next hourly run, nothing delisted),
  `an_unreadable_old_cartridge_is_delisted_with_one_staff_alert` (kept, updated), plus text tests for the DM and alert.
- `sr-activities/tests/arcade.rs` updated to the new trait and signature (the wave 55 re-check test still passes).

## Not done / left
- Nothing of the row. No route or schema type changed: no OpenAPI regeneration, no authz matrix change.
- Not run: full `cargo test -p sr-server` / `-p sr-api` suites; Playwright (no page changed).

## Routes shot
None: no page or dialog changed. The only visible additions are a new amber staff alert card ("Cartridge shelf not
answering", built by the existing `alert_card`) and the audit-log line for action 230 in a server's audit log
(existing list rows).

## Proposed threat-model row
T59's residual-risk text was updated in place (see the diff); no new row.

## Owner questions (defaults taken)
- The lister DM is sent straight through the system sender, without the recipient's DM settings gate (Message
  Requests, blocks) that the Coming Soon release notice uses: it is a notice about their own listing, like the takedown
  register's. Say if it should go through the gate.
- The audit entry's actor is the system account (id 1), not the lister.
- A shelf that stays down keeps its cartridges listed (unchecked) indefinitely; staff are alerted once at 24 hours,
  not again.
- A failed endpoint check counts as transient, as the row says, even though a shelf pointing at a private address
  would never recover on its own.

## Tests run (DB `sr_p56_media`, own target, inside the heavy lock)
- `cargo clippy -j 4 -p sr-activities -p sr-chat -p sr-events -p sr-api -p sr-server --all-targets -- -D warnings`: clean.
- `cargo test -p sr-activities --test arcade -- --include-ignored --test-threads=1`: 8 passed.
- `cargo test -p sr-server --lib arcade_jobs -- --include-ignored`: 6 passed (4 Postgres-backed).
- `cargo test -p sr-chat --lib audit`: 11 passed (code 230 pinned); `cargo test -p sr-db --test migration_versions`: passed.
- `cargo fmt --all -- --check` clean; web: `catalog.test.ts` 8 passed, prettier on the 2 web files, `i18n-audit` 0,
  `threat-model.test.mjs` 5 passed, `check-doc-links` ok.
