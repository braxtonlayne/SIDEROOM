# Integration handoff: integration/phase31 (wave 31, the voice jukebox)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p31-integ`, branch `integration/phase31`, base 6203bebe (wave 30 tree).
Env: `CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p31-jb-queue-target`, `CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`,
one cargo command at a time. Postgres was available: every Postgres suite below ran locally against a scratch
database of this integration (created and dropped). Nothing pushed, no PR, nothing merged to main.

## Resume here

Done: merges, wiring, gates (table below), docs. Nothing is half-finished. What is left is CI (the list under "What CI
must prove") and the owner's answers to the defaults P31-JB-2 to P31-JB-5.

## Merges (each `--no-ff`, its own commit)

1. `origin/main` (3f79f4f9, PR #60 plus the authz-matrix seeds of wave 30): clean.
2. `feat/p31-jb-sfu` (9de7f1bc, MUS-JB-001, migration 1560): clean.
3. `feat/p31-jb-queue` (38f1230c, MUS-JB-002, migration 1570; it already contained `feat/p31-jb-rooms`, whose one
   `lib.rs` conflict jb-queue resolved): clean. The expected adjacent-line conflicts (sr-events, sr-gateway, app.rs,
   CHANGELOG, docs/05, gatewayDocs) did not happen: jb-sfu touched none of those files.
4. `feat/p31-jb-rooms` (0aa09f92, MUS-JB-003, migration 1580): already contained ("Already up to date").

Migrations 1560, 1570 and 1580 have no `DO $$` blocks; a fresh database migrated through 1580 (`sr-cli migrate` for
the Playwright run, and the TestDb template of the Postgres suites).

## Wiring (the integration's own work)

- **Channel music settings** (MUS-JB-001): `Channel.music_allowed` and `music_bitrate_kbps` (sr-chat domain), read
  and written by `chat_repo` (select, insert, update), validated in `channel_rules` (voice and stage only, 32 to 320),
  `sr_music_allowed` / `sr_music_bitrate_kbps` on the channel object and the PATCH body, `VoiceChannelInfo` in the
  gateway. The gateway relays `VoiceCommand::SetMusic` on every fresh join (before the token) and when an occupied
  channel is edited (`music_refresh` on `ChannelUpdated`). Tests: a `channel_rules` unit test, the gateway test
  `the_sfu_learns_the_music_settings_at_join_and_after_a_channel_edit` (the move test now expects the SetMusic too),
  `sr-db --test channel_music` (defaults, round trip, the CHECK).
- **musicKbps**: the voice controller passes the channel's `sr_music_bitrate_kbps` as `ConnectParams.musicKbps`.
- **Jukebox stream mode on the DJ** (`apps/web/src/voice/jukebox/musicSink.ts`): the jukebox controller takes a
  `MusicSink`; while this member is the DJ of a stream-mode session with a playable track it calls `start()`, which
  routes the strip's `<audio>` through Web Audio once (source to destination, so the DJ still hears it), taps the
  source with `tapMusicForVoice` and has the voice controller publish the tap (`VoiceController.publishMusic`, kept
  across a rejoin, dropped on leave). Stop, hand-over, a switch to synced, leaving the jukebox and unmount call
  `stop()` (unpublish, detach). The DJ's element asks the bucket for CORS like the floating player (`crossOrigin`
  for a cross-origin presign). Tests: `voice/jukebox/__tests__/streamMode.test.ts`.
- **Listener music mute and volume**: `musicVolume` (0 to 200) and `musicMuted` in the voice settings (per device),
  applied to the engine at start and on change; `MusicControls` in the jukebox strip (mute toggle and slider; the
  mute also sends the SFU hint). Strings: `i18n/messages/en/p31-integ-1.ts`.
- **Closed listening room**: `DELETE /channels/{id}/listening-room` publishes `LISTENING_ROOM_UPDATE`
  `{channel_id, guild_id, closed: true}` (`PgListeningRooms::with_bus`); the client forgets the room and a listener
  stops (`ingestRoomClosed`). Tests: `jukebox_pg_tests::closing_a_room_announces_it_so_the_card_disappears`, the
  rooms vitest.
- **Gauge**: `sr_voice_music_egress_bits_per_second` (the music share of the egress, already counted in it).
- **Generated files**: OpenAPI and the api-client schema regenerated (MSW unchanged, `mocks:check` green, 1059
  operations); gateway docs regenerated and formatted.

## Fixes found by running the Postgres suites (they had never run)

- `jukebox_pg_tests`: the fixture bound the playlist item id as its track id, so all six cursor tests failed on the
  `music_playlist_items` foreign key. Fixed in the fixture (the job was right).
- Authz matrix: every admitted jukebox and listening-room write answered 404 because `{channel_id}` was a text channel.
  Each tenant now gets a voice channel with a jukebox session, four queue rows of its own track and a listening room on
  its playlist; `ctx_for` maps the jukebox routes onto it, the write bodies name the rows, an overlay maps the
  `{item_id}` of `jukebox/queue/{item_id}`, and the cursor columns the `jukebox.rooms` job moves are left out of the
  refused-write fingerprint for `listening_rooms` (the room's channel, playlist, mode and DJ are still compared).

## Gates (all run locally on the merged tree)

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| clippy `-D warnings --all-targets`: sr-voice, sr-voice-proto, sr-chat, sr-events, sr-admin / sr-db, sr-api, sr-gateway / sr-server / sr-sdk (default voice features, CMAKE set) | clean |
| `cargo test` sr-voice (incl. `tests/music.rs`, 8), sr-voice-proto | pass |
| `cargo test -p sr-sdk` (voice-e2ee, voice-transport, voice-opus) | pass (voice_transport 8) |
| `cargo test` sr-chat, sr-events, sr-gateway | 353 / 15 / 188 pass |
| `sr-api --lib`, `--test route_matrix`, `--test openapi_names` | 742 / 6 / 2 pass |
| `sr-server --lib` | 96 pass (81 ignored: Postgres) |
| Postgres `sr-db --test jukebox_repo`, `--test channel_music` | 4 / 1 pass |
| Postgres `sr-server --lib jukebox_pg_tests` | 7 pass |
| Postgres `sr-server --test authz_matrix` | 6 pass (rest: 0 failures, 16 weak refusals, the ceiling) |
| Postgres `sr-server --test immutable_rules` | 12 pass |
| Web `tsc --noEmit`, eslint, prettier `--check .` | clean |
| vitest, all of `apps/web` (`--maxWorkers=2`) | 353 files, 3089 tests pass |
| `packages/bot` tsc and vitest | pass (38, 1 live test skipped) |
| `vite build` and `check-bundle-budget.mjs` | ok (entrance 249.9 KB, no player code in it) |
| `check-css-tokens`, `i18n-audit`, `check-notices-copy`, `check-doc-links` | clean |
| `node --test "scripts/*.test.mjs"` | 58 pass |
| Workflow YAML parse (all `.github/workflows/*.yml`, `deploy/monitoring/alerts.yml`) | ok |
| Playwright Chromium, local full stack (sr-server with the SFU, `memory:` storage, Vite on 5173): `voice-live.spec.ts` (with the music step) and `jukebox.spec.ts` | pass, 3 runs each |
| Playwright Chromium: `voice-e2ee.spec.ts`, `keyboard-6-voice.spec.ts`, `player.spec.ts` | pass |

## What CI must prove

- The e2e `live` job: `voice-live.spec.ts` on Linux Chromium (the music step), and the Firefox and WebKit voice E2EE
  steps (the second payload type changes every offer; Firefox and WebKit have not seen it).
- `jukebox.spec.ts` in CI: it needs `psql` on the runner's PATH and `SR_TEST_DATABASE_URL`, or it skips; check that
  the e2e job provides both.
- The Postgres job: every suite above plus the whole ignored set (`--include-ignored --test-threads=1`).
- The voice soak (eight members) with music on for one member, the roadmap's data point (not run anywhere yet).
- The bundle budget and the entrance guard on Linux.

## Not covered end to end

- The DJ's stream mode through the real strip (a jukebox in stream mode, the DJ's element tapped into the call) is
  covered by unit tests and by the engine half in the music step of `voice-live.spec.ts`, not by one browser spec.
- The listening room's settings card (toggle and playlist picker) is not built; the REST exists. Switching a room to
  stream mode for a present DJ is not wired (the column and the job's freeze exist).
- The werift bot against a live SFU with a music line (only its offer text is tested).

## Owner questions (the defaults are recorded as P31-JB-2 to P31-JB-5 in the decisions log)

1. Keep `jukebox.vote_skip_percent` (an integer, 50), or add a decimal `vote_skip_ratio`?
2. A track a listener cannot read: keep showing its length (the title hidden), or hide the length too?
3. Listening-room titles: visible to everyone who can view the channel, or only to members with the server music grant?
4. Rooms play only from the server shelf and the instance bucket: allow a member's bucket per room?

P31-JB-1 (keep the mic mono: the mic on Opus 111 mono, music on Opus 112 stereo) is the owner's answer.
