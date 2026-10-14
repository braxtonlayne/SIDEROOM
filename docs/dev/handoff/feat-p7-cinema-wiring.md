# Handoff: feat/p7-cinema-wiring (phase 7, Cinema backends wired together)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p7-cinema-wiring, branch feat/p7-cinema-wiring (base integration/phase6).
Built with CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p4-passkeys-target and CARGO_PROFILE_DEV_DEBUG=0.
Scratch database `sr_p7_wiring` on the local Postgres 17. **No migrations were needed** (0440 to 0459 are unused).
Contracts: `docs/specs/cinema-media.md` section 9.10 (wiring and moderation) and section 10 (playback source contract,
committed first so the ingest-db agent could implement the Postgres source).

## Done

1. **Catalog adapter.** `crates/sr-server/src/cinema_catalog.rs`: `RepoCatalog` implements `sr_cinema_social::CatalogLookup`
   over `Arc<dyn CinemaRepository>` (public scope; restricted titles are returned, play stays gated by 900011, D-184).
   `app.rs` no longer uses `NoCatalog`. Two repository methods were added to the `CinemaRepository` port with default
   bodies (so stubs keep compiling): `titles_sharing_tags` (recommendations) and `asset_owner` (playback gate), both implemented
   in `PgCinemaRepo`.
2. **Providers and cascade.** `crates/sr-cinema-social/src/hooks.rs` (the social crate now depends on `sr-cinema`):
   `row_providers()` returns providers for `continue-watching`, `recently-explored`, `trending` (`?scope=`, default `known`),
   `because-you-watched`, `friends-watching`; `SocialObserver` implements `CatalogObserver` (merge re-points progress,
   ratings, list items, comments, reviews, presence; delete removes them). Registered in `app.rs` with `with_provider` and
   `with_observer`. `requests-fulfilled` has no provider (no requests pipeline exists).
3. **Playback.** `sr_media::playback` (new file in `sr-media`): the `PlaybackSource` port, `PlaybackAsset`, `SubtitleExtra`,
   `MemoryPlaybackSource`, `sample_asset`. `sr-api/src/routes/cinema/playback.rs`: `GET /cinema/assets/{id}/playback` and
   `POST /cinema/assets/{id}/refresh` (unix seconds; master URL, expiry, refresh_at, duration, subtitles with signed vtt/ass
   URLs, audio tracks, sprites URL and geometry, skip markers), gated by the new `CinemaService::authorize_asset` (owner
   published, restricted needs age confirmation, 403 code 900011). The playlists themselves are delivered by the CDN: new
   kind `hls` (`/cdn/hls/<asset>/...`, object prefix `video/<asset>/`), with a `PlaylistRenderer` hook in `CdnDeps`
   (`with_playlists`) that renders `master.m3u8`, rung and audio playlists, the subtitle wrapper playlists and the sprite
   map on request; other files are objects with HLS content types (`m3u8`, `m4s`, `vtt`, `ass`, fonts).
   `app.rs` plugs an **empty `MemoryPlaybackSource`**: the routes answer 404 for every asset until the Postgres source is
   swapped in (see "Merge step").
4. **Poster.** `TitleRef.poster_key` is replaced by a signed `poster_url` (the adapter signs it with the CDN signer, hour
   rounded like the catalog's artwork URLs). The My list page already read `poster_url`; only the web types and a test
   fixture changed. openapi.json and the generated client were regenerated.
5. **Jobs.** `crates/sr-server/src/cinema_jobs.rs`: `cinema.presence_sweep` every minute (singleton id 160), registered and
   scheduled in `app.rs`. The social crate named no other missing periodic job.
6. **Staff moderation.** `crates/sr-api/src/routes/cinema_social/moderation.rs` and `sr-cinema-social/src/moderation.rs`:
   `GET /admin/cinema/reports`, `POST /admin/cinema/reports/{id}/resolve`, `PUT /admin/cinema/reviews/{id}/hidden`,
   `PUT /admin/cinema/comments/{id}/hidden`. Staff session with `DELETE_CONTENT`; each change is in the access log
   (`cinema.review.hide|unhide`, `cinema.comment.hide|unhide`, `cinema.report.resolve`). Hiding closes the open reports about
   that content as `actioned`.
7. Inventory notes updated (CIN-ING-007, CIN-SOC-001); no new item was fully completed beyond what was ticked.

## Merge step (for the owner)

- Implement `sr_media::playback::PlaybackSource` over the ingest tables per spec section 10.2 (the table shape is documented
  there; `renditions` needs `frame_rate`, `channels`, `average_bandwidth_bps`, `meta jsonb` additions) and replace the
  `MemoryPlaybackSource::new()` in `crates/sr-server/src/app.rs` (block "Playback"). Promote transcode outputs to the object
  prefix `video/<asset_id>/` (the CDN kind `hls`).
- The `/cdn/hls` path is public-listener signed delivery like every CDN kind; nothing else is needed for the player's URLs.
- Job id 160 is taken by `cinema.presence_sweep`; the ingest worker should avoid it.

## Decisions (please confirm)

- Trait lives in `sr-media` (not `sr-api`) so Postgres code can implement it without depending on the HTTP crate.
- Moderation permission is `DELETE_CONTENT` (the platform permission that removes user content); no new permission bit.
- An anime without episodes is a movie to the social side (the catalog treats it as a film); with episodes it is `anime`.
- Merge conflicts of user state: newest row wins (progress, ratings, reviews); `completed` is sticky; the list keeps the target's place.
- `poster_key` was removed from `TitleRef` (a storage key is not displayable and need not leak).
- `trending` row defaults to the `known` scope when no `scope` is given.
- Playlist responses from the CDN are `private, max-age<=60`; segments keep the CDN's normal caching.

## Not done / gaps

- No Postgres `PlaybackSource` (by design; the ingest-db agent and the owner do it). The CDN hls path and the playback routes are
  tested against the in-memory source and an in-memory object store only, not against real ffmpeg output.
- "Requests fulfilled" provider; CIN-SOC-003 contribution credits; featured lists.
- Reports have no automatic notification of staff (the `ModerationHook::on_report` extension point is still the default no-op).
- The `RowProvider` for trending runs the full trending query per request; no caching.

## Tests

Gates run: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace`, `cargo deny check` (all green); web `typecheck`, `lint`, prettier and `vitest run src/cinema` (135 tests) green.
Counts: `sr-api` lib 350 passed (+1 ignored); `sr-media` playback 1; ignored Postgres suites: `sr-server` `cinema_wiring_pg_tests` 5, `sr-cinema-social --test db` 19 (+2: cascade, moderation queue), `sr-db --test cinema_repo` 12 (+1: asset authorization), `sr-api` moderation flow 1.
New non-database tests: playback and refresh (4: shape and signatures, gate and 404s, refresh, CDN rendering plus segment delivery), moderation access matrix and bad requests (3), openapi paths.
Run: `SR_TEST_DATABASE_URL=... cargo test -p sr-server cinema_wiring -- --ignored --test-threads=1`.
