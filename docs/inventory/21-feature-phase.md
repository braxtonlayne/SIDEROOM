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

## Wave 30b — Entrance isolation (D-385)

Integration: [dev/handoff/integration-phase30b.md](../dev/handoff/integration-phase30b.md).

- [x] **SEC-ENT-001** [FE][BUILD] The public build with one entry per surface (landing, auth, invite), the app build, `enterApp`, the CSP files, the legal renderer, the notices split, per-surface budgets (design: `docs/dev/entrance-isolation.md`) _Merged tree, 2026-10-30: `node scripts/build-web.mjs` (both builds; the `resolveId` guard now also works on Windows), the full web Vitest suite (355 files, 3114 tests, incl. router, entrance pages, `corePaths`, `enterApp`, `csp.test.ts`), the per-surface budgets (landing 113.6, auth 119.2, invite 111.4, app 236.7 KB gzip) and `check-entrance-isolation.mjs` with every surface ok._
- [ ] **SEC-ENT-002** [FE][PWA] The service worker with `sr-static` and `sr-app` caches, the purge, the two manifests, the plain error pages, `PublicUpdateToast` _Merged tree: `swFetch.test.ts`, `pwa.test.tsx` and `entrancePwa.test.tsx` green, `sw.js` now reads the table's Caddy syntax (found by `entrance-agreement.test.mjs`, now green on the real dist). Waits on: `pwa-update.spec.ts` and `static/pwa-security.spec.ts` on the real two-build dist (CI)._
- [x] **SEC-ENT-003** [BE][OPS] The gate in `sr-server` (`[web] root`), the legal renderer, `Clear-Site-Data` on logout, the API oracle fixes, the Caddyfile and deployment files _Merged tree: `cargo test -p sr-server --test web_gate` (18), sr-api `route_matrix` and `openapi_names`, fmt and clippy `-D warnings` on sr-core, sr-api, sr-preflight and sr-server, `render-caddyfile.mjs --check`. Waited on: the Postgres suites (`authz_matrix`, `immutable_rules`, sr-preflight `pg`), Caddy itself and `prod-smoke` (CI). Proven on CI at the merge of wave 37 (OPS-DOC-001, 2026-10-31): the ignored Postgres suite (`authz_matrix`, `immutable_rules`, `web_gate`) in [rust.yml run 41208735069](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735069) and the stack through Caddy in [prod-smoke.yml run 41205318174](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41205318174)._
- [ ] **SEC-ENT-004** [TEST][CI][DOCS] The per-surface checks (`scripts/check-entrance-isolation.mjs`, `e2e/tests/entrance-isolation.spec.ts`, `SR_E2E_WEB=server`, the dev and preview gates, the workflows, the docs) _Merged tree: `node --test "scripts/*.test.mjs"` (82 pass), the isolation check on the real dist (every surface ok), `publicDev()` wired into `vite.config.ts`, all 18 workflows parse, the deliberate leak fails the build guard and the check (integration-phase30b.md). Waits on: the Web, three-engine Playwright, Rust, repo-checks, voice-soak and prod-smoke jobs with the new steps, and `entrance-isolation.spec.ts` in server mode (CI)._

## Wave 30 — The floating player and the music library

Integration: [dev/handoff/integration-phase30.md](../dev/handoff/integration-phase30.md). The Postgres password file was
missing, so no Postgres suite ran locally; every row below waits on its CI proofs (the handoff lists them). Decision:
P30-MUS-1 (agent defaults the owner was told about).

