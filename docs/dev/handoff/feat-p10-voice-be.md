# Handoff: feat/p10-voice-be (voice backend: streams, stages, calls, soundboard, SFU video)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p10-voice-be, branch feat/p10-voice-be (base integration/phase10). Local commits only, nothing pushed. Scope: docs/inventory/08-lounge-voice.md, the backend, gateway and test items VOX-SIG-003/008/009/010, VOX-SFU-005/006/007/008/011, VOX-STG-001, VOX-SB-001. **VOX-E2E-\* was not started** (MLS E2EE is its own large effort); its status is under "Still open".

## Read this first (voice client and UI agents)
The wire contract is in `docs/specs/lounge-voice-video.md` (new sections 4.9 to 4.12, 8a, 8b; soundboard notes in 7; the allowance notes in 3) and `docs/05-api-and-gateway.md` sections 6.5 and the new 6.6. What changes for a client:

1. **Video publishing needs a declaration.** After adding a `sendonly` video (or second audio) m-line, send `op 12 {"t":"publish","mid":"<your mid>","source":"camera"|"screen"|"screen_audio","active":true,"width","height","fps"}`. Camera = VP8 or H.264 simulcast with RIDs **`q`, `h`, `f`** (quarter, half, full). The SFU drops video the gateway did not allow: set `self_video` through op 4 first (the allowance can refuse with `SR_VOICE_ERROR video_limit_reached {limit, count}`), and start a screen share with gateway op 18, not with `self_stream` in op 4 (ignored).
2. **Subscriptions.** Cameras arrive by default as a thumbnail; send `op 12 {"t":"subscribe","subscriptions":[{user_id, source, enabled?, layer?, viewport?}]}` with the tile size to get more. Screens (and their audio) arrive only after `STREAM_WATCH` (op 20) **and** a `subscribe` for that source. The msid stream ids are `<user>` (mic), `<user>-camera`, `<user>-screen` (screen and its audio share it).
3. `op 13 QUALITY {reduced, max_layer, reason}` is the "Reduced quality: the room is busy" notice. `op 12 published/layer` messages announce live tracks and layer changes.
4. New dispatches: `STREAM_CREATE/UPDATE/DELETE`, `STAGE_INSTANCE_CREATE/UPDATE/DELETE`, `CALL_CREATE/UPDATE/DELETE`, `VOICE_CHANNEL_EFFECT_SEND`, `GUILD_SOUNDBOARD_SOUNDS_UPDATE`, `SOUNDBOARD_SOUNDS` (answer to op 31), `SR_VOICE_LIMITS_UPDATE`. New `SR_VOICE_ERROR` codes: `age_confirmation_required`, `video_limit_reached`, `stream_quality_not_allowed`, `stream_full`, `unknown_stream`, `not_in_voice`, `already_streaming` (`stage_unsupported` is gone: stage channels can be joined).
5. DM calls: op 4 without `guild_id`. The voice token has no `guild_id`.
6. `GUILD_CREATE` does not carry stage instances, running streams, running calls or soundboard sounds. Fetch them: `GET /guilds/{id}/stage-instances`, `GET /channels/{id}/call`, gateway op 31, and the `self_stream` flag on voice states.
7. A voice state moved to the AFK channel arrives like a moderator move: `VOICE_STATE_UPDATE` and a new `VOICE_SERVER_UPDATE` (the client reconnects its media).

