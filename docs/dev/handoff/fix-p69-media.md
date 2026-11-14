# Wave 69, media (SEC-AUD-225..226; D-709, D-710, D-719)

## Done (commits on `fix/p69-media`)

- **SEC-AUD-225** (`82549fc1`, plus one follow-up commit that fixes a compile error in my own migration test): language codes are stored in the short form.
- **SEC-AUD-226** (`31e2b822`): the title editor says where subtitle languages come from; the two placeholders are a key.

### Item 1

- **Fault confirmed by reading the base, not by running it.** `sr_cinema::rules::language("eng")` kept `eng` (it only lower-cased) and `sr-media`'s `language_tag(Some("fin"))` kept `fin` (outside its 18-name table). The two tests that keep this (`a_three_letter_language_code_is_stored_in_the_short_form`, `a_language_outside_the_name_table_is_tagged_in_the_short_form`) were NOT run against the base; they assert the short form, which the old code could not return.
- **Table:** `crates/sr-core/src/language.rs` (`LANGUAGE_PAIRS`, 204 pairs, 184 languages, sorted; `short_language(&str) -> &str`). The `sr-media` to `sr-core` edge (`default-features = false`) resolved offline; `Cargo.lock` gained one line. No second copy of the table.
- `sr-media/src/plan.rs`: `language_info` now returns names only (labels unchanged); `language_tag` shortens the primary subtag (`eng` to `en`, `por-br` to `pt-br`, `fil` stays). `lang_slug`, storage keys and playlist paths are unchanged (tested).
- `sr_cinema::rules::language` shortens the primary subtag after the shape check; `languages(...)` already compared after normalising (`["en","eng"]` gives `["en"]`, tested).
- **Every place that takes or derives a language:**
  - Title editor lists, trailer `language`, catalog filters `language` and `subtitle_language` (`rules::browse`): all reach the validator (so `eng` finds `en`; `the_catalog_filter_finds_the_title_by_the_short_code`).
  - Subtitle upload and edit (`routes/cinema_media/subtitles.rs`): call the validator; route test `a_three_letter_language_is_stored_and_reported_in_the_short_form` (stored `en`, audit `language_added` is `en`, `POR-br` is `pt-BR`, `fil` stays).
  - D-694 `title_language` (`media_repo/languages.rs`): calls the validator, unchanged; `a_finished_transcode_adds_the_short_form` proves an embedded `fin` / `ger` track gives `fi` / `de` (and `fil` stays) in the title and in the track tags.
  - Embedded audio and subtitle tracks: `plan.rs` `language_tag` (above).
  - Member submission of a title (`cinema/submit`): the draft carries the same `TitleInput`, validated by the same rules; nothing to do. Metadata lookup: no language field found. Sidecar file names (`naming.rs`): the tag is only parsed as written; it is not stored by another path.
  - `ProbeSummary.audio_languages` (`sr-media/src/service.rs`): raw probe codes on the admin probe summary. Left as probed (a record of the file); not stored.
  - Library `language` (`sr-library/src/rules.rs`, `0200_library_catalog.sql`): a separate list and column, does not share the Cinema columns. Left alone.
- **Migration `2970_language_codes_short_form.sql`:** a temp table with the pairs (a `VALUES` list), two temp functions, then it rewrites `video_titles.languages` and `subtitle_languages` (element by element, order kept by first place, duplicates removed, only rows holding a rewritable element), `video_audio_tracks.language_tag`, `video_subtitle_tracks.language_tag`, and also `video_trailers.language` (extra: the trailer editor accepted `eng` too). Left alone: the tracks' raw `language` column (the probed code), file names, storage keys, playlist paths. **`updated_at` does not move** (not a staff edit; tested). Idempotent (a second run changes no row, not even xmin; tested by running the file text twice). The search index holds no languages (only an empty list in a `sr-cinema/src/search.rs` test fixture). The temp objects are dropped at the end of the file.

### Item 2

