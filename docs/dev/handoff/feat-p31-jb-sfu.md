# feat/p31-jb-sfu: MUS-JB-001, a stereo music track in E2EE voice

Branch `feat/p31-jb-sfu` (base `integration/phase31`). Migration 1560 only. No dependency, nothing pushed, no PR, no
server or browser started (8080/5173 were not released, so the Playwright step has not run).

## Done

**Protocol (`crates/sr-voice-proto`)**
- `video::Source::Music` (JSON `"music"`), `MUSIC_MIN_KBPS` 32, `MUSIC_DEFAULT_KBPS` 256, `MUSIC_MAX_KBPS` 320 and
  `clamp_music_kbps`. A music line is declared with the existing op 12 `publish` (`source: "music"`), announced to the
  room with `published`, and a listener mutes one DJ's music with a `subscribe` hint `{source: "music", enabled: false}`.
- `VoiceCommand::SetMusic { channel, music_allowed, bitrate_kbps }` (additive; defaults apply until the gateway sends it).

**SFU (`crates/sr-voice`)**: `TrackSource::Music`, a second audio mid per member.
- Forwarded to every other member on the msid stream `<user>-music`, unless that member's hint turned it off. Never decoded
  (D-201): it is the same `forward_audio` path as the microphone, so it is also behind the E2EE-keyed gate
  (`e2ee_keyed`, frames counted in `unkeyed_frames`), the deaf flag and the room allowance.
- `SPEAK` is required (`Participant::allowed(Music)`); the channel setting `music_allowed` is `SfuCore::set_music`
  (off: the lines are withdrawn from everyone at once, voice untouched; the bitrate is clamped to 32..320 and kept per room).
- The SFU's Opus payload type 111 now advertises `stereo=1; sprop-stereo=1` (needed so the receiving browser decodes
  stereo). Consequence: a microphone line shares that payload type, so browsers may encode the (mono) mic as dual-mono
  stereo; each sender's real rate is capped client-side, so the cost is a few bytes per packet.
- Governor: the governor feeds on measured node egress, which already includes the music bytes, so music is counted once
  and at its real rate (320 kbit/s per listener at the maximum). `Snapshot.music_egress_bps` and `music_packets` expose the
  music share (a one second window); audio has no layers, so the governor can only lower video. The `VoiceEgress` alert
  comment in `deploy/monitoring/alerts.yml` says so. No new Prometheus gauge (`metrics.rs` is not in my row); the
  snapshot fields are ready for one.
