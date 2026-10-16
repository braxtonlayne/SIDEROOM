# Handoff: feat/p15-rest-a

Scope: inventory 10-admin, 12-review, 15-live-tv, 16-watch-parties, 11-donations, 14-cinema. Audit first, then build.

## Done
- ADM-CNT-003: ticked after audit (page reorder, replace one page and re-convert exist: `admin/library/PagesDialog.tsx`).
- ADM-CNT-004: cinema rows and hero spotlights already existed (`admin/cinema/CollectionsSection.tsx`); added the missing staff UI for featured reading lists (`admin/library/FeaturedListsCard.tsx`, uses `GET /library/lists/featured` and `POST /admin/library/lists/{id}/feature`).
- REV-ED-001: chapter table in Library > Submissions can rename (inline form), reorder (drag or up/down buttons, done as renumbering through a spare number so no two chapters share a number) and exclude (remove). `PUT /submissions/{id}/library/chapters/{id}` now refreshes the review item label. Logic in `review/chapterEdit.ts`.
- REV-ED-002: folder import rows are editable (include checkbox, chapter number, title, clash warning).
- REV-ED-004: `ValidationSummary` (errors and warnings: title, no chapters, duplicate numbers, chapters without pages, numbering gaps, no synopsis).
- CIN-SOC-003: new `GET /cinema/titles/{title_id}/contributors` (credits were already granted by the cinema submission kinds); the title page Contributors tab uses it. Members who hide contributions or were staff-hidden are left out.
- OpenAPI, api-client types and MSW mocks regenerated; route matrix row added; `KNOWN_CLASHES` untouched (schemas are `CinemaContributor(s)`).
- No migrations were needed (range 0980-0989 unused).

## Still open (unticked) and why
- ADM-GRAPH-003 Canvas graph renderer: large front-end piece, tree/list views cover under 500 nodes.
- ADM-CNT-001: Packages tab waits for the package system (no review routes).
- ADM-PLT-004/005: owner decision (editable default themes, SMTP/OAuth/provider keys in the UI vs operator config).
- ADM-PLT-006: backups last run and disk need new backend probes.
- REV-ED-003: no member artwork upload route (artwork keys are ignored by design); needs backend plus crop UI.
- REV-RV-002: metadata edit-in-place needs a decision on whether reviewers (REVIEW_READING) may call publisher routes.
- TV-ENG-001/003/007, TV-ADM-003/004: tv_schedule_slots, tv_guide_imports, tv_favorites, the imported_guide resolver, fuzzy-matching import and weekly grid are one large vertical slice (DB, sr-tv, API, FE); not started.
- TV-HLS-003 (needs pre-encoded media assets), TV-HLS-005 (needs a real player and Safari).
- WP-BE-008, WP-FE-008, WP-FE-010: in-chat live party card needs a new message type and edit path. WP-FE-012: webcam bubbles.
- DON-PAY-009: needs bitcoind and electrs in CI. DON-ENT-006: remaining call sites belong to other crates. DON-UI-001: public `/support` page unchanged. DON-UI-005: patron badge exists on profiles with the `hide_badge` toggle; avatar ring needs patron status on member objects.
- CIN-PLY-004: JASSUB is LGPL; adding it is an owner decision (seam in `cinema/player/ass.ts`).

## Tests run
- cargo: `sr-api --lib cinema::` (17 pass), `sr-api --test route_matrix` (5 pass), `sr-db --test cinema_repo contributors` (Postgres scratch DB).
- web (vitest, maxWorkers 2): cinema, review, library submissions, admin p13library, i18n catalog. `i18n/firstpaint` fails on the untouched base too (timing).
- tsc and eslint clean on touched dirs.

## Owner questions
- Same as the open items marked "owner decision" above (ADM-PLT-004/005, JASSUB licence, reviewer metadata edits).
- Chapter rename resets the chapter's volume link (the draft chapter view carries no volume id); draft chapters from submissions have none today.