- `TitleEditor.tsx`: the Subtitle languages field gets a hint through `ListInput`'s existing `hint` prop (the other list fields keep "Separate with commas"); it says: added automatically, also for cuts and files uploaded for the title, a track never removes one, hide the track to keep a language off. Wired by the primitive's `aria-describedby` (tested, and checked in the browser with `toHaveAccessibleDescription`). After a save the form takes the saved title, so `eng, fin` shows as `en, fi`.
- `SubtitlesPanel.tsx`: both placeholders are `admin.subtitles.languagePlaceholder`; a track row shows the language as the server saved it after a patch (`patch` returns the saved track).
- Fragment `en/p69-media-1.ts` (2 keys). `node scripts/i18n-audit.mjs` prints `TOTAL 0`.

## Not done

Nothing dropped. Not run: the base for the "confirm the fault" tests (see above).

## Routes shot

`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p69-media`: `editor-hint-*`, `editor-saved-*`, `panel-*`, `panel-saved-*`; desktop 1280x800 and phone 375x812; Editorial, Lounge Light, Lounge Dark. On the real stack the title editor field took `eng, fin`, saved, and showed `en, fi`; the subtitle panel (fake asset 777 through `page.route`) took `POR-br` and showed `pt-BR`. No sideways scroll at 375 px (asserted by the spec). I opened four of the 24 PNGs (desktop light hint, phone dark saved, phone editorial panel, desktop dark panel); the rest are covered by the spec's assertions only.

## Tests run

- `cargo clippy -j 4 -p sr-core -p sr-media -p sr-cinema -p sr-db -p sr-api --all-targets -- -D warnings`: clean.
- Pass: sr-core `language` (6), sr-cinema `language` (2), sr-media plan (24, including the new one), sr-api (`a_three_letter_language_is_stored...`, `subtitle_routes_add_their_language...`, `subtitle_changes_are_validated...`), sr-db `the_sql_pairs_and_the_rust_table_are_the_same_list`, and with `--ignored`: `a_finished_transcode_adds_the_embedded_languages`, `a_finished_transcode_adds_the_short_form`, `migration_2970_rewrites_three_letter_codes_and_only_those_and_is_idempotent`, `an_uploaded_track_adds_its_language_to_the_title`, `hiding_or_deleting_a_track_removes_nothing`, `facets_count_the_published_catalog`, `the_catalog_filter_finds_the_title_by_the_short_code`.
- Web: `p69media.test.tsx` (3) passes. Watched fail without the change: with `TitleEditor.tsx` stashed, the help-line test and the saved-list test failed (the panel test depends on `SubtitlesPanel.tsx`, which was not stashed). tsc, eslint and prettier clean.
- Playwright `p69-media-shots.spec.ts` (2): pass on my own stack with my own binaries (the migration ran in them).
- Not run: whole suites, route matrix, bundle and entrance checks (nothing first paint loads was touched). No route or response shape changed; `openapi.json` and the client are not regenerated.

## Docs updated

`docs/specs/cinema-media.md` (a new bullet on the short form, the D-694 bullet rewritten for D-709, D-710, D-719, the filter sentence), `docs/04-data-model.md` (the language columns). Search of `docs/`, `deploy/` and the README for "Subtitle languages" and `eng` examples found nothing else. `check-doc-links` ok.

## Proposed inventory rows

- SEC-AUD-225: Language codes are stored in the short form (D-719): `sr_core::language` table used by the Cinema validator and the embedded track tags; migration 2970 rewrites existing titles, tracks and trailers.
- SEC-AUD-226: The title editor says where subtitle languages come from (D-709, D-710) and shows the list as saved; the two placeholders are a catalog key (the i18n audit can go to `--max 0`).

## Threat-model rows

None.

## Sentences for ops

- `CHANGELOG.md`: "Languages are stored in one form: migration 2970 rewrites three-letter codes of titles, tracks and trailers to the two-letter form (`eng` becomes `en`); the catalog's language filters no longer split a language in two, and the title editor says that visible subtitle tracks add their language."
- `deploy/UPGRADE.md`: "Migration 2970 rewrites three-letter language codes of titles and tracks to the two-letter form; no action."

## Dead keys

None.

## Owner questions

None blocking. Defaults chosen: the migration also rewrites trailer languages; `updated_at` does not move; the admin probe summary keeps the probed code.

## Cleanup

Dropped `sr_p69_media` and `sr_p69_media_ui`; deleted `p69-media-target` and `p69-media-bin`. Untracked scratch remains in `p69-media/.scratch` (never committed) and `SIDEROOM_wt/p69-media-scratch`.
