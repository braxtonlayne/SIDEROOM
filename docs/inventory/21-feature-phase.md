# 21 — Feature phase (`BYOS`, and the later prefixes of roadmap-p27)

Waves 27 to 40. Plan, acceptance criteria, agent split and owner needs: [dev/roadmap-p27.md](../dev/roadmap-p27.md) (D-380). Prefixes are those of roadmap-p27 section 4; each wave's integrator adds its items here.

Same conventions as [20-launch-readiness.md](20-launch-readiness.md): an item ticks only when its proof runs (a CI job, a Postgres or browser test, or a recorded run in the integration handoff), never when it is only written. The live Garage cases of `storage.yml` run only on CI; an item ticked on its mock and Postgres proofs says so, and its CI run is linked once the owner pushes.

## Wave 27 — BYOS core

Integration: [dev/handoff/integration-phase27.md](../dev/handoff/integration-phase27.md).

### BYOS-MNT — Mounts
- [x] **BYOS-MNT-001** [DB][BE] Mounts and sealed credentials: migration 1400 `storage_mounts`, `sr_storage::mounts` (`MountRepo`, `MountResolver`, sealed credentials), `CachedMountResolver`, user and guild mount CRUD routes, `sr-cli` rotate for mounts, `sr_storage::path::safe_key` _Proven 2026-10-26 on the merged tree (integration-phase27.md): `cargo test -p sr-storage`, `sr-db --test mounts_repo` (Postgres), `sr-api --lib` (mount route tests), `route_matrix`._
- [x] **BYOS-MNT-002** [BE] Connection test and provider guidance: `POST .../mounts/{mount_id}/test` (pass/warn/fail rows, 20 s budget), `abuse.storage_tests_per_hour` _Proven 2026-10-26 on the merged tree (integration-phase27.md): `sr-storage --test s3_probe_mock`, `sr-api --lib` (rate limit and route tests). The Garage live case runs on CI only._
- [x] **BYOS-MNT-003** [BE][CLI] CORS checker and apply: `.../cors/check`, `.../cors/apply`, daily job `storage.cors_recheck` (id 134), `sr-cli storage ensure-cors` _Proven 2026-10-26 on the merged tree (integration-phase27.md): sr-storage CORS checker tests (mock S3), `sr-server --lib admin_jobs` (job id, kind and key unique), `cargo test -p sr-cli`. The Garage live case runs on CI only._

### BYOS-SCP — Scopes
- [x] **BYOS-SCP-001** [DB][BE] Scopes, shares and grants: migration 1410, `sr_storage::scopes::resolve`, Friend Shelf shares, guild storage grants, unfriend hook _Proven 2026-10-26 on the merged tree (integration-phase27.md): `sr-db --test scopes_repo --test social_repo` (Postgres), `authz_matrix storage_scopes_hold_over_http`._
- [x] **BYOS-SCP-002** [BE] Scoped listing and presign: objects listing, metadata, presign (TTL settings `byos.presign_*`), authz matrix storage actors _Proven 2026-10-27 on the merged tree (integration-phase27.md): `authz_matrix` `every_route_enforces_its_authz_row` (every group, group rest at 16 weak refusals and 45 malformed, no ceiling raised) and `storage_scopes_hold_over_http`, the browse tests in sr-storage._

### BYOS-INS — The instance bucket
- [x] **BYOS-INS-001** [DB][BE] The instance bucket as mount 1: migration 1420 (`mount_id`, `source`), `MountStorages`, mount 1 seeded at start, a changed bucket refused _Proven 2026-10-27 on the merged tree (integration-phase27.md): the full workspace `cargo test --workspace --exclude sr-storage -- --include-ignored --test-threads=1` on Postgres (3489 passed, 0 failed) with `instance_mount` and its Postgres test; the existing suites pass unchanged._

### BYOS-UI — Settings
- [x] **BYOS-UI-001** [FE] User Settings > Storage and Guild Settings > Storage, Friend Shelf toggles, role editor storage grants, bucket browser read side, nav modes, `check-css-tokens` _Proven 2026-10-26 on the merged tree (integration-phase27.md): vitest on src/settings, src/storage, src/lounge (146 files, 1162 tests), typecheck, eslint, prettier, i18n-audit 0, check-css-tokens, build and bundle budget._

