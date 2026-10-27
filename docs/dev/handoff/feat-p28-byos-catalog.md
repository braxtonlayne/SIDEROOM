# Handoff: feat/p28-byos-catalog (BYOS-CAT-001, BYOS-CAT-002)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p28-byos-catalog`, base `integration/phase28`.
`CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p28-byos-catalog-target`, `CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`.
Scratch DB `sr_p28_byos_catalog` (exists; drop it at the end of the wave). Migration 1450 only. Job id 1450 / kind
`storage.index`. Nothing pushed, no server started, no download, no new dependency.

## Resume here

Everything below "Done" is committed. Next, in order:
1. Index-time metadata fetch from the lookup providers (not built; the editor's "Fetch details" is). Design: a
   `DetailsFetcher` trait next to `IndexStore`, an adapter over `sr_api::routes::review::LookupDeps`, run after
   `apply` for at most N new titles per run, exact title and year match only, writes `synopsis`, `genres`,
   `external_ids` and `attribution` unless the field is in `overridden`. Waits on owner question Q22 (TMDB terms).
2. Music screen (API is built: `GET /shelves/music`); there is no Music section in the web app until wave 30.
3. Playwright `byos-mount.spec.ts` step (a film and a CBZ from Garage): needs Garage, which only CI has, and
   8080/5173; not written.
4. The header sniff (`index::sniff`, tested) is not yet called by the walker: `web_native` is by extension today.
5. Integrator: add the CAT-001/002 lines to `docs/inventory/21-feature-phase.md` (the file has no such lines on this
   base), run `every_route_enforces_its_authz_row` group 5 (the six new routes carry `route_matrix` rows; the seeds
   are one block of `authz_matrix.rs`, plus the new test `shelves_hold_over_http` at the end of that file), web
   build and bundle budget. OpenAPI, api-client and MSW mocks are regenerated on this branch (1002 operations);
   regenerate again after merging.

## Done

BYOS-CAT-001 (indexing)
- Migration `1450_storage_objects.sql`: `storage_objects`, `storage_index_runs`, `music_tracks`, and on
  `video_titles`, `video_episodes`, `reading_series`, `reading_chapters`: `mount_object_id`, `shelf_key`,
  `overridden`, `attribution`, `shelf_state` (and `external_ids` on series and tracks).
- `crates/sr-storage/src/index/`: `classify.rs` (folders, names, `.sideroom.json`, covers by folder, loose chapter
  numbering), `sniff.rs` (container facts from the first 1 MiB), `shelf.rs` (types, `ShelfStore`, `reachable`,
  `drop_delisted`), `mod.rs` (`IndexStore`, `IndexControl`).
- `crates/sr-db/src/index_repo.rs`: `PgIndexStore` (one run per mount at a time, upserts, overrides honoured, vanished
  objects hidden, a partial listing hides nothing) and `IndexControl` (queue one job per mount).
- `crates/sr-server/src/storage_jobs.rs`: walker, `sr_media::naming` adapter, sidecar reader, the job (kind
  `storage.index`; singleton id 1450 daily 06:10 UTC; one-shot with `{"mount_id"}`), registered and scheduled in
  `app.rs`. The instance bucket (mount 1) is never indexed. Cap and budget: `ops.storage_measure_max_objects`, 60 s;
  `library.ingest.rar_enabled` gates CBR.
- Tests: sr-storage index (15), sr-db `index_repo` (4, Postgres), sr-server `storage_jobs` (5; a generated tree of 204
  names through the real naming rules, partial listing, sidecars), `storage_index_pg_tests` (10,000 objects in about
  7 s, a re-run adds nothing, one queued job per mount). `load-testing.md` has the row.
- Misfile rate on the fixture tree: 0 of 204. Real libraries will differ; the sidecar and the editor are the escape.

BYOS-CAT-002 (shelves and editor)
- Rows from a mount are written with `published = FALSE`, so no existing member query sees them (fail closed); the
  shelf queries read them by `source = 'mount'`, the mounts `scopes::access` admits for the section's kind, and
  `shelf_state = 'listed'`, then drop delisted keys.
- Routes (session-only; bots refused in `auth::policy`): `GET /shelves/{section}`,
  `GET|PATCH /shelves/{section}/{shelf_item_id}`, `PATCH /shelves/{section}/{shelf_item_id}/parts/{shelf_part_id}`,
  `GET|POST /storage/mounts/{mount_id}/index`. Code: `routes/storage/shelves.rs`, `sr-db/src/shelf_repo.rs`. The
  attribution text of fetched details is chosen by the server from the provider id; the client cannot write it.
- Tests: sr-db `shelf_repo` (2, Postgres: visibility per viewer and section, paging, filters, delisting, disabled mount,
  store failure, overrides survive a re-index), sr-api shelves unit tests (2), route_matrix (6),
  `authz_matrix shelves_hold_over_http` (Postgres, passes; `storage_scopes_hold_over_http` still passes).
- Web (`apps/web/src/storage/shelves/`, `library/reader/cbzRange.ts`): list with source chips and the "Shelves"
  filter, item page, web-native playback from a presigned URL with refresh, greyed "Convert to play" for the rest,
  CBZ read by range requests (`sr-library-wasm` `listPages`: one request for the directory, one per page), EPUB and
  PDF through the library's own readers from a presigned fetch, the metadata editor with "Fetch details" through
  `GET /submissions/lookup`, episode and chapter editing, Re-index with the last run. Routes `/cinema/shelves[/$id]`
  and `/library/shelves[/$id]`, nav entries. Vitest: 17 (MSW, axe) and 5 (range reader); lint, prettier, tsc,
  i18n-audit, check-css-tokens clean. Fragment `i18n/messages/en/p28-byos-catalog-1.ts`.
- Known small gaps: the EPUB/PDF readers' "back to series" link (error state only) points at `/library/s/{id}`, which
  does not exist for a shelf series; shelf books save no reading progress.

Wave-27 follow-up (per-row `mount_id`, per-mount media URLs)
- `mount_id` (default 1) on `TitleRecord`, `EpisodeRecord`, `SeriesRecord`, `ChapterRecord`, `Asset`,
  `AssetPlayback`, `PlaybackAsset`, read by the existing queries. Cinema and Library build media links with
  `media_url(mount_id, key)`: mount 1 keeps `/cdn/<key>`; any other mount gets `/cdn/mount/<id>/<key segments>`.
- The CDN answers that kind with a **302 to a presigned URL** of the mount's bucket (`MountStorages::for_mount`), TTL
  min(left of the signed link, 15 min), at least 60 s, `no-store`; no bytes pass through (the smallest correct form:
  a mount-scoped redirect, not a proxy). Disabled or unknown mount: 404; mount 1: refused; keys pass `safe_key`; a
  takedown blocking `mount/<id>/<key>` gives 451. `CdnDeps::with_mounts` wires it. Tests: 5 in `cdn/mount_tests.rs`,
  2 in `cdn/path.rs`, `cinema_repo a_row_on_another_mount_gets_mount_scoped_media_links`.
- Not done: the HLS playlist renderer and `PlaylistRenderer::storage_key` still read the instance bucket
  (`PlaybackAsset.mount_id` is carried; nothing writes renditions to another mount before wave 29's converter).

## Decisions

- Mount CBZ chapters use `format = 'images'` with no stored pages and `source_key` = the archive, so no enum or
  constraint changed; the shelf tells CBZ, EPUB and PDF apart by `storage_objects.container`.
- A Series-folder file without an episode number is left out; two files for one episode number keep the first; a
  chapter number already taken moves to the next free one (`docs/specs/ingest-naming-conventions.md` section 4).
- Job id 1450 (above 1024, like 1105 and 1150) to stay clear of the other agents' small ids.

## Open owner questions

- Q22 (provider terms): automatic fetching at index time waits for it; the editor's fetch is member-triggered and
  carries the provider's attribution on the row and on screen.
- Delisting (`ScopeFacts::delisted`) is byos-legal's stub; the shelves already call it.

## Tests run (all green)

`cargo test -p sr-storage`, `-p sr-library-wasm`, `-p sr-media -p sr-tv`, `-p sr-cinema -p sr-library`, `-p sr-api --lib`
(cdn, auth, shelves), `--test route_matrix`, `--test openapi_names`; Postgres (`--include-ignored --test-threads=1`):
sr-db `index_repo`, `shelf_repo`, `cinema_repo`, `library_repo`, `library_wiring`, `cinema_submissions`, `review_repo`,
`media_repo`; sr-server `storage_index_pg_tests`, `authz_matrix` (`shelves_hold_over_http`,
`storage_scopes_hold_over_http`), `storage_jobs`; `cargo fmt --check` and `clippy --all-targets -D warnings` on the
touched crates; web vitest (`src/storage/shelves`, `src/library`, `src/cinema`, `src/i18n`), tsc, eslint, prettier,
i18n-audit, check-css-tokens, `mocks:check`.
Not run: full workspace `cargo test`, the full authorization matrix, web build and bundle budget, Playwright.

## Integration notes from the commander's messages

- **Takedown and delisting (byos-legal, 5386073f).** Shelves decide through `scopes::reachable`, which calls
  `scopes::access` (refuses a `delisted` mount, an inactive owner, a disabled mount) and `ScopeFacts::delisted` for
  every page and for single files and parts, so the shelves honour delisting as soon as the merged `PgScopeFacts`
  implements `delisted` with `storage_legal::delisted_keys`. Erased mounts are soft-deleted, which `MountResolver`
  already hides. The indexer: `IndexStore::delisted(mount, keys)` leaves delisted keys without a row (and an object
  indexed earlier is hidden as vanished); `PgIndexStore::delisted` runs the same query as `storage_legal::delisted_keys`
  against `takedown_items` and answers "none" only when that table or column is absent (a schema without
  migration 1460). At integration, replace its body with `crate::storage_legal::delisted_keys(&self.pool, mount_id,
  keys)`. The daily job indexes only mounts whose status is `Active` or `Failing` (an allow-list, so `Delisted` and any
  later status are skipped).
- **Delete and move through the browser (byos-browser).** Chosen: immediate re-keying, with the next run as the
  safety net. `PgIndexStore::on_objects_deleted(mount_id, key_or_folder)` hides the object rows and their catalog
  rows (and series left empty); `on_objects_moved(mount_id, from, to)` re-keys `storage_objects`, a chapter's
  `source_key` and a film's `shelf_key` so edits travel with the file. Test: `index_repo
  deleting_and_moving_objects_updates_the_rows_at_once`. The integrator calls them from `DELETE
  /storage/mounts/{id}/objects/{key}` and `POST /storage/mounts/{id}/move` after the bucket call succeeded, then
  `IndexControl::request(mount_id)` after a move (a series keyed by a moved folder is found again by that run). If
  not wired, the next `storage.index` run reaches the same state without the preserved edits of a moved file.
- Job id 1450 does not clash with byos-browser's 138. I implement no `ScopeStore` methods of my own; `PgShelfStore`
  and `PgIndexStore` are separate types.
