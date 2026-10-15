# Handoff: feat/p11-review (phase 11, Submissions and Review)

Base: integration/phase11. Migrations 0700 and 0701.

## Done
- REV-CORE-006 contribution credits. `contribution_credits` (unique per section and item, so replacements and re-publishes earn nothing). Granted in `publish_now` and `publish_approved`; best effort, a failure is an internal `credit_failed` timeline note. `SubmissionKind::credit_refs` lets a kind exclude replacements (default: all published items). Revoke: `ReviewService::revoke_credits` + `POST /admin/review/submissions/{id}/revoke-credits` (publish permission + reason, access-logged); reads: `GET /submissions/{id}/credits`, `GET /admin/review/submissions/{id}/credits`.
- REV-CORE-008 upload allowance. 50 GB per UTC day, staff exempt, atomic guarded upsert in `submission_upload_bytes`. Charged in the Library begin-upload route after the Library accepts the file. `GET /submissions/quota`.
- REV-CORE-009 housekeeping. Hourly job `review.housekeeping` (singleton id 701). Warns at 23 idle days (`SR_SUBMISSION_UPDATE` type `stale_warning`), abandons drafts at 30 days only after a warning at least 3 days old, purges rejected submissions after 14 days. New trait hook `SubmissionKind::purge(s, items)`; implemented for the Library kinds (deletes the draft series or unpublished chapters; `library.gc` then removes files). Default is a no-op, so the Cinema kinds keep their files (see open).
- REV-UP-007 naming parsers `sr_media::naming` (video) and `sr_reader::naming` (library), regex based, 700+ and 300+ fixtures including generated matrices and hostile input.
- REV-UP-008 `POST /submissions/naming/parse` and `review/NamingGuide.tsx` mounted in Library > Submissions.
- REV-REQ-002 requests board `review/RequestsBoard.tsx` at `/library/requests` (Cinema links there too); backend gained request claims ("I'll upload this" with note): `PUT|DELETE /requests/{id}/claim`, `ContentRequest.claimed_by/claim_note/claimed_by_me`.
- Timeline labels for the new events.

## Still open (not built)
- REV-CORE-001 remainder: all tables exist now (credits added); tick left to the owner after review of the `requests`/`request_votes` naming (they are `content_requests` / `content_request_votes`).
- REV-CORE-007: system-user DM and Staff Server card (no system-user or Staff Server channel model to post into yet).
- REV-UP-001/002 resumable multipart + IndexedDB upload manager, REV-UP-003 folder drag and drop, REV-UP-004 archive pre-inspection, REV-UP-005 watched folders, REV-UP-006 storage import: large, need owner decisions on the multipart protocol and the staff scanner config.
- REV-ED-001..006 metadata editor, lookup providers (need operator API keys/feature flags), duplicate detection.
- REV-RV-002 remainder (video proxy player, package sandbox, manifest diff, edit-in-place), REV-RV-003 duplicate panel (depends on REV-ED-006), REV-RV-005 Staff Server card.
- Cinema kinds do not implement `purge` or `credit_refs`; the Cinema agent should override both (episode replacements must not earn credit).
- Naming tester is not yet in Admin > Ingest Sources, and the parsers are not yet used to propose structure on upload (REV-ED-002).
- Requests: no fulfil-from-board UI, no staff decline button on the board.

## Tests run
- `cargo test -p sr-review` (38), `-p sr-media naming`, `-p sr-reader`, `-p sr-api --lib review` (15), `route_matrix` (regenerated, `?` cells set to staff/other).
- Postgres: `cargo test -p sr-db --test review_repo -- --include-ignored --test-threads=1` on a scratch DB (8 passed), dropped afterwards.
- clippy -D warnings on sr-review, sr-db, sr-api, sr-media, sr-reader, sr-library, sr-server; fmt --check.
- Web: vitest for review, library, i18n; tsc, eslint, prettier on touched files; `vite build` + `pnpm budget` pass.
- OpenAPI, api-client schema and mocks regenerated.

## Decisions
- Credit key is (section, item ref); a revoked credit stays so the same item cannot be credited again.
- Idle = latest of the record change and any timeline event except `stale_warning`. Library chapter edits do not touch the submission record, so a draft being built only through chapter uploads can look idle; a warning gives 7 days to touch it.
- Abandoned drafts become `withdrawn` (no new state) and are purged immediately.
- Quota is reserved at slot creation (an abandoned upload still counts that day).
- Shared files touched outside review: `library/routes.tsx` (requests route), `library/Submissions.tsx` (NamingGuide), `sr-library/src/submissions.rs` (purge), `sr-api` library `begin_upload` (quota).

## Owner questions
- Should the daily limit be an operator setting (platform_settings) instead of the constant 50 GB?
- Should abandoned or rejected Cinema submissions purge uploaded media too (needs the Cinema agent's storage layout)?