## Wave 28 — BYOS write side, catalog, legal, flags and the harness

Integration: [dev/handoff/integration-phase28.md](../dev/handoff/integration-phase28.md). The Postgres password file was
missing for the whole integration, so no Postgres suite ran on the merged tree locally. PR #57's CI ran them (all 27
checks green), and the wave-29 integration ticked the eight rows on that run.

### BYOS-BRW — The bucket browser, write side
- [x] **BYOS-BRW-001** [BE][FE] Multipart upload and folders through the ledger: migration 1440 and 1441, `sr_storage::uploads`, upload, folder, move and delete routes, the `storage.uploads_sweep` job (id 138), the mount upload kind of the IndexedDB ledger, the upload panel _Proven by PR #57's green CI (merged 2026-10-27; all 27 checks green, which ran every Postgres suite, the authz matrix and `immutable_rules`) on top of the merged-tree proofs of integration-phase28.md._

### BYOS-CAT — Catalog and shelves
- [x] **BYOS-CAT-001** [BE][JOB] Catalog indexing: migration 1450, `sr_storage::index`, `PgIndexStore`, the `storage.index` job (id 1450), delisted keys from the takedown register, the browser's delete and move and an upload's complete update the index _Proven by PR #57's green CI (merged 2026-10-27; all 27 checks green, which ran every Postgres suite, the authz matrix and `immutable_rules`) on top of the merged-tree proofs of integration-phase28.md._
- [x] **BYOS-CAT-002** [FE] Shelves in Cinema and Library and the metadata editor: `GET|PATCH /shelves/...`, source chips, playback from a presigned URL, CBZ by range, `?guild=` with `member_shelves_in_guild` _Proven by PR #57's green CI (merged 2026-10-27; all 27 checks green, which ran every Postgres suite, the authz matrix and `immutable_rules`) on top of the merged-tree proofs of integration-phase28.md. Still open as a follow-up, not a proof: the `byos-mount.spec.ts` step was never written._

### BYOS-LEG — Takedown and delisting
- [x] **BYOS-LEG-001** [BE][FE][DOC] Takedown and delisting for members' buckets: item kind `storage_object` (migration 1460), whole-mount delist and relist, DSR export and erasure, Admin > Storage, lawyer brief Q20-Q23, threat-model rows T30 and T31; P28-LEG-1 (a server bucket strikes the uploader the ledger knows, else the owner) _Proven by PR #57's green CI (merged 2026-10-27; all 27 checks green, which ran every Postgres suite, the authz matrix and `immutable_rules`) on top of the merged-tree proofs of integration-phase28.md, including P28-LEG-1's `storage_takedown` test._

### BYOS-FLG — Settings and the feature
- [x] **BYOS-FLG-001** [BE][FE] `platform.byos`, `guild_mounts`, `member_shelves_in_guild`, the `byos.*` settings (one copy of each key) _Proven by PR #57's green CI (merged 2026-10-27; all 27 checks green, which ran every Postgres suite, the authz matrix and `immutable_rules`) on top of the merged-tree proofs of integration-phase28.md. Not built (follow-up): the endpoint test button for staff and an HTTP test that the kill switch keeps mount 1 reads._

### OPS — Operations
- [x] **OPS-IDX-001** [OPS][BE][FE][TEST] Only the landing page is indexable (D-384) _Proven by PR #57's green CI (merged 2026-10-27; all 27 checks green, which ran every Postgres suite, the authz matrix and `immutable_rules`) on top of the merged-tree proofs of integration-phase28.md._
- [x] **OPS-IMM-001** [BE][FE][TEST] E2EE is the only voice mode _Proven by PR #57's green CI (merged 2026-10-27; all 27 checks green, which ran every Postgres suite, the authz matrix and `immutable_rules`) on top of the merged-tree proofs of integration-phase28.md, including `voice_pg_tests` and the three-engine `voice-e2ee.spec.ts` of `e2e.yml`._
- [x] **OPS-IMM-002** [TEST] The IMMUTABLE RULE harness, LEGACY pinned by P28-OPS-1 _Proven by PR #57's green CI (merged 2026-10-27; all 27 checks green, which ran every Postgres suite, the authz matrix and `immutable_rules`) on top of the merged-tree proofs of integration-phase28.md, including `refusals_and_logs_hold_in_every_flag_state`._

