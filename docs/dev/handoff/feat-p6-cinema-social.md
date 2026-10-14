# Handoff: feat/p6-cinema-social (phase 6, Cinema social and progress backend)

Branch `feat/p6-cinema-social`, based on `integration/phase5`. Items: CIN-PLY-007, CIN-PLY-009, CIN-MY-002, CIN-SOC-001, CIN-SOC-002
(ticked). CIN-SOC-003 (contribution credits) is a documented gap. The contract for the web agents is
`docs/specs/cinema-media.md` section 8 (committed first); the OpenAPI document and the generated client are regenerated.

## What was built

- **Crate `crates/sr-cinema-social`** (depends on `sr-core`, `sr-db`, `sr-events`, `sqlx`; no catalog crate). Postgres
  implementation lives in the crate (`CinemaService` over a `PgPool`), split by topic: `progress`, `presence`, `lists`,
  `social` (ratings, reviews, comments, reports), `discover` (trending, recommendations). `rules` holds the pure logic.
- **Migrations 0400 to 0403** (`crates/sr-db/migrations`): `cinema_progress`, `cinema_user_state`, `cinema_watching` and the SQL
  function `cinema_share_level()` (0400); `cinema_lists`, `cinema_list_items` (0401); `cinema_ratings`, `cinema_reviews`,
  `cinema_review_votes` (0402); `cinema_comments`, `cinema_comment_likes`, `cinema_reports` (0403). `title_id` / `episode_id` have
  **no foreign keys**; `episode_id = 0` is a movie.
- **Routes** `crates/sr-api/src/routes/cinema_social/` (51 operations, all session-user only), merged additively.
- **`CatalogLookup` port** (`catalog.rs`) with `MemoryCatalog` (tests) and `NoCatalog` (empty; what `sr-server` wires today).
- **Gateway events** `SR_CINEMA_UPDATE` (own sessions) and `SR_ACTIVITY_UPDATE` (watching presence to friends), both
  `DomainEvent::UserScoped` through `BusEvents`; documented in doc 05 section 6.3.

## Integration steps for whoever merges with p6-cinema-data

1. Implement `sr_cinema_social::CatalogLookup` in `sr-cinema` and replace `Arc::new(sr_cinema_social::NoCatalog)` in
   `crates/sr-server/src/app.rs` (block "Cinema social and progress"). Until then every progress/rating/comment call answers 404.
2. Conflicts are expected only in `routes/mod.rs` (3 lines: `pub mod cinema_social;`, one `.merge(...)`, one `doc.merge(...)`),
   `sr-api/Cargo.toml` / `sr-server/Cargo.toml` (one dependency line each, plus a `sqlx` dev-dependency in sr-api), and `app.rs`
   (one block before the review pipeline and one `.extension(cinema_social_deps)` line). Axum accepts different path parameter
   names at the same position (checked), so `/cinema/titles/{id}` and `/cinema/titles/{title_id}/progress` can coexist.
3. Regenerate `openapi.json` and the client after merging (my regeneration is purely additive: 32 paths, 47 schemas). The client was
   generated with the `openapi-typescript` binary of the main clone because this worktree has no `node_modules` (no install was run).
4. All OpenAPI schema names of this crate are `Cinema`-prefixed through `#[schema(as = ...)]` (the Library already owns `Progress`,
   `HistoryPage`, ...; utoipa's merge would otherwise overwrite silently). Pick other names in the catalog crate or keep them distinct.

## Decisions (please confirm)

- **Movies use `episode_id = 0`**; routes: `/cinema/titles/{id}/progress` (movie) and `/cinema/titles/{id}/episodes/{eid}/progress`.
  A series without an episode id is a 400; a movie with an episode is a 404.
- **Completion:** `percent >= 90` or explicit `completed`; sticky across a rewatch until `completed:false` or delete.
- **Conflict handling** is the Library's: `base_updated_at_ms` guard in the upsert `WHERE`, `applied:false`, `force`.
- **History pause** accepts heartbeats without storing them (`recorded:false`) and ends presence. Clearing history also deletes progress.
- **`cinema.share_watching`** (`off` / `friends` / `everyone`, default `friends`) is read in SQL from the user-settings blob by
  `cinema_share_level(user_id)`; no new settings endpoint (clients patch `settings.cinema.share_watching` like `library.hide_age_restricted`).
  `everyone` = friends plus people sharing a server (D-120), capped at 1000 recipients per change, minus people who blocked or ignored the watcher.
- **Presence TTL 45 s**, refreshed by heartbeats; events are sent when the item changes or the record lapsed, not on every heartbeat.
  Rows expire lazily; `CinemaService::sweep_presence` exists for a periodic job but no job is registered.
- **Age-restricted titles:** progress/rating/comment calls need age confirmation (900011) like the Library; presence goes to friends
  only and is masked in the event (no title, no ids); `friends-watching` shows the real title only to confirmed adults; restricted titles
  never appear in `known`/`global` trending or recommendations, and appear in `friends` trending only for confirmed adults.
- **Trending:** score = viewers (7 days) + 2 x completions + ratings of 7 or more; global needs 2 viewers and ignores people with `off`;
  `known` counts friends (not `off`) and server mates who chose `everyone`.
- **Ratings** are 1 to 10 only (no thumbs). **Reviews** one per account and title, edit = replace, rating link, helpful votes (not on
  one's own). **Comments** one reply level, likes, blocked/ignored authors filtered; deleting a parent cascades.
- **Lists:** `visibility` private/friends/public; the saved list is a lazily created `kind = 'saved'` list; the Library has no lists
  (LIB-SOC-006 is open), so nothing was generalised and the duplication does not exist yet.
- **Rate limits:** the default bucket (40 per 10 s per route, 50 per second global) is far above the 10 s heartbeat; no override added
  to keep `rate_limit.rs` untouched.

## Gaps / follow-ups

- CIN-SOC-003 contribution credits (needs the cinema submissions pipeline).
- No staff HTTP surface for hiding/unhiding reviews and comments or reading reports (`CinemaService::set_hidden` and the
  `ModerationHook` exist); no featured lists.
- The gateway does not yet fold the watching activity into `PRESENCE_UPDATE.activities` (separate `SR_ACTIVITY_UPDATE`).
- `NoCatalog` wiring (see above); no presence sweep job; a user's own account deletion relies on `ON DELETE CASCADE` (docs/09).
- Comment/review text is plain text; markdown rendering and mention parsing are not applied.

## Tests

- `cargo test -p sr-cinema-social`: 10 unit tests (progress validation, 90% and stickiness, text and cursors, share setting,
  restricted masking, ranking).
- `cargo test -p sr-api cinema_social`: 4 endpoint tests without a database (401 matrix over 40 routes, 404s, age gate 900011, 400s).
- `SR_TEST_DATABASE_URL=... cargo test -p sr-cinema-social --test db -- --ignored --test-threads=1`: 17 Postgres tests (conflict
  and 90% rule, catalog checks, history paging/pause/clear, continue watching and up-next, watched marks, presence privacy
  (friends/everyone/off, blocked, restricted, lapse and sweep), lists and visibility, ratings per scope, reviews, comments and
  reports, trending scopes and privacy, recommendations).
- Workspace gates run: `cargo fmt --all --check`, `clippy --workspace --all-targets -D warnings`, `cargo test --workspace`,
  `cargo deny check`; `sr-db` `library_wiring` ignored suite still passes (11).
