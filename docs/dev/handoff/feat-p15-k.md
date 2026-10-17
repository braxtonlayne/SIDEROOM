# Handoff: feat/p15-k (answers follow-ups K-04, K-05, K-06, K-07, K-11, K-12)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p15-k`, branch `feat/p15-k` (base integration/phase15 with feat/p15-answers, feat/p15-rest-a and feat/p15-browser merged). Local commits only, nothing pushed. No migrations (1010 to 1019 unused).

## Done

- **K-04 (D-252) Activity serving gate.** `crates/sr-activities/src/service.rs`: `lookup`, `shelf` and `may_start` treat an application as served when it is not suspended and is `released`, or `in_review` with an approved version in `application_versions`. Development stays as before. Spec developer-platform 3.10 and doc 11 reworded. Test `in_review_with_a_prior_release_keeps_serving_and_never_released_does_not` (Postgres).
- **K-05 (D-253) System cards.** Audit showed (a) review cards already close on every decision path (`on_review_card` from the review service), and (d) the press handlers already answer a closed card. Done now:
  - (b) `StaffVoice::close_stale_resume_cards` (`staff_feed.rs`, run every 60 s from `app.rs`) closes the incident "Resume invites" card with "Expired: invites are open again." or "Invites were resumed." once `invites_paused` and the timed pause are both over. There is no job that clears `invites_disabled_until` (it is time based), so the sweeper looks at the system user's recent messages. Test `a_resume_invites_card_closes_once_when_the_pause_is_over`.
  - (c) `StaffVoice::rewrite` now calls `routes::messages::system::rewrite_system_card` (which gained a `content` argument), the same function the AutoMod and incident presses use. One implementation.
  - (e) `lounge/msg/Components.tsx`: the per-page `staffDone` state and `isOnce` are removed; the server text is a toast and the buttons come and go with `MESSAGE_UPDATE`. Test updated in `staffCards.test.tsx`.
  - Doc 05 paragraph added.
- **K-06 (D-255) Reviewer edit-in-place.** `ReviewService::editable_reviewer` (in review, kind review permission, not the submitter, live claim; a claim held by someone else is a 409 naming the account id), `editable_for` (submitter or reviewer), `note_reviewer_edit` (`metadata_edited {by, fields}` timeline event, visible to the submitter), `readable_for` (submitter or reviewer may read the draft chapter list). The library draft routes `update_series`, `create_chapter`, `update_chapter`, `delete_chapter`, `set_cover` use it; uploads stay submitter only. Web: `admin/sections/ReviewerEdit.tsx` on the review page (badge "Editing as reviewer", claim time, series form, cover, chapter edit/remove), timeline label. Tests: `a_reviewer_with_the_claim_edits_the_draft_in_place` (Postgres), route test in `routes/review/tests.rs`, `reviewerEdit.test.tsx`. REV-RV-002 ticked.
- **K-07 Chapter rename keeps its volume.** Found a real bug on the way: the server cleared `volume_id` and the web sent format `images` for epub chapters. `LibraryDraftOps::update_chapter` now keeps the chapter's volume (unless another volume of its edition is named), its format, and the store keeps the source key. `create_chapter` may name a volume (the store validates the edition). Web: `chapterUpdateBody` (`review/chapterEdit.ts`), `review/ChapterRename.tsx` with a volume select (existing series only), `DraftChapterList.volumes`. Tests: `a_chapter_rename_keeps_its_volume_and_format` (Postgres), `chapterEdit.test.ts`, `reviewerEdit.test.tsx`, `submissions.test.tsx`.
- **K-11 Member artwork.** `POST /submissions/{id}/artwork/{kind}` (`routes/review/artwork.rs`; `SubmissionKind::artwork_slots`/`set_artwork`; `ReviewService::set_artwork`): `cover` for `library.series` (reuses the G-78 cover logic), `poster` (600x900) and `backdrop` (1920x1080) for `cinema.title` (`CinemaService::set_title_artwork`: sniff JPEG/PNG/WebP, re-encode, caps 2 and 4 MiB, key stored on the draft title, which is the object publishing makes visible). Same access rule as K-06. Web: `review/ArtworkField.tsx` with the existing canvas crop dialog, used in the submitter's series editor and the reviewer panel. Tests: `member_artwork_goes_through_the_submission_rule`, `a_member_sets_the_poster_and_backdrop_of_a_draft_title` (Postgres), route test, `artwork.test.tsx`. REV-ED-003 ticked.
- **K-12 (D-254) Defaults and integrations policy.**
  - `crates/sr-features/src/platform_defaults.rs`: `platform.appearance` (group `default_theme_by_section` with `lounge, entrance, cinema, library, support, tv`, values the six shipped theme ids or empty for "shipped default") and `platform.integrations` (groups `unfurl`, `gif`, `lookup` with the non-secret keys). Both are platform scope and show on Admin > Features (sections "Appearance" and "Email & Integrations").
  - `GET /sr/appearance/defaults` (public, because the entrance is themed before sign-in). Web: `settings/user/platformThemes.ts`; resolution is own section theme, own theme for everything, platform default, shipped default (`applyPrefs`, `EntranceShell`, `UserSettings` preview). ADM-PLT-004 ticked.
  - `sr-server/src/integration_policy.rs`: at startup a saved policy overlays the config values (unfurl, gif region/locale, lookup providers); the config keys' doc comments say the config is the bootstrap value. ADM-PLT-005 ticked ("secrets are operator config by D-254"). The System page has a line pointing to Features.
  - Doc 12 section 2 step 4 and section 3 reworded, doc 05 rows added.

OpenAPI, client and MSW mocks regenerated; `route_matrix.tsv` has two rows (`GET /sr/appearance/defaults` public/none, `POST /submissions/{submission_id}/artwork/{kind}` authenticated/author), no `?` left. `openapi_names` KNOWN_CLASHES stays empty.

## Not done / open

- **Policy changes apply at the next start.** The unfurler, GIF client and lookup providers are built once at startup from the merged config. A hot reload would need those objects to be swappable; not done. The field help says so.
- **Library banner** does not exist in the model (no column), so `banner` is refused. **Cinema has no member or reviewer web editor** and no draft metadata routes besides the new artwork one, so K-06 in the web is Library only. "Pick frame from video" is deferred as the item says.
- **Crop** uses the existing canvas `ImageCropDialog`; there is no `sr-image-tools` WASM crate in the repo.
- K-07: the chapter object has no `source_key`, so the web cannot send it back; the server keeps it instead.
- `own choice` for the global theme is detected as "different from the default `lounge-dark`", because the prefs store has no "unset" state. A member who deliberately picked `lounge-dark` still gets platform defaults.
- Not touched on purpose: K-01/02/03/08/09/10/14 (other agent), K-13 (owner).

## Tests run

- `cargo fmt --all --check`; `cargo clippy -j 2 --all-targets -D warnings` on sr-features, sr-review, sr-library, sr-cinema, sr-api, sr-server, sr-activities, sr-db, sr-core.
- Scratch DB `sr_p15_k` (dropped at the end): `sr-activities --test db` (7), `sr-db --test review_repo` (12), `sr-db --test cinema_submissions` (5), `sr-server --lib staff_feed integration_policy` including ignored (13).
- `sr-api` all targets (601 lib plus integration tests, route matrix, openapi_names); `sr-features`, `sr-review` lib; `sr-cinema`, `sr-library`, `sr-core`.
- Web: `tsc`, eslint on touched dirs, prettier on touched files, vitest `--maxWorkers=2` (full suite result in the final report), `vite build` and the bundle budget (see final report).