## Wave 29 — The client media pipeline, Live TV over shelves, the beta opens

Integration: [dev/handoff/integration-phase29.md](../dev/handoff/integration-phase29.md). The Postgres password file was
missing again, so no Postgres suite ran locally. PR #58's CI ran them (30 checks green, merge 6f7e48ef), and the
wave-30 integration ticked the five rows on that run. Decisions: P29-CNV-1 and P29-TV-1.

### BYOS-CNV — Conversion
- [x] **BYOS-CNV-001** [FE][WASM] Remux and subtitle extraction with ffmpeg.wasm: `apps/web/src/convert/*` (probe, remux, subtitles, lazy single-threaded core), the Convert step in the bucket browser and the Cinema submit picker, migration 1480 `playable_key` (set by the indexer from `converted/<stem>.mp4`, played by the shelves), `byos.client_convert_max_mib` (default 2048, P29-CNV-1) and `byos.require_client_remux` through the shelves summary, `THIRD_PARTY_NOTICES.md` _Proven by PR #58's green CI (merged 2026-10-30 01:51 UTC as 6f7e48ef; 30 checks green and the manual `visual-baselines` skipped, which ran every Postgres suite, the authz matrix, `immutable_rules`, the three-engine E2E and the real-ffmpeg media pipeline) on top of the merged-tree proofs of integration-phase29.md. Still open as a follow-up, not a proof: Library and Music modals: no video there yet (Music arrives in wave 30)._
- [x] **BYOS-CNV-002** [FE] Transcode with WebCodecs where the browser can: `convert/decide.ts`, `convert/webcodecs/*` (fMP4 demuxer, transcoder, mp4-muxer and webm-muxer), wired into the Convert step through `webcodecsBridge.ts` _Proven by PR #58's green CI (merged 2026-10-30 01:51 UTC as 6f7e48ef; 30 checks green and the manual `visual-baselines` skipped, which ran every Postgres suite, the authz matrix, `immutable_rules`, the three-engine E2E and the real-ffmpeg media pipeline) on top of the merged-tree proofs of integration-phase29.md. Still open as a follow-up, not a proof: the Chromium H.265 case of `convert.spec.ts` (not written) and the Firefox and WebKit path records._
- [x] **BYOS-CNV-003** [BE][OPS] The transcoder as an optional worker into the member's bucket: job kind `mount_transcode`, presigned I/O, `worker_tokens.scope` (migration 1500), `storage_object_transcodes` (1501), the mount transcode and worker-token routes, `byos.worker_for_members`, Admin > Transcoding's Mount column, `deploy/TRANSCODER.md` _Proven by PR #58's green CI (merged 2026-10-30 01:51 UTC as 6f7e48ef; 30 checks green and the manual `visual-baselines` skipped, which ran every Postgres suite, the authz matrix, `immutable_rules`, the three-engine E2E and the real-ffmpeg media pipeline) on top of the merged-tree proofs of integration-phase29.md._

### BYOS-TV — Live TV over shelves
- [x] **BYOS-TV-001** [BE][FE] Live TV over guild shelves, per-guild guides: migration 1510 `pool_source` (numbers unique per source), `TvCatalog::pool_in`, the watch rules, the "Server shelf" chip, imported guides only on instance channels and the guide order of P29-TV-1 _Proven by PR #58's green CI (merged 2026-10-30 01:51 UTC as 6f7e48ef; 30 checks green and the manual `visual-baselines` skipped, which ran every Postgres suite, the authz matrix, `immutable_rules`, the three-engine E2E and the real-ffmpeg media pipeline) on top of the merged-tree proofs of integration-phase29.md. Still open as a follow-up, not a proof: mount-backed items on air (a runtime from the indexer and the `EXT-X-BYTERANGE` renderer; wave 30 or a follow-up), the admin editor's source picker, member-created channels (BYOS-TV-002)._