- `crates/sr-voice/src/server.rs` got the `SetMusic` arm and the snapshot merge (needed to compile; not in my row's list).
- `crates/sr-voice/tests/video.rs`: two `unreachable!` arms for `Source::Music` so its exhaustive matches still compile.

**Migration `crates/sr-db/migrations/1560_channel_music.sql`**: `channels.music_allowed boolean NOT NULL DEFAULT true`,
`channels.music_bitrate_kbps smallint NOT NULL DEFAULT 256 CHECK 32..320`. No `DO $$` blocks. Not applied to a database
(see tests). Nothing reads the columns yet: see "Wiring left for the integrator".

**Web (`apps/web/src/voice`)**
- `protocol.ts`: `VideoSource` and `VideoHint.source` gain `music`; `parseStreamId` returns `kind: 'music'` for
  `<user>-music`; `tuneOpus(sdp, maxBitrate, profile = 'voice' | 'music')` (the `music` profile: `stereo=1; sprop-stereo=1;
  usedtx=0; useinbandfec=1; maxaveragebitrate=<kbps*1000>`); `clampMusicKbps` and the constants.
- `engine.ts` (`RtcVoiceEngine`): `publishMusic(stream, kbps?)`, `unpublishMusic()`, `setMusicMuted(muted)`,
  `setMusicVolume(percent)`, `ConnectParams.musicKbps`, `EngineEvents.remoteMusic(userId, stream | null)`. The music sender
  goes through `attach()`, so `e2ee.attachSender(sender, 'audio')` runs for it exactly as for the microphone (the same
  per-sender frame encryptor, D-249; E2EE stays non-optional). While a music line is attached the connection negotiates the
  `music` Opus profile (the payload type is shared), and goes back to the `voice` profile after `unpublishMusic()`. Each
  sender is capped separately with `setParameters` (mic at the channel bitrate, music at its own, never above 320).
- `playback.ts`: `RemoteAudio.addMusic/removeMusic/hasMusic/setMusicVolume/setMusicMuted` (own volume and mute; deafen, the
  output device and the PRIORITY_SPEAKER duck apply, so a priority speaker ducks the music; the duck is the existing
  `DUCK` 0.4) and `tapMusicForVoice(ctx, node)` which returns `{ stream, detach }` from a node of the player's graph.
- e2e-only handle `globalThis.__srVoiceEngine` (set only when `VITE_SR_E2E === '1'`, like `__srE2ee`).

**Bots**: `crates/sr-sdk/src/voice_transport.rs`: `ConnectOptions { music }`, `VoiceSession::connect_with`,
`VoiceLink::connect_with` (rejoins keep the option), `send_music_opus`, `start_music`, `stop_music`, `VoiceEvent::Music`,
`TransportError::NoMusicLine`; the music frame goes through the same `FrameTransform` as speech. Encode with
`audio::OpusEncoder::new(Channels::Stereo, ..)` and `set_bitrate` (D-377); the voice-opus feature itself is untouched.
`packages/bot/src/voiceTransport.ts` is the twin (`connect(.., {music: true})`, `sendMusicOpus`, `startMusic`, `stopMusic`,
`music` event, `no-music-line`).

## API for the integrator: putting the DJ on the line (jb-queue stream mode)

```ts
import { tapMusicForVoice } from './voice/playback';          // apps/web/src/voice/playback.ts
// the player's Web Audio graph: pick the node after EQ/crossfade (the one that feeds ctx.destination)
const tap = tapMusicForVoice(playerCtx, playerOutputNode);    // null without Web Audio
engine.publishMusic?.(tap.stream, channel.musicBitrateKbps);  // second audio sender, under the E2EE encryptor
// when the DJ stops, hands over, or the controller reports `streamed` no longer applies to this member:
engine.unpublishMusic?.();
tap.detach();
// listener side: nothing to do for playback (the engine plays <user>-music itself);
engine.setMusicMuted?.(true);        // mute music separately: local silence + hint to the SFU to stop sending it
engine.setMusicVolume?.(80);         // 0..200
// events.remoteMusic(userId, stream|null) says when somebody's track starts or ends.
```
The browser context must be the same `AudioContext` as the player's (a `MediaStreamAudioDestinationNode` belongs to its
context). `engine` is the `VoiceEngine` that `controller.ts` holds; the methods are optional on the interface (the mock
engine need not have them). The member needs SPEAK; the SFU silently drops the line without it or when `music_allowed` is off.

## Wiring left for the integrator (not in my row)
1. Read `channels.music_allowed` / `music_bitrate_kbps` (chat_repo, sr-chat domain and channel rules, the channel DTO and
   the settings card of jb-rooms) and send `VoiceCommand::SetMusic` from the gateway when a room opens and when the channel
   is edited; pass `musicKbps` in `ConnectParams`. Until then every channel behaves as allowed, 256 kbit/s.
2. Call `publishMusic` from the jukebox stream mode (jb-queue) as above.
3. A UI for the listener's music mute/volume (`setMusicMuted`, `setMusicVolume`).
4. CHANGELOG (the integrator writes it): "Phase 31: members with Speak can play music into a voice channel at up to 320
   kbit/s in stereo, end-to-end encrypted like speech; listeners can mute it separately; a channel can switch music off."

