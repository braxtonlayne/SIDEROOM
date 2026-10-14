# Handoff: feat/p8-livetv-be (Live TV schedule engine and virtual-linear HLS, backend)

Worktree `SIDEROOM_wt/p8-livetv-be`, branch `feat/p8-livetv-be` (base `integration/phase7`). Backend only. Local commits, nothing pushed. The HTTP contract is `docs/specs/cinema-live-tv.md` section 8 ("HTTP API as built"), written first and kept in step with the code.

**Nothing here has played a real stream.** There is no ffmpeg, no real transcode output and no HLS player in this environment. The live playlists are checked against fixture assets (`sr_media::playback::sample_asset` widened to 300 segments) by string and structure assertions only. Treat Safari, hls.js behaviour at programme boundaries and real segment timing as unverified.

## What exists

| Piece | Where |
|---|---|
| New crate `sr-tv` | `crates/sr-tv` (`tz`, `model`, `catalog`, `engine`, `schedule`, `live`, `presence`, `repo`, `pg`, `service`) |
| Migrations | `crates/sr-db/migrations/0460_tv_channels.sql`, `0461_tv_blocks.sql` |
| Routes | `crates/sr-api/src/routes/tv/` (`mod.rs`, `member.rs`, `admin.rs`, `tests.rs`), merged in `routes/mod.rs` (router and openapi) |
| Catalog adapter | `crates/sr-server/src/tv_catalog.rs` (`RepoTvCatalog` over `CinemaRepository`), wired in `app.rs` (one block, one `.extension(tv_deps)`, `cinema_repo.clone()`) |
| Contract | `packages/api-client/openapi.json` and `src/generated/schema.d.ts` regenerated |

### Engine (TV-ENG-002, 004, 005, 006, 008)
`engine::generate(channel, pools, from, to, cursor) -> blocks + cursor` is pure and deterministic. Channel rules: a grid (programmes start on 0, 5, ..., 60 minute lines in channel time), a no-repeat window, and dayparts (name, `HH:MM` start and end that may wrap midnight, weekdays, `sequential` or `shuffle`, `overrun` or `fit`, and a pool filter on kinds, genres, tags, title ids, years, collection, runtime and the restricted rule). A series marathon is a sequential daypart whose pool names one title. Gaps after a programme and between dayparts are filled by the filler policy: `bumpers` (titles tagged `filler`, 10 minutes or less, clipped to the grid line; longer gaps get a slate), `next_program_early`, `slate`, `off_air`. The cursor (`at_ms`, per-daypart sequential positions, recent keys, RNG state, bumper index) is stored on each generated block, so generating A to B and then B to C equals generating A to C (tested). Times of day use `tz::Zone`: a built-in table of 40 IANA zones with US, EU and Australian DST rules (no tz database exists in the workspace and none was downloaded). Tests cover DST days of 23 and 25 hours staying on the local grid, a prime-time daypart moving between winter and summer in UTC, wrapped dayparts with day filters, fit, empty pools, overlap-free contiguous output and seed sensitivity.

### Schedule storage and the rolling schedule
Blocks of a channel never overlap. Rule-based channels are **extended lazily**: any member read (`channels`, `guide`, `now`, `tune`, live playlists) makes `TvService::ensure` generate from the end of the last block (cursor from the last generated block) to now + 24 h, under a service-wide lock, with `ON CONFLICT DO NOTHING` so concurrent servers cannot double-write. There is no background job (kept `app.rs` edits to a minimum; add a `tv.extend` job later if cold channels matter). Staff `publish` regenerates every **generated** block that has not started (from the end of the programme on air), keeps manual and override blocks and fits generated blocks around them; `preview` does the same without storing. `schedule::place` trims, splits or removes what a staff-placed block overlaps (also used by "on now").

### Live HLS (TV-HLS-001, 002, 004)
`live::render_media` cuts the last 6 segments whose wall start is not after now (wall start = block start - block offset + position in asset), so joining 17 minutes in starts 17 minutes in and everyone gets the same window inside one segment. Sequence numbers must survive reloads, so each block stores `seq_base` and `disc_base`, assigned lazily on first view: chained from a contiguous predecessor (`seq_base + segments on air`, `disc_base + 1`) else seeded from the wall-clock seconds (monotonic because segments last at least a second). Boundaries carry `#EXT-X-DISCONTINUITY`, a fresh `#EXT-X-MAP` and `#EXT-X-PROGRAM-DATE-TIME`; segment URLs are signed `/cdn/hls/<asset>/...` (existing hls kind, no new CDN kind). The master reuses `sr_media::playlist::render_master` with relative URIs `v/{height}.m3u8` and `audio.m3u8`, offering only heights every programme of the next 3 hours has. Playlist endpoints are session-authenticated (cookies), not signed URLs. `Presence` (45 s, in memory) counts distinct members who fetched a media playlist; it is a trait so watch parties can share it.

