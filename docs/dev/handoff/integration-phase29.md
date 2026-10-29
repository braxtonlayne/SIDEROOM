# Integration handoff: integration/phase29 (wave 29)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p29-integ`, branch `integration/phase29`, base a382cd52 (main at PR #57 plus
the owner-approved ffmpeg.wasm and muxer downloads). Env: `CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p29-integ-target`,
`CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`, one cargo command at a time; `CMAKE` = the VS 2019 Build Tools copy for sr-sdk.
No Postgres (the password file `%TEMP%\sr_pg_pw.txt` is missing). Nothing pushed, no PR, nothing merged to main.

## Resume here

Done: the four merges, the webcodecs bridge, Fable's P29 decisions (two commits), `playable_key`, the licence check,
the ffmpeg.wasm notice, the inventory (wave-28 ticks, wave-29 section) and the CHANGELOG. In progress: OpenAPI, api-client
and MSW regeneration, then the gates (see "Gates"; a row without a result has not run yet).

## Merges

1. `feat/p29-conv-webcodecs` (145f3c4b): clean.
2. `feat/p29-conv-remux` (7130b117): `THIRD_PARTY_NOTICES.md` and its `apps/web/public` copy; both sections kept
   (muxers, then ffmpeg.wasm). `check-notices-copy.mjs` identical.
3. `feat/p29-conv-worker` (7b21b44b): clean.
4. `feat/p29-tv-byos` (a7a79708): clean (generated OpenAPI files are regenerated at the end anyway).

## Integration notes applied

- **WebCodecs bridge.** `convert/webcodecsBridge.ts` imports `./webcodecs` lazily with the module's real types (the
  `import.meta.glob` stand-in shapes are gone): ffmpeg demuxer names map to the table's containers (`matroska,webm` by
  extension), HEVC is `hvc1.*`, progress is `TranscodeProgress.fraction`, a streamed (null) blob is `NO_OUTPUT`. The
  unused `setDecider` / `registerTranscoder` / `setCapabilities` seam and its types are deleted; `convert/index.ts`'s
  synchronous rule is now `decideRemux`, so `decide` is only the WebCodecs table. ConvertStep asks the table first and
  uses the remux rule for the `-c copy` plan and the worker reason. Found at the join: with the real module loaded, jsdom
  answers `canPlayType` with `''`, so the table sent everything to the worker and four Convert tests failed; the tests
  now stub `canPlayType`, and `webcodecsBridge.test.ts` covers the mapping and the three paths.
- **`playable_key`.** The classifier links a non-native video to `converted/<stem>.mp4` (then `.webm`) in the same
  listing and ignores everything under `converted/` and `transcoded/` (never a title of its own). `PgIndexStore::apply`
  writes the copy, else the single MP4 of `storage_object_transcodes` (`converter` remux or worker); a finished MP4
  `mount_transcode` sets it at once. Shelves play `COALESCE(playable_key, key)` with the copy's container and
  `web_native` true. Tests: `a_converted_copy_is_the_playable_key_of_its_original_and_no_title_of_its_own` (sr-storage,
  ran) and `shelves_play_coalesce_playable_key_key` (sr-db Postgres, compiles, CI).
- **Licences.** `pnpm licenses list --json --prod | node scripts/check-licenses.mjs`: ok.
- **ffmpeg.wasm notice.** Verified: the GPL-2.0 text is byte-identical to xz's `COPYING.GPLv2`; the library list matches
  the configure line inside `ffmpeg-core.wasm` (FFmpeg 5.1, libvpx 1.13.1 by version string, HarfBuzz and libogg by
  symbol). Added the MIT text of `@ffmpeg/ffmpeg` and `@ffmpeg/util`. **Not verified:** the wrappers' upstream copyright
  line (the packages carry no LICENSE; `package.json` names Jerome Wu) and the versions of the other bundled libraries.
- **Inventory.** The eight wave-28 rows are ticked on PR #57's green CI; the wave-29 section is added, all rows open.
- **CHANGELOG:** `## Phase 29 - 2026-10-29`.

## Fable's decisions (commits 914fc23f and 99093ad3; rows P29-CNV-1 and P29-TV-1)

- Q1: `byos.client_convert_max_mib` (storage, 64..4096, default 2048), `DEFAULT_MAX_MIB = 2048`, the summary's `convert`
  policy, ConvertStep reads it when the dialog passes neither prop; tests in sr-admin, sr-api and Vitest.
  **Bug found and fixed:** boolean settings are stored as JSON `true`, but `setting_value_i64` read only integers, so
  `byos.worker_for_members` and `byos.require_client_remux` could never be switched on. `setting_i64` now reads booleans
  as 1/0 (`a_saved_boolean_setting_reads_as_one_or_zero`).
- Q5: route test `an_owner_queues_with_their_own_worker_token_when_the_operator_does_not_serve_members_and_gets_409_without_one`
  (new `routes/storage/transcode_tests.rs`, over the uploads harness's scope store, the cinema-media stub and a local
  HEAD-only bucket; the mounts_tests harness has no `ScopeDeps`).
- Q6: `storage_scopes_hold_over_http` asserts the guild manager's 403 and a non-member's 404 on a guild mount transcode
  (Postgres, CI).
- Q2: follow-up below. Q3: `imported_guide` refused on non-instance pools (test). Q4: migration 1510 drops the old
  `UNIQUE(number)` by catalog lookup; guide order instance, servers, personal on the server (Postgres and memory) and
  the web (`compareChannels`).

## Follow-ups (not done here)

- **BYOS-TV-002 (S):** member route for server/personal channels under the `manage` grant, caps `tv.channels_per_guild`
  (default 5, 0..50) and `tv.channels_per_user` (default 3, 0..20).
- Server-side enforcement of `byos.require_client_remux` (P29-CNV-1: the upload cannot tell an original from its copy).
- Mount-backed Live TV items on air: an indexer runtime plus the `EXT-X-BYTERANGE` renderer (wave 30 or a follow-up).
- HLS outputs of `mount_transcode` are not a `playable_key` (their segments would need signing); only MP4 is.
- A takedown of an original hides its shelf item but not a direct presign of its `converted/` copy by key.
- The Chromium H.265 case of `convert.spec.ts` (conv-webcodecs' plan in its handoff) is not written.
- No member UI for mount worker tokens yet (the routes exist).

## Gates (no database)

(Filled in as they run.)

## Left for CI (Postgres and browsers)