## Not done / open
- The `voice-live.spec.ts` music step is written (one `test.step` inside the existing two-user case, B asserts a second
  inbound audio stream with bytes and an Opus codec reporting `channels: 2`) but **has not been run**: the ports were not
  released and the stack was not started. `channels` is the rtpmap value, which engines report inconsistently, so it is
  asserted on Chromium only. It needs the web build with `VITE_SR_E2E=1` (the playwright config sets it).
- The eight-member soak with music on for one member (risks, section 9) was not run; it needs the voice-soak workflow.
- `packages/bot` live test (`voice.live.test.ts`) has no music case; the Rust integration test below covers the protocol.
- Postgres tests for migration 1560: the Postgres password file `%TEMP%\sr_pg_pw.txt` is missing, so the migration was read
  by eye (two `ALTER TABLE`, `DROP CONSTRAINT IF EXISTS` before `ADD CONSTRAINT`) and not applied. CI applies all migrations.
- No Prometheus gauge for music egress (see above).
- `getStats` stereo check, mic stereo side effect and a shared Opus payload type were reasoned from the SDP rules and not
  seen in a real browser: look at it in the live run (does the mic still sound right, is the music line really stereo).

## Tests run
- `cargo test -j 2 -p sr-voice -p sr-voice-proto`: all pass, including the new `crates/sr-voice/tests/music.rs` (8 tests:
  two audio mids and separate msid streams, music without mic frames, the per-listener mute and unmute, SPEAK required,
  `music_allowed` off/on and bitrate clamp, nothing forwarded before the E2EE handshake, the stereo fmtp in the SFU offer,
  music byte accounting vs 2 listeners at 320 kbit/s) and 2 new proto unit tests. Existing e2e/loopback/video/load_smoke pass.
- `cargo test -j 2 -p sr-sdk --no-default-features --features voice-transport,voice-e2ee --test voice_transport`: 7 pass,
  including the real in-process SFU case `a_bot_dj_sends_an_encrypted_music_track` and
  `music_needs_a_music_line_and_a_transform_in_an_encrypted_call`. (`voice-opus` not built: it needs cmake; no code there changed.)
- `cargo fmt --check` (sr-voice, sr-voice-proto, sr-sdk) clean; `cargo clippy -D warnings --all-targets` clean for sr-voice,
  sr-voice-proto, sr-sdk (voice-transport, voice-e2ee); sr-gateway compiles against the new variant.
- Web: `vitest run src/voice` 231 pass (new `engineMusic.test.ts`, 13 tests: Opus profiles, clamp, stream id, second sender
  under the same E2EE hook and announced as music, stereo profile on/off through renegotiation, bitrate caps, first-offer
  case, receive on `<user>-music` with the receiver hook for that user, separate mute with hints merged, leave cleanup,
  playback volume/mute/duck/deafen, the player tap); `tsc --noEmit`, eslint and prettier clean on the touched files;
  `vite build` + `scripts/check-bundle-budget.mjs` all ok. The wasm dist for the vitest e2ee suites was copied from the main
  checkout and `sr-music-wasm` built dev/no-opt locally (gitignored).
- `packages/bot`: `tsc --noEmit`, eslint, prettier, `vitest run src/voiceTransport.test.ts` pass.

## Decisions
- Music is its own `Source` (like screen audio) rather than a new "kind" field, so the existing op 12 and hints carry it.
- One shared Opus payload type means one fmtp for the connection: stereo/no-DTX is only negotiated by the client while a
  music line is attached; the SFU side always advertises stereo so listeners can decode it.
- `music_allowed` and the bitrate reach the SFU as a separate `VoiceCommand::SetMusic` instead of new fields on `SetFlags`/
  `JoinClaims`, to avoid breaking sr-gateway (not my files).

## Owner questions
- None blocking. For information: with the shared payload type a microphone may go out as dual-mono stereo Opus for
  everyone (negligible rate effect, capped client-side). If you would rather keep mono mic packets strictly, the alternative is
  a second Opus payload type for music (more SDP surgery on both sides); say so and I will do that in a follow-up.