### Staff (TV-ADM backend)
Channels CRUD, `schedule`, `preview`, `publish`, `blocks` (place), `DELETE /admin/tv/blocks/{id}`, `PUT now` (on-now control), `GET now?at_ms=` (simulate now), `report` (what aired, clipped to the window). Permission `MANAGE_TV` (Media Curator has it); all changes call `AdminService::record_action` (`tv.channel.create|update|delete`, `tv.schedule.publish`, `tv.block.place|remove`, `tv.now.set`).

### Age rule (D-184)
Restricted titles are scheduled where a pool allows them (default: excluded on normal channels, included on `age_restricted` channels) and listed for everyone; `tune` and every live playlist answer 403 code 900011 when a block in the window is restricted and `accounts.age_confirmed_at` is unset (same `AgeGate` as Cinema).

## Tests
* `cargo test -p sr-tv --lib`: 41 tests (tz 4, model 3, catalog 1, engine 12, schedule 2, live 8, presence 1, service 10).
* `crates/sr-api` `routes::tv`: 6 tests (auth and permission matrices for every route, shapes, live playlists with viewers, 900011, staff flows with access-log rows, OpenAPI paths).
* Ignored, need `SR_TEST_DATABASE_URL`, run with `--ignored --test-threads=1` (all passed here): `crates/sr-tv/tests/pg.rs` (3: channels, block apply/query/bases, service over Postgres) and `crates/sr-server/src/tv_wiring_pg_tests.rs` (2: catalog adapter over `PgCinemaRepo`, a channel scheduled from the real catalog).
* Whole workspace: 1474 passed, 0 failed, 174 ignored. `cargo fmt --check` and `clippy --workspace --all-targets -D warnings` clean. `cargo deny check`: bans, licenses and sources ok; **advisories fail on rustls RUSTSEC-2026-0346 to 0349, which exist on the base too** (no dependency was added, only the workspace crate `sr-tv`).

## Decisions
* Time zones: a built-in table instead of a tz crate (nothing to download). Historical rule changes are not modelled; unknown names are 400.
* Schedule state is a cursor carried by the generated blocks (rolling schedule) rather than a pure function of the date, so marathons continue exactly and shuffles keep their window.
* Durations come from the catalog's `runtime_min`, not from the asset. A block longer than its asset simply has no segments past the asset end (the stream stalls until the next programme); a shorter one is cut. Reconciling the two (e.g. reading `video_assets.duration_ms` when building pools) is the first thing to do before real use.
* Route shapes differ from the spec text in section 5 (`/tv/{channel}/live/{height}.m3u8`); section 8 is authoritative. `master_url` carries the `/api/v1` prefix.
* Audio is one group with the default track of each programme; subtitles are not offered live. Audio and video of an asset are assumed to have the same number of segments (the numbering chain counts the lowest video rung).
* Slates and colour bars have no media (TV-HLS-003 needs pre-encoded loops): a slate or off-air block makes the playlists answer 404 "Channel is off air" and `tune` return `state: off_air`; clients draw their own slate.
* Statistics are not persisted (D-101): the report shows what the schedule aired, not who watched.

## Not done
* `imported_guide` channels, the guide import (CSV/JSON, fuzzy matching, mapping report), `tv_schedule_slots`/`tv_guide_imports` (TV-ENG-001 partial, TV-ENG-003, TV-ADM-004).
* Favorites and reminders tables and routes (TV-UI-007, TV-PLY-005 backend half).
* Logo upload reservation route (the `logo_key` field and signed `logo_url` exist; keys must start with `tv/`).
* Cache invalidation on library changes (TV-ENG-007 partial): blocks keep their snapshot; an asset that disappears makes the playlists answer off air for that block.
* Slate and bars media (TV-HLS-003), two-client sync run and Safari discontinuity check (TV-HLS-005): only unit-level playlist assertions.
* A background job to extend cold channels, and a metrics endpoint for viewers (the count is on the channel list and `now`).
* No web UI (the web agent codes against section 8 of the spec).

## Notes for reviewers
* `app.rs`: three small edits (`cinema_repo.clone()`, the `tv_deps` block, `.extension(tv_deps)`); `routes/mod.rs`: three lines; `lib.rs` of `sr-server`: two module lines.
* Expect merge noise in `packages/api-client/openapi.json` (regenerate after merging) and `Cargo.lock`.