## What was built
- **Gateway** (`crates/sr-gateway/src`): `voice.rs` (join checks with the age gate, AFK sweep, moderation with `suppress`, state in `Option<GuildId>` for DM calls, `allowance_ok`), `voice_stage.rs`, `voice_stream.rs`, `voice_call.rs`, `voice_sound.rs`. New `GuildDirectory` methods with fail-closed defaults: `channel_guild`, `voice_guild`, `age_confirmed`, `voice_limits`, `dm_call_members`, `soundboard_sounds`. The directory in `sr-server/src/gateway_pg.rs` implements them over Postgres (entitlements through `sr_payments::Entitlements`).
- **SFU** (`crates/sr-voice/src`): `core.rs` rewritten around tracks (mic, camera, screen, screen audio) and a single `reconcile` that decides each subscriber's lines; `layers.rs` (selector), `keyframe.rs` (coalescer), `governor.rs`, `loss.rs` (loss-based bandwidth fallback), `loadtest.rs` plus `examples/loadtest.rs`. `sr-voice-proto/src/video.rs` has the op 12/13 types; `JoinClaims` and `VoiceCommand::SetFlags` carry `camera`, `screen`, `tier`; new `VoiceCommand::StreamViewers`.
- **Soundboard**: migration `0640_soundboard.sql`, domain and file inspection in `sr-chat/src/settings/soundboard.rs` (WAV, Ogg Opus/Vorbis, MP3 duration parsers; eight synthesized default sounds; play cooldown), service ops, Postgres repo, routes in `sr-api/src/routes/settings/soundboard.rs`, `DomainEvent::SoundboardUpdated`.
- **REST**: `sr-api/src/routes/voice_live.rs` (stage instances, stream previews, call routes), `voice.rs` (`suppress`, `@me` stage state, `VoiceBackend` growth), `messages/call_record.rs` (the `CALL` message and its `call` object). Route matrix rows reviewed; OpenAPI, TypeScript client and mocks regenerated.
- **Config**: `voice.egress_budget_mbps` (default 0 = governor off), `voice.video_bwe` (default false). Documented in `docs/dev/server.md`.
- New error codes: 10067, 10097, 30045, 150006 (Discord's), 900012 (cooldown).

## Done (inventory ticked)
VOX-SIG-003, 008, 009, 010; VOX-SFU-005, 006, 007, 008, 011 (harness; see the target below); VOX-STG-001; VOX-SB-001. Each tick carries a one-line note naming the file.

## Still open, with reasons
- **VOX-E2E-001..008**: not started (OpenMLS, WASM frame encryptor, ScriptTransform, op 101 relay, identity keys, privacy codes UI). The SFU is E2EE-compatible by construction: frames are forwarded opaque; header extensions and the codec payload descriptor (which carries the keyframe flag) are the only things it reads. VOX-E2E-004 (op 101 relay and external-sender ordering) is a gateway item that could start independently.
- **The 300 Mbit/s exit gate is not met.** First `loadtest` run (Windows dev machine shared with seven other jobs, release build): 5 publishers x 1.5 Mbit/s, 40 viewers watching all five, 311 Mbit/s egress, the media actor busy 110 % of one core (106 % scaled to 300 Mbit/s). A single actor therefore tops out near 280 Mbit/s. The core has no global state, so the fix is to shard rooms across several actors (`TODO(scale)` in `server.rs`) and measure again on the AX42-class host. Not done here: it is an architecture change, and the first number should come from the real hardware.
- **No real browser or encoder has run against the video path.** Everything is covered with str0m clients and tagged byte frames (`crates/sr-voice/tests/video.rs`). Real simulcast from Chrome (RID header extensions, SSRC mapping), H.264 profile negotiation, and the keyframe flag from real H.264 need a manual pass once the web client exists.
- **All client work** (VOX-CLI-\*, VOX-UI-\*, VOX-STG-002, VOX-SB-002/003): not in scope here. No web code was touched, so no i18n fragment, no bundle budget and no Playwright were involved.
- **State is in memory on the node**: stage instances, streams and previews, DM calls and rings (like voice states). A restart ends them (members reconnect and rejoin). Multi-node needs a shared store first (the existing note in `voice.rs`).
- A member's governor **tier is fixed when they join the room**; a server level change updates the allowance (`SR_VOICE_LIMITS_UPDATE`, viewer caps, Patron exemption) but a member's tier for the governor changes at the next join.
- AFK moves are checked every 15 s (the gateway sweep), so the real delay is `afk_timeout` plus up to 15 s. Idle is "no speech heard by the SFU and no voice state change"; a member who only listens or watches a stream is moved, as on Discord.
- Server-side Opus transcode of soundboard sounds (spec section 7 says "-> Opus"): not done, no codec library in the build. Files are stored as uploaded and browsers decode MP3, Ogg and WAV.
- A subscriber whose browser lacks the publisher's codec gets nothing from that publisher (no transcoding, SFU never transcodes). VP8 and H.264 are both enabled, which every current browser handles.

## Tests run
- `cargo fmt --all --check`, `cargo clippy -p sr-core -p sr-events -p sr-chat -p sr-db -p sr-api -p sr-gateway -p sr-voice -p sr-voice-proto -p sr-server --all-targets -D warnings`: clean.
- New unit and in-process tests: sr-gateway 116 (stage, streams, previews, allowance, age gate, AFK with a paused clock, DM calls with ring timeout, soundboard, SFU flags), sr-voice 36 unit + 8 video loopback + 2 load smoke (plus the 21 audio tests that existed, unchanged and green), sr-voice-proto 17, sr-chat soundboard 11 + service 2, sr-api voice_live 5 and soundboard 5 and call record 3, route matrix 3.
- Postgres (scratch database `sr_p10_voice_be`, created and dropped here): `settings_repo` 6 (new soundboard round trip), `voice_pg_tests` 2 (new: age gate, AFK settings, entitlements, DM members, sounds on real rows) with `-- --include-ignored --test-threads=1`.
- Full `cargo test` (non-Postgres) passes for sr-core (283), sr-events, sr-chat (244), sr-api (421 + 34 integration), sr-server (25), sr-db, sr-gateway, sr-voice, sr-voice-proto. The machine ran out of memory once while 8 agents built at the same time (spurious "memory allocation failed" and "can't find crate"); rerunning with `-j 2` was clean.
- Load run: `cargo run --release -p sr-voice --example loadtest -- --publishers 5 --viewers 40 --seconds 6 --warmup 8`.

## Decisions (mine; revert freely)
- **D-046 holds unchanged**: voice activity or push-to-talk; a channel forces push-to-talk only by denying `USE_VAD` (`media_flags.ptt_required`). Nothing in this work adds a push-to-talk requirement.
- **TWCC bandwidth estimation is off by default** (`voice.video_bwe`). With it on, str0m pads every sending connection to 50 kbit/s, including audio-only participants, which would add about 5 Mbit/s for a 100 person room and 100 Mbit/s on a 2000 participant node. The default fallback is the loss reported in receiver reports. Flip it on for video-heavy nodes.
- **The gateway decides who may publish video**, not the client: a camera is allowed only while `self_video` passed the sender allowance and `STREAM` is held; a screen only while the member has a stream. The SFU enforces it, so the sender limit and the viewer cap cannot be bypassed from the client.
- `STREAM` ("Video") is required for the camera as well as the screen (Discord's one permission).
- Stage moderators are those holding all of Manage Channels, Mute Members and Move Members in the channel (Discord's definition); the audience is suppressed at the SFU; a stage with nobody in it ends.
- The stage instance id is the channel id (one instance per channel at a time).
- `USE_EXTERNAL_SOUNDS` is now in the default `@everyone` permissions of **new** guilds (D-044: sounds from other servers are free). Existing guilds keep what they have.
- The call record stores its details as JSON in the message `content` and renders Discord's `call` object; no schema change.
- Screen shares have one layer and bypass the layer selector (`force_single`). Cameras start as thumbnails.
- A deleted or moved participant's streams end; a stream does not follow its owner into another room.

## Open owner questions
1. **Shard the SFU actor now or later?** The load numbers say one actor saturates a core near 280 Mbit/s; the exit gate needs several. This is the main remaining risk for VOX-SFU-011's target.
2. Turn `voice.video_bwe` on by default for deployments that expect heavy video, accepting the 50 kbit/s padding per connection?
3. Backfill `USE_EXTERNAL_SOUNDS` into existing guilds' `@everyone` roles (a migration), or leave it to owners?
4. Persist stage instances and running calls (a table plus a bus) so they survive a restart and work across nodes, or keep them tied to the voice states they describe?
5. Does the web client want `stage_instances` and `soundboard_sounds` inside `GUILD_CREATE` (as Discord does) instead of the REST and op 31 fetches? It is a small follow-up in `gateway_pg.rs`.