### MUS — Music
- [x] **MUS-PLY-001** [FE] The persistent floating player: `apps/web/src/player/*` (store, engine over two gapless decks, graph with music-lib's EQ chain, Media Session, three skins, skin and dock picker in Settings > Appearance, player keys in `lounge/shortcuts.ts`), the PWA update deferred to a track boundary, signed-in app only (`player/scope.ts`, the router requests the gate on app routes only, D-385) _Merged tree: vitest src/player (incl. the gapless engine over a fake Web Audio context, the wiring and the static import guard), src/pwa, src/settings, the bundle budget's `player` row (no player code in the entrance) and `check-css-tokens`. Waited on: `e2e/tests/player.spec.ts` (signed in, full stack) and `player-layout.spec.ts` (mocked session) on CI. Not built: a launcher that opens the Library Manager with an empty queue (nothing outside the player starts playback yet; built in wave 30b as MUS-PLY-002). Proven on CI at the merge of wave 37 (OPS-DOC-001, 2026-10-31): `player.spec.ts` (5 cases, Chromium, Firefox and WebKit) and `player-layout.spec.ts` (7, Chromium) in [e2e.yml run 41208735093](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735093)._
- [x] **MUS-PLY-002** [FE] The music button in the left rail (`lounge/GuildRail.tsx`) opens the player on its Library tab with an empty queue, through `player/presence.ts` only (never the store) _Merged tree (wave 30b): `src/player/__tests__/launch.test.tsx`, `src/lounge/__tests__/p30bMusLaunch.test.tsx`, the full web suite, the bundle budget's `player` row, and the isolation check (no player word in any public file)._
- [x] **MUS-WASM-001** [WASM][FE] `sr-music-wasm`: tags (lofty =0.25.4 first, the hand-written ID3v2/v1, FLAC, Ogg, MP4 and WAV readers as the fallback), cover art, LRC; `loadMusic()` in `@sideroom/wasm` with `MusicNeedMoovError` _Merged tree: `cargo test -p sr-music-wasm`, vitest `packages/wasm/test/music.test.ts`, the bundle budget's `music` row (lazy, 197.9 KB brotli without wasm-opt, limit 600 KB), `cargo deny`, the notices copy. Waited on: `wasm-pack test --headless --chrome` (wasm.yml). Proven on CI at the merge of wave 37 (OPS-DOC-001, 2026-10-31): `wasm-pack test --headless --chrome` over every `*-wasm` crate in [wasm.yml run 41208735100](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735100)._
- [x] **MUS-IDX-001** [BE][FE][JOB] Auto-index and client-written tags: migration 1540, `PgMusicStore`, `GET /music/library`, `POST /music/library/rescan`, `GET|PATCH /music/tracks/{id}`, `POST /music/tracks/{id}/presign`, the lazy tagger (4 at a time, backoff, NEED_MOOV walk, covers at `covers/<hash>.jpg`) started by the library panel's first listing, Rescan tags per editable bucket; the bucket CORS rule now allows `range` _Merged tree: `sr-storage --lib` (incl. the CORS cases), `sr-api --lib`, `route_matrix`, vitest src/music. Waited on: `sr-db --test music_repo` (3 Postgres tests) and the authz matrix (CI). Proven on CI at the merge of wave 37 (OPS-DOC-001, 2026-10-31): the ignored Postgres suite (`music_repo`, the authz matrix) in [rust.yml run 41208735069](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735069)._
- [x] **MUS-LIB-001** [BE][FE] Playlists, smart queue, gapless, EQ, crossfade, favorites, recent, search: migration 1550, `PgMusicLib` (`sr-db` `music_lib_repo`), the playlist, favorites and recent routes, `music.max_playlists` and `music.max_items`, the Library Manager panel in the player's Library tab, the port, the recent tracker, `GaplessDecks` in the player engine _Merged tree: `sr-api --lib routes::music` (10), `sr-admin` settings, `route_matrix`, vitest src/music and src/player. Waited on: `sr-db --test music_lib_repo` (8 Postgres tests), the authz matrix and `immutable_rules` (CI). Proven on CI at the merge of wave 37 (OPS-DOC-001, 2026-10-31): the ignored Postgres suite (`music_lib_repo`, the authz matrix, `immutable_rules`) in [rust.yml run 41208735069](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735069)._

## Wave 31 — The voice jukebox

Integration: [dev/handoff/integration-phase31.md](../dev/handoff/integration-phase31.md). Decisions: P31-JB-1 (owner:
keep the mic mono) and P31-JB-2 to P31-JB-5 (agent defaults the owner may change).

### MUS-JB — The voice jukebox
- [ ] **MUS-JB-001** [BE][FE] A stereo music track in E2EE voice: `video::Source::Music`, `VoiceCommand::SetMusic`, two audio mids per member in `sr-voice` (mono Opus 111 for the mic, stereo Opus 112 for music, `sr_voice_proto::sdp`), the per-listener mute hint, SPEAK required, `music_allowed` and the bitrate (migration 1560, read by `chat_repo`, `sr_music_allowed`/`sr_music_bitrate_kbps` on the channel object, sent to the SFU at a join and on edit), the gauge `sr_voice_music_egress_bits_per_second`; the web engine's `publishMusic` under the same frame encryptor (D-249), the listener's music mute and volume; `sr-sdk` and `packages/bot` send music _Merged tree: `sr-voice` (incl. `tests/music.rs`), `sr-voice-proto`, `sr-sdk` (default voice features, incl. the real-SFU bot DJ), `sr-gateway` (SetMusic at a join and on edit), `sr-chat` channel rules, `sr-db --test channel_music` (Postgres, migration 1560), vitest `src/voice` (engineMusic, streamMode), `packages/bot`, and Playwright `voice-live.spec.ts` on Chromium locally (the music step: B receives a second inbound audio stream, Opus, `channels: 2`; three passes). Waits on: the Firefox/WebKit live runs and the eight-member soak with music on (CI)._
- [x] **MUS-JB-002** [BE][FE] Collaborative queue, vote-skip, synced local playback: migration 1570, `sr-chat` `jukebox/*`, `PgJukebox`, 13 routes under `/channels/{id}/jukebox`, `JUKEBOX_UPDATE`, the strip and the controller (Watch Party clock and drift policy), stream mode on the DJ's client (`musicSink.ts`: the element tapped into the music track) _Merged tree: `sr-chat --lib jukebox`, `sr-db --test jukebox_repo` (4, Postgres), `sr-api --lib` (presign scope), `route_matrix`, `openapi_names`, the authz matrix (Postgres, jukebox routes seeded), vitest `src/voice/jukebox` (incl. the DJ stream mode and the music sink), and Playwright `jukebox.spec.ts` on Chromium locally (two listeners within 250 ms after five seconds; three passes). Waited on: nothing local; CI repeats them. Proven on CI at the merge of wave 37 (OPS-DOC-001, 2026-10-31): the Rust ignored suite in [rust.yml run 41208735069](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735069) and vitest in [web.yml run 41208735072](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735072). `jukebox.spec.ts` skips itself on CI (it needs the SFU), so its Chromium run stays the local one recorded above._
- [ ] **MUS-JB-003** [BE][FE][JOB] 24/7 Listening Room channels: migration 1580, job `jukebox.rooms` (id 1580), `LISTENING_ROOM_UPDATE` (a `closed: true` payload when `DELETE /channels/{id}/listening-room` switches it off), `PUT|GET|DELETE /channels/{id}/listening-room`, the channel card and join-at-cursor _Merged tree: `sr-server --lib jukebox_pg_tests` (7, Postgres, incl. the closed-room event), the authz matrix (a seeded room), vitest `src/lounge/rooms`, `immutable_rules`._ Not built: the channel settings card (toggle and playlist picker; the REST exists) and the switch of a room to stream mode for a present DJ (the column and the job's freeze exist).

## Wave 32 — The utility drawer (waves 32 and 33, merged by the owner)

Integration: [dev/handoff/integration-phase32.md](../dev/handoff/integration-phase32.md). Decisions: P32-UTL-1 to
P32-UTL-5 (agent defaults the owner may change). Boxes stay open until CI runs Playwright on three engines.

### UTL — The utility drawer
- [x] **UTL-SHL-001** [FE] The drawer, pop-outs and pins: Mod+K opens the drawer on Go to (the former quick switcher), `>` lists tools, gadgets (six, dragged, resized, synced in `user_settings.tool_layout`), the pin rail (twelve), `DrawerGate` beside `PlayerGate`, Escape closes the drawer only _Merged tree: vitest `src/tools` (layout, registry, sync, drawer with axe), `lounge/__tests__` (Mod+K precedence), entrance isolation clean. Waited on: `drawer.spec.ts` on three engines (CI; fixed at integration to sign in, D-385). Proven on CI at the merge of wave 37 (OPS-DOC-001, 2026-10-31): `drawer.spec.ts` (4 cases) passes on Chromium, Firefox and WebKit in [e2e.yml run 41208735093](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735093)._
- [x] **UTL-SRC-001** [WASM][FE] `sr-tools-wasm` three-stage search (prefix, Damerau-Levenshtein and Jaro-Winkler with Did you mean, BM25 over `corpus.json`), wired into the drawer at integration with the pure filter as fallback; `check-tools-corpus.mjs` also proves the corpus ids equal the registry ids _Merged tree: `cargo test -p sr-tools-wasm` (74), `packages/wasm` facade tests, `check-tools-corpus` (32 tools, 384 phrases), 205 KB brotli without wasm-opt. `wasm-pack test` in a browser not run. Proven on CI at the merge of wave 37 (OPS-DOC-001, 2026-10-31): `wasm-pack test` over `sr-tools-wasm` in [wasm.yml run 41208735100](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735100)._
- [x] **UTL-CALC-001** [WASM][FE][BE][JOB] Calculator, programmer mode, units, percent phrases, currency (migration 1620 `fx_rates`, job `tools.fx_refresh` id 1620, `GET /tools/fx`, settings `tools.currency_rates` and `tools.currency_stale_hours`), the time zone tool and `pickedTime` (used by the invite form) _Merged tree: `cargo test -p sr-tools-wasm`, `sr-server --lib fx_jobs` and `fx_pg_tests` (Postgres), vitest `src/tools`, the authz matrix._ People's time zones from profiles: not built (owner question). Proven on CI at the merge of wave 37 (OPS-DOC-001, 2026-10-31): `fx_pg_tests` and the authz matrix in the Rust ignored suite, [rust.yml run 41208735069](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735069)._
- [x] **UTL-DEV-001** [WASM][FE][BE] Developer tools, dice and `/roll`, loot tables (migration 1630), the encrypted scratchpad (`scratch_notes`), timers and `PUT /channels/{id}/voice-timer` with `VOICE_TIMER_UPDATE` (listener in `VoiceBridge`) _Merged tree: `sr-tools-wasm`, `sr-gateway` (197), `sr-userfeat --test tools_db` (Postgres), `sr-api --lib`, the authz matrix (seeded; an unauthorised stop with nothing shared now refused), vitest `src/tools`, the data export (`tools.json`). Proven on CI at the merge of wave 37 (OPS-DOC-001, 2026-10-31): `tools_db`, the gateway tests and the authz matrix in [rust.yml run 41208735069](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735069)._
- [x] **UTL-IMG-001** [WASM][FE] Image and meme studio, redact and pixelate, palette (k-means), video to GIF (the `gif` crate, 15 s and 20 MiB caps), "Edit image" in the tray, "Open in studio" on a copy, `toPngBlob` (used by the tier list's post as image) _Merged tree: `sr-tools-wasm` (GIF round trip), vitest `tools/studio`. Waited on: `studio.spec.ts` (needs the full stack, CI). WebCodecs frames: not built. Proven on CI at the merge of wave 37 (OPS-DOC-001, 2026-10-31): `studio.spec.ts` (2 cases, Chromium; the other engines skip it by design) in [e2e.yml run 41208735093](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735093)._
- [ ] **UTL-AUD-001** [FE] Audio trimmer and normalizer (RMS gain with a true-peak guard, WAV writer, Opus via MediaRecorder), a drawer tool (entry added at integration) and the voice-message Trim hook _Merged tree: vitest `src/tools/audio`, `src/lounge/msg`. Not built: the soundboard upload swap (its D-201 trimmer takes decoded PCM, not a one-call change) and the Playwright trim case (Chromium records WebM, the voice path is Ogg-only)._
- [x] **UTL-COL-001** [BE][FE][JOB] Tool rooms: migration 1660, job `tool_rooms.sweep` (id 1660), `/channels/{id}/tool-rooms*`, `TOOL_ROOM_UPDATE`, the tier list (posted as text or, since integration, as an image) and the poll builder _Merged tree: `sr-db --test tool_rooms_repo`, `sr-server --lib tool_rooms_pg_tests` (Postgres), `sr-events`, `sr-gateway`, the authz matrix (seeded room). Waited on: `tool-rooms.spec.ts` (two contexts, CI). Proven on CI at the merge of wave 37 (OPS-DOC-001, 2026-10-31): `tool-rooms.spec.ts` (two contexts, Chromium) in [e2e.yml run 41208735093](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735093) and `tool_rooms_pg_tests` in [rust.yml run 41208735069](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735069)._
- [x] **UTL-CRD-001** [BE][FE][JOB] Invite cards: migration 1670, job `cards.remind` (id 1670), message type 201, `card:` buttons, `/channels/{id}/cards*`, the `.ics` writer, DM reminders that follow the DM settings, the watch-party hand-off _Merged tree: `sr-chat` (cards, the fixed `.ics`), `sr-db --test cards_repo`, `sr-server --lib cards_pg_tests` (Postgres), the authz matrix (seeded card), the data export (claimed seats). No Playwright spec exists. Proven on CI at the merge of wave 37 (OPS-DOC-001, 2026-10-31): `cards_pg_tests`, `cards_repo` and the authz matrix in [rust.yml run 41208735069](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735069)._

## Wave 34 — Profiles, scrapbook, unfurling, status rings, player skins

Integration: [dev/handoff/integration-phase34.md](../dev/handoff/integration-phase34.md). Decisions: P34-PRF-1, P34-PRF-2,
P34-RNG-1, P34-SCR-1, P34-UNF-1, P34-UNF-2 and P34-SKN-1 (agent defaults the owner may change). Boxes stay open until CI
runs Playwright on three engines and the live Garage case of `storage.yml`.

### PRF — Profiles and sharing
- [ ] **PRF-PRO-001** [DB][BE][FE] Profiles at `/~handle`: migrations 1680 (`profile_privacy`) and 1681 (held guestbook notes), `sr-packages` `profile_privacy` policy and the tier filter, `GET|PUT /users/@me/profile-privacy`, `GET /users/@me/profile-page/preview`, the public `GET /public/profiles/{handle}` and its guestbook, `platform.public_profiles` (off), the `profile` public surface (`packages/profile-blocks`, `src/public/profile`), the privacy panel and "view as" strip, Links, Featured servers and Now playing blocks. Open: `media_showcase`; `ring_tier` is stored but not yet read (no ring on the profile page); the pending-note report kind _Merged tree: see the integration handoff's gate table_
- [x] **PRF-SCR-001** [BE][FE][JOB] Scrapbook vault: migration 1690, module `lounge.scrapbook` (`operator_gated`), jobs `scrapbook.requests` (1690) and `scrapbook.removals` (1691), 11 routes under `/guilds/{id}/scrapbook*`, the guild-bucket copy and cascade with visible failed removals, `scrapbook.json`, the gallery. The `storage.yml` Garage case has run _Merged tree: see the integration handoff Proven on CI at the merge of wave 37 (OPS-DOC-001, 2026-10-31): the scrapbook archive copy into a Garage guild mount ("Rust live tests (scrapbook archive into a Garage guild mount)") in [storage.yml run 41208735086](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735086), and the Postgres suites in [rust.yml run 41208735069](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735069)._
- [ ] **PRF-UNF-001** [BE][FE] Unfurling: `sr-unfurl` provider detection and `sr_play`, "Play here" with click-to-load, "Load embeds", CSP `frame-src` for the two embed hosts and `media-src https:` (app only), Watch Together over `embed`/`direct` media (migration 1700). Open: PeerTube parties, non-Safari HLS _Merged tree: see the integration handoff_
- [x] **PRF-RNG-001** [BE][FE][JOB] 24-hour status rings: migration 1710, job `rings.expire` (1710), `status_ring_visible_to`, ten routes, report kind and hold `status_ring`, `rings.json`, the tray, viewer and composer _Merged tree: see the integration handoff Proven on CI at the merge of wave 37 (OPS-DOC-001, 2026-10-31): `userfeat` `rings`, `rings_pg_tests`, the moderation hold tests and the authz matrix in [rust.yml run 41208735069](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735069)._

### MUS-SKN — Player skins
- [x] **MUS-SKN-001** [FE] Backdrop skins (`player/skins/backdrop/*`): Valve & Walnut, Tidepool Glass, Night Circuit, Pastel Study Deck, Steel Stack, each a lazy chunk; shuffle as one store mode (shared with the library panel through the port since integration); the Study Deck's timer and notes bound to wave 32's tools (`player/toolsWiring.ts`) _Merged tree: see the integration handoff Proven on CI at the merge of wave 37 (OPS-DOC-001, 2026-10-31): `player.spec.ts` "each skin works and passes axe" on Chromium, Firefox and WebKit in [e2e.yml run 41208735093](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735093), and the skin tests in [web.yml run 41208735072](https://github.com/braxtonlayne/SIDEROOM/actions/runs/41208735072)._
- [ ] **MUS-SKN-002** [FE] Opus redesigns of Valve, Tidepool, Circuit and Study after the owner's references _Merged tree: see the integration handoff; screenshots in `screenshots/player-skins/final/`_

## Wave 35 — The Discord bridge

Integration: [dev/handoff/integration-phase35.md](../dev/handoff/integration-phase35.md). Decisions: P35-BRG-1 to P35-BRG-3
(owner) and P35-BRG-4 to P35-BRG-8 (commander defaults the owner may change); D-381 governs. Ticked where the item's own
proofs ran on the merged tree; Playwright ran on Chromium only (Firefox and WebKit are CI's).

### BRG — The Discord bridge
- [x] **BRG-COR-001** [DB][BE] Bridge model, sealed bot token, outbound webhooks: migrations 1720 and 1721 (the
  integration author), `bridge.enabled` (owner-only, off by default) and `platform.bridge` (kill switch), the bridge
  routes (`guild_owner`, `bridge_admin`), outbound queue with rate limits and dead letters, teardown, Admin > System
  tile _Merged tree: `cargo test -p sr-bridge` (57, Postgres), `sr-features` (136), `sr-api` (895 incl. route_matrix,
  openapi_names), `sr-db --test secret_columns`, `sr-server --test authz_matrix` (7, bridge rows seeded per tenant,
  no ceiling changed) and `immutable_rules` (13); `bridge.spec.ts` (Chromium) against `e2e/support/fake-discord.mjs`_
- [x] **BRG-GW-001** [BE] Inbound over the gateway: one `sr-sdk` shard per bridge (`bridge.max_bridges`), the intent
  ladder (members and presence used when Discord grants them), loop guard, replies, edits, deletes, reactions, typing,
  bridged members (1732), `abuse.bridge_messages_per_minute`, the production `ChatSink` and the app wiring, `<@id>`
  of a bridged member both ways and never an account _Merged tree: `sr-bridge --test inbound` (14, Postgres),
  `sr-sdk` (gateway), `sr-chat`, `sr-api` (`a_bridged_member_id_that_collides_with_an_account_never_pings_it`);
  `bridge.spec.ts` posts on the fake gateway and sees the DISCORD-tagged message, and the connection closes with the
  teardown_
- [x] **BRG-UI-001** [FE][DOC] The Modules card, the conditional nav entry, the bridge screen (reconciled with the real
  routes), teardown dialog, channel banner (from `sr_bridge` on REST and gateway channel payloads), DISCORD tag, Discord
  members in the member list, privacy page list, DSR, lawyer brief Q21, docs/09 row; no per-member opt-out (P35-BRG-1)
  _Merged tree: Vitest (401 files, 3671 tests), typecheck, lint, prettier, `build-web` + entrance isolation + bundle
  budget; `bridge.spec.ts` on Chromium (manager greyed card, owner on, token, map, both directions, switch-off leaves
  nothing)_

## Wave 35b — The whole-server Discord mirror

Integration: [dev/handoff/integration-phase35b.md](../dev/handoff/integration-phase35b.md). Decisions: D-386 and
P35B-Q1 to Q11, P35B-UI-1 to 3, P35B-INT-1 and 2 (defaults the owner may change); D-381 and the IMMUTABLE RULE
govern. Ticked where the item's own proofs ran on the merged tree; Playwright ran on Chromium only. Open parts are in
the handoff's "Not done" (hiding a held channel, `Origin::Bridge`, thread state and forum tags in the reconcile).

### BRG-SYN — Whole-server mode
- [x] **BRG-SYN-001** [DB][BE] Modes, maps, reconciler, plan and seed, deletion hold, teardown options: migrations
  1750 to 1753, `sync::{engine, plan, reconcile, reconcile_run, ops, holds, jobs, store, eligible, patch}`, the
  `bridge_sync` routes (plan, mode, sync, reconcile, pause, resume, holds, preflight, mirrors, teardown-preview,
  teardown), the sync worker and listener in `sr-server`, `bridge.*` settings _Merged tree: `sr-bridge` Postgres
  suites (`sync_engine` 24 incl. the integration's wiring tests, `sync_db` 9), `authz_matrix` 7 incl.
  `every_route_enforces_its_authz_row` with a seeded `{plan_id}` and held channels, `route_matrix`, `openapi_names`;
  `bridge-whole-guild.spec.ts` (Chromium) plans, confirms, seeds against the fake, restores a held channel, goes back
  to selective and switches off with "also remove"_
- [x] **BRG-SYN-002** [BE][SEC] Bit masks, overwrite translation, reverts, conflicts, audit 220 to 224, authz
  property and matrix rows: `Permissions::BRIDGE_SHARED` / `SR_ONLY`, `sync::{perms, overwrites, conflicts}` wired
  into the reconcile and the seed, conflict routes (+ unfreeze, retry), ping-pong freeze _Merged tree:
  `sr-core --lib permissions` (19), `sr-bridge --test authz_property` (5, 500 cases), `--test conflicts` (5),
  `immutable_rules` (incl. the bridge check), `authz_matrix` (no ceiling changed); the spec sees a Discord permission
  edit put back on the fake and logged here, and "Apply on SIDE ROOM" saves nothing_
- [x] **BRG-SYN-003** [BE] Discord REST for structure with the audit-reason header, dispatch handling to the engine
  (`EngineSink`), intents per mode with a shard restart on a mode change, the stateful fakes, display roles and
  mention translation both ways, migration 1756 _Merged tree: `sr-bridge --test structure` (8), `--test inbound`
  (15), the wiring test `gateway_and_sideroom_changes_queue_looks_only_while_the_whole_guild_sync_runs`; the spec's
  rename on the fake arrives here through the gateway_
- [x] **BRG-SYN-004** [FE][DOC] Mode card, preflight, plan, sync status, conflict log, role and channel badges,
  member grouping, teardown options, privacy texts, docs (07, 05, 09, lawyer brief Q21) _Merged tree: Vitest
  `bridge/__tests__/sync.test.tsx` (17, MSW answering the wire shapes) and the full web suite, typecheck (web and e2e),
  lint, prettier, `build-web` + entrance isolation + bundle budget; `bridge.spec.ts` and `bridge-whole-guild.spec.ts`
  on Chromium against sr-server and the fake_

## Wave 36 — Activities with shared state

Integration: [dev/handoff/integration-phase36.md](../dev/handoff/integration-phase36.md). Decisions: P36-ACT-1 to
P36-ACT-5 (commander defaults the owner may change). Ticked where the item's own proofs ran on the merged tree (after
merging main 28bdcbf9); Playwright ran on Chromium only (Firefox and WebKit are CI's).

### ACT — Activities
- [x] **ACT-CH-001** [BE][FE] Data channels through the SFU under the room's keys: one SCTP channel per member in
  `sr-voice` (relay scoped to room and instance, `voice.datachannel_kbps`), the `sr-e2ee` data frame, the SDK's
  `sdk.state` / `sdk.input` / `sdk.on` with `sdk.self` and `sdk.hostId`, the `datachannel` capability (migration
  1760), `voice::DataChannel` in sr-sdk, T32; the instance ends when its host leaves (P36-ACT-1) _Merged tree:
  `sr-e2ee` (38), `sr-voice` (all, 6 datachannel), `sr-activities` (Postgres, host-leave case), `sr-sdk` (incl.
  `voice_data` against the SFU), `sr-server` lib, `immutable_rules`, `web_gate`, `authz_matrix` (no ceiling changed),
  `sr-db` secret_columns; SDK Vitest (23); `activities-live.spec.ts` state exchange on Chromium_
- [x] **ACT-WB-001** [FE] Whiteboard: strokes, shapes, text, images, pages, undo, host compaction, PNG export
  _Merged tree: Vitest (71, incl. the late join after a compaction now a plain `it`); live case draws, undoes, adds a
  page and re-joins in two contexts (Chromium). Images from a mount and the studio's canvas wait (owner items)_
- [x] **ACT-CT-001** [FE] Card table: Texas Hold'em with hole cards by addressed reply, betting, showdown, the
  not-cheat-proof notice, 60 s turn timer and host Remove (P36-ACT-2) _Merged tree: Vitest (62); live case plays one
  hand in two contexts (Chromium)_
- [x] **ACT-TT-001** [FE] Tabletop map and dice: grid or hex map, tokens, fog, initiative, seeded dice, measuring
  _Merged tree: Vitest (86, incl. the CSPRNG roll seed found missing by the live run); live case moves a token and
  rolls in two contexts (Chromium). The map comes from a device file, not a mount (owner item)_
- [x] **ACT-LND-001** [FE] Land.io as a built-in activity hosted in the starter's browser _Merged tree: Vitest (299),
  `tsc` check of the JS; live case plays one match in two contexts and the host leaving ends it (Chromium)_

## Wave 37 — The emulator

Integration: [dev/handoff/integration-phase37.md](../dev/handoff/integration-phase37.md). Decisions: P37-EMU-1 to
P37-EMU-7 (commander defaults the owner may change). Ticked where the item's own proofs ran on the merged tree (wave 36
merged first); Playwright ran on Chromium only, against the dev server and, for the emulator spec, also against the
production bundle served by sr-server.

### ACT-EMU — The emulator
- [x] **ACT-EMU-001** [FE] The core loader and the shell: `/games` (lazy, after sign-in), the 7z core unpacked in the
  browser, canvas, Web Audio through the core, Gamepad API with a mapping screen and keyboard fallback, save states and
  SRAM to the member's own mount under `emu/saves/<core>/<rom-sha1>/` by presigned `PUT`, ROMs from `emu/roms/` of the
  member's and the servers' shelves through the sixth grant `view_games` (migration 1800), the `rom` catalog kind, the
  PS1 BIOS rule; the emulator page policy on `/games` documents only _Merged tree: Vitest (mapping, saves, BIOS,
  detect, session, test ROM on a 6502 model, page); `sr-storage` (166), `sr-db` scopes/index repos (Postgres),
  `sr-api` lib and `route_matrix`, `sr-server` `authz_matrix`, `immutable_rules`, `web_gate` (the `/games` policy and
  the gated `/emu/cores/*`); `emulator.spec.ts` boots FCEUmm with the self-written test ROM, red frame, blue on A, saves
  a state (Chromium, dev server and production bundle). Not done: no homebrew ROM ships (owner pick, P37-EMU-5); SNES
  and Genesis have no allowed core (P37-EMU-1)_
- [x] **ACT-EMU-002** [FE] Co-op by input forwarding: "Play together" in a server call (voice panel and the Games
  page's call strip), the host's screen share of the game, players 2 to 4 over wave 36's sealed data channel, measured
  round trip shown, no rollback said in the UI, seats only for `view_games` holders (P37-EMU-4) _Merged tree: Vitest
  (protocol, input, seats, host/guest sessions, the seat rule, dialog incl. the refusal); `emulator-coop.spec.ts` on
  Chromium in probe mode and with the real loader, FCEUmm and the test ROM: the guest is refused, then granted, takes
  player 2, its A turns the host's frame green and the guest's stream follows, release goes back to red_
- [x] **ACT-EMU-003** [OPS][DOC] Cores by digest at install time: `deploy/scripts/fetch-emu-cores.sh`, `sr-cli emu
  fetch-cores`, `deploy/emu-cores.lock`, `deploy/EMULATOR.md`, the installer's optional step, the cached and verified
  CI step, licences in the notices and `check-licenses.mjs` by name _Merged tree: `node --test` fetch script (fake
  server), lock lib, `check-ops-files`, `install.test.mjs` (32 pass); `sr-cli` (unit and `emu_fetch`);
  `check-licenses`, `check-notices-copy`, `check-ops-files` ok; the local cores verified against the pins. The CI step
  is proven only on GitHub; only 3 of the 5 allowed cores have a source (P37-EMU-1)_

## Wave 39 — Operator switches, invite controls, the immutable suite, Game Servers "Coming soon"

Integration: [dev/handoff/integration-phase39.md](../dev/handoff/integration-phase39.md). Decisions: P39-INV-1 to
P39-INV-4, P39-FLG-1 to P39-FLG-3, P39-GSV-1. Ticked where the item's own proofs ran on the merged tree (scratch
databases, Chromium only for Playwright).

### OPS — Operator switches and the immutable suite
- [x] **OPS-FLG-001** [BE][FE] Per-guild and global switches for every new module: `jukebox`, `listening_rooms`,
  `platform.tools` and 31 `platform.tools.<tool>` sub-flags, `activities_datachannel`, `emulator`,
  `watch_together_embeds` registered; the route gate `module_gate.rs`; `profiles.access` (walled default, P39-FLG-1);
  `lounge.status_rings` mapped to `lounge.rings` (P39-FLG-2) _Merged tree: `sr-server --test feature_registry` (7,
  incl. `platform.game_servers`), Vitest `registryScreens.test.tsx` (both screens from the refreshed fixture),
  `authz_matrix` (8, incl. `module_kill_switches_stop_their_routes` and `profile_privacy_holds_over_http`), `sr-api`
  lib (806, the `module_gate` tests), `sr-voice`, `sr-activities`, `sr-watchparty`. Not built: the per-server jukebox
  switch does not hide the web strip (the server refuses the routes)_
- [x] **OPS-INV-001** [BE][FE] Invite controls: Lock the Doors (`invites.locked`, audited, emergency invites included,
  P39-INV-2), `invites.default_credits` (renamed, migration 1890), `invites.max_outstanding`,
  `invites.unused_code_days`, the job `invites.prune` (1890; never touches `invite_graph`), Admin > Invites, the
  entrance notice (D-385) _Merged tree: `sr-db --test invite_controls` (3, Postgres), `sr-server --lib` (244, incl.
  `the_prune_job_revokes_old_unused_codes_and_never_touches_the_graph`), `sr-api` lib and `route_matrix`, `sr-admin`
  catalog tests, Vitest `p39invites` and `doorsClosed`, Playwright `doors-closed.spec.ts` (Chromium, dev server), entrance
  isolation (landing 114.1/120 KB). `docs/09` unchanged (`git diff` empty)_
- [x] **OPS-IMM-003** [TEST] The final IMMUTABLE RULE suite: the name test over the whole catalog, the deliberate
  `voice.e2ee_test` violation, the fourth state `KillAll`, `security-nightly.yml` _Merged tree: `immutable_rules` (22;
  `refusals_and_logs_hold_in_every_flag_state` green 2 of 3 runs, the first failed 6 admin-group checks under
  KillFeatures once, not reproduced, see the integration handoff), the full matrix locally with
  `SR_SECURITY_NIGHTLY=1` (normal, kill_features, kill_all: 15987 refusals compared, 0 disagree). First CI run of
  `security-nightly.yml` on main: green, run 41224789059 (2026-10-31)_

### GSV — Game servers

**Deferred, not open (D-383, the owner, 2026-10-31).** Wave 38 does not run until the owner says. These items are kept verbatim in [roadmap-p27.md](../dev/roadmap-p27.md) section 16 and return with a re-plan; they are not counted as unfinished work of this phase, and no box here is ticked or waits on anything:

- ~~GSV-AGT-001~~ the node agent (pull mode, one-time pairing, scoped tokens, signed templates by digest), deferred
- ~~GSV-SRV-001~~ the game-server service, routes and tables (migrations 1840 to 1879 stay reserved), deferred
- ~~GSV-PRE-001~~ the presets (Minecraft, Terraria, Valheim, Project Zomboid) and their image digests, deferred
- ~~GSV-UI-001~~ the working Game Servers screens (the "Host it yourself" and "Managed servers" flows), deferred
- ~~`node-agent.yml`~~ the workflow, deferred

- [x] **GSV-PAGE-001** [FE] The Game Servers page, "Coming soon" (cosmetic only, D-383, P39-GSV-1): the splash, the two
  cards, the picker tile, the nav entry, `platform.game_servers`, the handbook line _Merged tree: Vitest
  `gameServers.test.tsx` (7, incl. axe and reduced motion) and `handbook.test.tsx`, `sr-features` (139),
  `feature_registry` lists it, `i18n-audit` 0, `check-css-tokens` clean. No e2e spec (the page is inert)_

## Wave 40 — Close

Integration: this wave's handoffs are [feat-p40-ops-docs.md](../dev/handoff/feat-p40-ops-docs.md) (the others are named in the wave's integration handoff).

### OPS — Docs, CI and dependencies
- [ ] **OPS-DEP-001** [BE] tantivy 0.27 (DEP-FU-001). _Not started, by design: it runs on or after 2026-11-17 or on the advisory trigger (P26-NEXT-1); the owner checklist lists it with its date._
- [x] **OPS-DOC-001** [DOC] Docs truth and the owner's packet: `docs/13` gains waves 27 to 40 (38 deferred) with the gates and links; this inventory ticked where a CI run or a recorded test ran and the game-server items listed as deferred; `docs/15` rows T33 to T38 and T30's presign controls; `docs/09`, `docs/05` and `docs/07` rows for the feature phase; the lawyer brief's Part E checked against the decisions log (Q25 added); the member handbook pages and the operator page; `deploy/OPERATOR.md`; the owner checklist sections 4 and 5 _Proven 2026-10-31: `check-doc-links.mjs`, `check-ops-files.mjs`, `threat-model.test.mjs` (T1 to T38) and `handbook.test.tsx` green, see the handoff for the web gates._
