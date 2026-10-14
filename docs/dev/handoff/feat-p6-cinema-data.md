# Handoff: feat/p6-cinema-data (Cinema catalog backend; gate green)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p6-cinema-data, branch feat/p6-cinema-data (base integration/phase5).
Built with CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p4-passkeys-target and CARGO_PROFILE_DEV_DEBUG=0.
Scratch DB `sr_p6_cinema` on the local Postgres 17 (dropped at the end of the run; the ignored tests clone a template per test through `sr_db::testing::TestDb`, so they need a role that may CREATEDB).
The route shapes are in `docs/specs/cinema-media.md` section 8 ("HTTP API as built"), committed before the code and kept in step with it.

## Done
- **Schema (CIN-DATA-001, catalog side), migration 0370.** `video_titles`, `video_seasons`, `video_episodes`, `video_title_variants`, `video_trailers`, `video_markers`, `video_collections`, `video_collection_items`. Free text people lists (`cast_list`, `directors`, `writers`, `studios`, `crew`, D-089), `alt_titles`, `languages`, `subtitle_languages`, `external_ids`, artwork keys, `age_restricted`, `merged_into` redirect, denormalised `season_count` / `episode_count` / `last_content_at`. `asset_id` columns (titles, episodes, variants, trailers) are plain nullable references with no foreign key. Doc 04 section 9 updated.
- **New crate `crates/sr-cinema`** (no HTTP, no SQL): `model`, `rules` (validation, 100% unit tested), `repo` (port `CinemaRepository`), `service` (`CinemaService`: member reads via `Viewer`, publisher writes via `Actor`), `search` (tantivy `TitleIndex`, prefix matching over titles, original/alt titles, cast, crew, genres; kind and age filters; hits are re-read through the public scope), `rows` (generated row definitions, `RowProvider`, `CatalogObserver`).
- **Postgres repo** `sr-db/src/cinema_repo.rs` (`PgCinemaRepo`): dynamic browse query with bound parameters only; keyset cursors for all six sorts (`updated`, `newest`, `title`, `year`, `runtime`, `curated`) in both directions; total is a separate count so it ignores the cursor; transactional merge.
- **HTTP** `sr-api/src/routes/cinema/`: 13 member routes under `/cinema/*` and 34 staff routes under `/admin/cinema/*` (staff session; permission checked in the service; every change goes through `AdminService::record_action` as `cinema.<thing>.<verb>`). New CDN kind `cinema` (`/cdn/cinema/...`, signed). Unique operationIds (`cinema_*`, `admin_cinema_*`).
- **Wiring** (`sr-server/src/app.rs`): service over Postgres, index in `<search dir>/cinema` rebuilt at boot with `reindex_all` (CIN-DATA-004), `PgAgeGate`, new `PgCinemaPrefs` reading `cinema.hide_age_restricted` from the user settings blob (default false), storage for artwork upload reservations.
- openapi.json and api-client `schema.d.ts` regenerated (pure insertions; the generator ran from the main clone's node_modules because the worktree has none).
- Inventory CIN-DATA-001..006 ticked, with notes on what belongs to the other agents.

### What each CIN-DATA item delivered
- 002: title/season/episode CRUD, publish/unpublish (publish needs `PUBLISH_MEDIA` and something playable; unpublish takes either `PUBLISH_MEDIA` or `DELETE_CONTENT`), delete (`DELETE_CONTENT`), merge (both bits), variants, trailers, skip markers, asset link (`PUT .../asset`), artwork upload reservation (presigned PUT, `POST .../artwork`).
- 003: discover (hero + rows), `/cinema/titles` browse (kind, genres, exclude_genres, tags, year range, rating csv, runtime range, audio and subtitle language, age, collection, sort, order, limit, cursor), facets, title detail, episodes (keyset), episode, markers, play targets, search.
- 005: curated rows = collections (row / hero / banner, placed on `surfaces`), generated rows `new-this-week`, `recently-added`, `anime-room`, `one-more-episode`, `films`, `animation-banner`, `genre:<name>`, `collection:<id>`; provider rows through `RowProvider`.
- 006: `age=hide|include|only` (default include; `hide` when `cinema.hide_age_restricted`), restricted titles visible to everyone, opening them (`/play`, trailer assets) needs age confirmation (403, code 900011).

## Behaviour decisions (please review)
1. **Three kinds**, `movie` / `series` / `anime`, instead of doc 04's four. An anime title is episodic when it has seasons, otherwise a film (documented in doc 04 and the spec).
2. **Table names carry the `video_` prefix** (`video_seasons`, `video_episodes`, ...) rather than doc 04's bare `seasons` / `episodes` / `trailers`, to avoid generic names clashing later. **The ingest agent must reference `video_episodes` / `video_titles` if it joins them.**
3. **Publishing needs something playable**: a title needs its own `asset_id` or a published episode; an episode needs an `asset_id`. A draft can exist without an asset. Removing the last playable thing of a published title (unlinking the asset, unpublishing or deleting the last published episode, deleting its season) **unpublishes the title** automatically (`CinemaService::settle`) instead of returning a 409.
4. **The asset link is not part of the metadata input**; it has its own `PUT .../asset` route and `CinemaService::link_title_asset` / `link_episode_asset` (needs `PUBLISH_MEDIA`). Editing metadata never unlinks an asset. The ingest agent calls these (or sets the column) when an asset becomes ready. Variants and trailers carry `asset_id` in their input.
5. **`/play` is the opening gate.** `CinemaService::resolve_play(viewer, PlayRef)` returns the asset to play, markers and next/previous episode, and answers `AgeConfirmationRequired` for a restricted title. A title with episodes and no asset of its own plays its first playable episode. The ingest/playback manifest route and the download routes should call `resolve_play` (or at least check the same rule) before handing out media. Trailer `asset_id` is withheld for a restricted title until the age is confirmed.
6. **Merge**: the target wins a clash and fills its gaps from the source (asset, still, synopsis, air date, artwork, language lists; the source title is added to `alt_titles`). Clashing seasons merge by number, clashing episodes by number inside the season; the source's duplicate episode is deleted and reported to `CatalogObserver::title_merged` as `(dropped id, surviving id)`. Variants move (a clashing label gets ` (id)`), trailers and collection places move, markers move unless the target already has that kind. The source stays as a redirect (`merged_into`), public reads of its id answer with the target; its own `asset_id` stays on the redirect row.
7. **Markers**: one window per kind (intro, credits, recap) per title or episode, replaced as a set. Not per variant.
8. **Row items are flat**: `RowItem` is a `Title` plus `episode_id`, `progress_percent`, `reason` (`serde(flatten)`).
9. **Genres and tags are lower-cased** on write and in filters, like the Library. Content ratings are upper-cased.
10. Browse does no free text search; use `/cinema/search` (index) for text. `q` is not a browse parameter.

## Not done / notes
- **Storage cleanup**: artwork keys replaced or orphaned by deleting a title are not collected (no GC job like the Library's `reading_gc_keys`). Reserved but never used artwork uploads also stay in the bucket.
- The artwork presigned PUT is tested against the in-memory signer only (no live S3).
- `Title.has_trailer`/`playable` semantics: `playable` is `asset_id IS NOT NULL OR published episodes > 0`; it says nothing about whether the asset's transcode is ready (that is the ingest side; wire it in when assets exist, e.g. by also requiring a ready asset before `publish`).
- No per-user data anywhere (progress, lists, ratings, trending): the rows are hooks only.
- Sort by rating/popularity is not offered (needs the social tables).
- Spec says Movies/Series/Anime pages filter by "subtitles": implemented as `subtitle_language` over the catalog text list (`subtitle_languages`), not derived from real tracks.
- No frontend.

## For the other agents
- **Web (browse, player)**: contract is spec section 8. The generated TS types are in `packages/api-client/src/generated/schema.d.ts` (`Title`, `TitlePage`, `Row`, `RowItem`, `Discover`, `TitleDetail`, `EpisodeList`, `PlayTarget`, `Facets`, `InstantSearch`, ...). Errors: age gate is 403 code 900011 on `/cinema/titles/{id}/play`, `/cinema/episodes/{id}/play`; everything unknown or unpublished is 404. Cursor errors are 400. Settings key to add in the settings UI: `{"cinema": {"hide_age_restricted": bool}}`.
- **Social (p6-cinema-social)**: implement `sr_cinema::RowProvider` for `continue-watching`, `recently-explored`, `trending` (reads `scope` from `RowContext::params`: `friends|known|global`), `because-you-watched`, `friends-watching`, `requests-fulfilled`, and register them on the service in `sr-server/src/app.rs` with `cinema_service.with_provider(..)` **before** it is moved into `CinemaDeps`. Implement `CatalogObserver` to re-point `title_id` / `episode_id` rows after a merge and delete them when titles or episodes are deleted; register with `with_observer`. Use `CinemaService::resolve_play` to resolve an episode for "resume".
- **Ingest (p6-cinema-ingest)**: link a finished asset with `link_title_asset` / `link_episode_asset` (staff actor) or the admin routes; gate manifest and download routes with `resolve_play` (age rule) and visibility (only published items); migrations 0370 is mine, yours must not reuse it; `video_*` table names are mine.

## Tests
- sr-cinema: 20 unit tests (rules, cursors, search index, generated rows).
- sr-api: 12 cinema router tests (auth matrices of all member and staff routes incl. 401/403/50013, curator vs owner permissions, access log rows, age filter defaults and `cinema.hide_age_restricted`, 900011 gate, parameter errors, row routes and the provider contract, artwork reservation, openapi paths and unique operation ids). Total sr-api lib: 339.
- Postgres-backed (ignored): `cargo test -p sr-db --test cinema_repo -- --ignored --test-threads=1`, 11 tests: permissions and the publish rule, filters/sorts/keyset paging for every sort in both directions, age rules (hide/only/gate/trailers/search/facets/rows), seasons/episodes/play/neighbours and the playable invariant, variants/trailers/markers, merge of duplicates (clashing seasons, episodes, variants, markers, collection places, observer map, redirect), collections/rows/discover (hero fallback), provider rows cannot leak drafts, search rebuilt from the database and kept in step, facets, signed artwork.
- Gates: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo deny check` all green.

## Changes to the route shapes after the first commit (aea1aa1)
- Added `POST /admin/cinema/titles/{id}/artwork` (`ArtworkRequest` -> `ArtworkUpload`).
- Added `GET /admin/cinema/episodes/{id}/markers` (staff read of an episode's windows).
- `discover` also places `requests-fulfilled` (after `friends-watching`).
- Everything else is as first documented.
