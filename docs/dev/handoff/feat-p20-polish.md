# Handoff: feat/p20-polish (phase 20, small items left open by phase 19)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p20-polish`, branch `feat/p20-polish`, base `integration/phase20` (= main). Local commits only. No migration used (range 1170 to 1179 untouched), no job id, no audit id.

## Done

1. **Age buttons on item pages.** `apps/web/src/moderation/AgeRestrictionButton.tsx`: "Mark 18+" or "Remove 18+ mark" next to the Report button on the Library series page and the Cinema title page. It shows only for staff whose `/admin/me` bits the route accepts (`MANAGE_CASES`, `DELETE_CONTENT`, or `REVIEW_READING` for a series, `REVIEW_MEDIA` for a title); a failed `/admin/me` means no button. A dialog asks for a required reason (up to 500 characters) and calls the existing `POST /moderation/age/restrictions`; the page refetches on success; 403/404/other errors have their own text. Strings in `i18n/messages/en/p20-polish-1.ts`.
2. **Search indexes follow the flag at once.** New `sr_moderation::ItemIndexSink` (called after the commit when a series or title really changed), `LibraryService::reindex_one` and `CinemaService::reindex_one`, and `sr-server/src/moderation_index.rs` (`CatalogIndexSink`) wired into `Moderation` in `app.rs`. A failed refresh is logged and the boot rebuild still catches up.
3. **Reports tile in the Review hub.** New `GET /moderation/dashboard/inbox/count` (`MANAGE_CASES`, returns the dashboard's `DashInbox`, access-log action `moderation.inbox.count`), so the hub does not build the whole dashboard. The tile sits next to the Signals tile, hidden at zero or on failure, and links to `/admin/moderation?tab=dashboard` (new deep link in `ModerationSection`). Counts D-339 unattached reports.
4. **"Move a message" (misplaced NSFW): closed by decision, not built.** The spec only says "move or delete"; Discord has no message move; a repost-then-delete would copy the reported content under new ids and lose reactions, replies and attachment provenance. Delete plus warn (already built) stays. One-line note added to spec 17.3 and to inventory MOD-AGE-002.

Docs: spec section 17 (marking, misplaced NSFW, "Not built" line for the Reports count), inventory `19-moderation.md` (MOD-AGE-001, -002, MOD-DASH-002). Generated: OpenAPI, api-client schema, MSW mocks (958 operations); route matrix row added (`staff`/`deny`, no `?`).

## Checked, nothing else small and open

- p19-mod-age: the member-directory/message-search hiding bullet is closed by D-343; mandatory external reporting waits on the lawyer (D-336 h); not small.
- p19-mod-dash: `MOD-AGE-003` is built (integration); the retention check not counting guilds waiting on held messages or held library/cinema rows is deliberate (not purge-job skips); browser check not allowed.
- p19-mod-sus: Discord-identity signal closed by D-348; "open a case from a signal" is built (`link-case`, P-12); Signals count in the hub built (P-15); MANAGE_ACCOUNTS-only tab visibility built (P-16). The remaining open item in all three is only the lawyer checks (D-341, D-343, D-347), which are owner work.

## Tests run

- `cargo fmt --all --check`; `cargo clippy -j 2 -p sr-moderation -p sr-api --all-targets -- -D warnings`.
- Postgres on own scratch DB `sr_p20_polish` (dropped at the end): `sr-server --lib moderation_index` (new, library and cinema search hide/show a series and title as the flag flips, no rebuild), `sr-moderation --test dashboard` (8, count added), `sr-api --lib routes::moderation` (19, count route in the denial matrix and access log).
- `sr-api --test route_matrix` (5), `openapi_names`.
- Web: new Vitest files `library/__tests__/p20polish`, `cinema/__tests__/p20polish`, `admin/__tests__/p20polish` (11 tests). Full gate results are in the final report.

## Open owner questions

None new. (Decision 4 above is the conservative default; say if a repost action is wanted after all.)