### BYOS-NAV — Navigation
- [x] **BYOS-NAV-001** [FE] Auto nav and empty shelves: `STORAGE_MOUNT_UPDATE` (gateway), the summary refetch on READY and on the event, `ShelfEmpty` in Cinema and Library _Proven by PR #58's green CI (merged 2026-10-30 01:51 UTC as 6f7e48ef; 30 checks green and the manual `visual-baselines` skipped, which ran every Postgres suite, the authz matrix, `immutable_rules`, the three-engine E2E and the real-ffmpeg media pipeline) on top of the merged-tree proofs of integration-phase29.md._

## Wave 30 — The floating player and the music library

Integration: [dev/handoff/integration-phase30.md](../dev/handoff/integration-phase30.md). The Postgres password file was
missing, so no Postgres suite ran locally; every row below waits on its CI proofs (the handoff lists them). Decision:
P30-MUS-1 (agent defaults the owner was told about).

### MUS — Music
- [ ] **MUS-PLY-001** [FE] The persistent floating player: `apps/web/src/player/*` (store, engine over two gapless decks, graph with music-lib's EQ chain, Media Session, three skins, skin and dock picker in Settings > Appearance, player keys in `lounge/shortcuts.ts`), the PWA update deferred to a track boundary, signed-in app only (`player/scope.ts`, the router requests the gate on app routes only, D-385) _Merged tree: vitest src/player (incl. the gapless engine over a fake Web Audio context, the wiring and the static import guard), src/pwa, src/settings, the bundle budget's `player` row (no player code in the entrance) and `check-css-tokens`. Waits on: `e2e/tests/player.spec.ts` (signed in, full stack) and `player-layout.spec.ts` (mocked session) on CI. Not built: a launcher that opens the Library Manager with an empty queue (nothing outside the player starts playback yet)._
- [ ] **MUS-WASM-001** [WASM][FE] `sr-music-wasm`: tags (lofty =0.25.4 first, the hand-written ID3v2/v1, FLAC, Ogg, MP4 and WAV readers as the fallback), cover art, LRC; `loadMusic()` in `@sideroom/wasm` with `MusicNeedMoovError` _Merged tree: `cargo test -p sr-music-wasm`, vitest `packages/wasm/test/music.test.ts`, the bundle budget's `music` row (lazy, 197.9 KB brotli without wasm-opt, limit 600 KB), `cargo deny`, the notices copy. Waits on: `wasm-pack test --headless --chrome` (wasm.yml)._
- [ ] **MUS-IDX-001** [BE][FE][JOB] Auto-index and client-written tags: migration 1540, `PgMusicStore`, `GET /music/library`, `POST /music/library/rescan`, `GET|PATCH /music/tracks/{id}`, `POST /music/tracks/{id}/presign`, the lazy tagger (4 at a time, backoff, NEED_MOOV walk, covers at `covers/<hash>.jpg`) started by the library panel's first listing, Rescan tags per editable bucket; the bucket CORS rule now allows `range` _Merged tree: `sr-storage --lib` (incl. the CORS cases), `sr-api --lib`, `route_matrix`, vitest src/music. Waits on: `sr-db --test music_repo` (3 Postgres tests) and the authz matrix (CI)._
- [ ] **MUS-LIB-001** [BE][FE] Playlists, smart queue, gapless, EQ, crossfade, favorites, recent, search: migration 1550, `PgMusicLib` (`sr-db` `music_lib_repo`), the playlist, favorites and recent routes, `music.max_playlists` and `music.max_items`, the Library Manager panel in the player's Library tab, the port, the recent tracker, `GaplessDecks` in the player engine _Merged tree: `sr-api --lib routes::music` (10), `sr-admin` settings, `route_matrix`, vitest src/music and src/player. Waits on: `sr-db --test music_lib_repo` (8 Postgres tests), the authz matrix and `immutable_rules` (CI)._
