# 08 — Lounge: Voice, Video, E2EE (`VOX`)

Phase 2. Spec: [lounge-voice-video](../specs/lounge-voice-video.md), ADRs 0003/0004, doc 08 §2–3.

## VOX-SIG — Signaling & state
- [x] **VOX-SIG-001** [BE] Voice state model (guild/channel, session, self/server mute/deaf, video, stream, suppress (stage), request_to_speak) *(suppress and request_to_speak_timestamp are used by stage channels, VOX-STG-001; state is in memory per node)*
- [x] **VOX-SIG-002** [GW] Op 4 VOICE_STATE_UPDATE handling, VOICE_STATE_UPDATE dispatch, VOICE_SERVER_UPDATE with endpoint/token/ICE servers *(SR_VOICE_ERROR explains refused joins; docs/05 section 6.5)*
- [x] **VOX-SIG-003** [BE] Join checks (CONNECT, user limit, MOVE bypass, age gate, AFK) *(gateway/src/voice.rs check_join: CONNECT/VIEW, user limit with MOVE_MEMBERS/MANAGE_CHANNELS/ADMINISTRATOR bypass, age gate (SR_VOICE_ERROR age_confirmation_required from accounts.age_confirmed_at), AFK channel move after the guild timeout (the SFU reports speech; checked every 15 s; AFK members listen only); tests in voice_tests::ext)*
- [x] **VOX-SIG-004** [BE] Voice token minting (HMAC, 60s TTL) + SFU node selection *(60 s HMAC token, rendezvous-hash node selection over voice.endpoints)*
- [x] **VOX-SIG-005** [BE] `sr-voice-proto` message types (identify, select protocol, session description, speaking, video, client connect/disconnect, heartbeat, resume) *(SDP offer/answer replace SELECT_PROTOCOL/SESSION_DESCRIPTION; VIDEO and QUALITY ops are reserved)*
- [x] **VOX-SIG-006** [BE] TURN credential endpoint (coturn REST secret) *(GET /sr/voice/turn and ice_servers, HMAC-SHA1, 1 h)*
- [x] **VOX-SIG-007** [BE] Server mute/deafen/move/disconnect APIs + permission checks *(PATCH /guilds/{id}/voice-states/{user}, audit log entries)*
- [x] **VOX-SIG-008** [BE] DM/group call lifecycle (ring, ringing timeout, CALL_CREATE/UPDATE/DELETE, missed-call system message) *(gateway/src/voice_call.rs: join a DM with op 4 without guild_id, rings everyone for 45 s, CALL_CREATE/UPDATE/DELETE, ring and stop-ringing REST, the CALL system message written by sr-api messages/call_record.rs; in memory on the node; tests in voice_tests::ext and messages tests)*
- [x] **VOX-SIG-009** [BE] Sender allowance counters per channel (excluding Patrons), viewers-per-stream counters, entitlement re-evaluation *(gateway/src/voice_stream.rs and voice.rs allowance_ok: per-channel sender counters (camera and stream one each, Patrons exempt), viewer cap per stream, SR_ENTITLEMENT_UPDATE re-reads limits and never kicks, SR_VOICE_LIMITS_UPDATE; the SFU only accepts video the allowance let start)*
- [x] **VOX-SIG-010** [BE] Stream (Go Live) keys: STREAM_CREATE/UPDATE/DELETE, watch/unwatch, preview thumbnail upload endpoint (periodic JPEG from streamer) *(gateway/src/voice_stream.rs: ops 18/19/20/22, STREAM_CREATE/UPDATE/DELETE, watch and unwatch with the viewer cap, preview upload and fetch (POST/GET /streams/{key}/preview, in memory, 5 s rate limit); the SFU forwards a screen only to the listed viewers)*

## VOX-SFU — `sr-voice`
- [x] **VOX-SFU-001** [BE] str0m integration: UDP socket loop, ICE-lite, DTLS-SRTP, ICE-TCP fallback *(UDP port range, ICE-lite, DTLS-SRTP, passive ICE-TCP with RFC 4571 framing)*
- [x] **VOX-SFU-002** [BE] Voice WS endpoint + token validation
- [x] **VOX-SFU-003** [BE] Room manager (create on first join, destroy on last leave), participant tracks
- [x] **VOX-SFU-004** [BE] Audio forwarding (Opus), audio-level header → speaking events *(audio-level header to SPEAKING events; push-to-talk enforced server side when USE_VAD is denied)*
- [x] **VOX-SFU-005** [BE] Video forwarding with simulcast layer selection per subscriber (viewport hints, BWE via TWCC) *(sr-voice layers.rs and core.rs: per-subscriber LayerSelector (client hint or viewport, bandwidth budget, governor cap), switches only on a keyframe, upgrade hold; TWCC when voice.video_bwe is on, else a loss-based fallback (loss.rs); loopback tests in tests/video.rs with real str0m clients)*
- [x] **VOX-SFU-006** [BE] Screen share tracks (+ screen audio) as separate stream group *(core.rs: screen and screen audio tracks with a shared msid stream group (<user>-screen), VIDEO publish/subscribe (op 12), forwarded only to viewers the gateway lists)*
- [x] **VOX-SFU-007** [BE] PLI/FIR coalescing, NACK/RTX handling *(sr-voice keyframe.rs: PLI/FIR from subscribers, new subscribers and layer switches coalesce into at most one request per 500 ms per track and layer; NACK and RTX are str0m's (1000 packet send buffer, RTX negotiated))*
- [x] **VOX-SFU-008** [BE] Egress governor (budget, per-tier priority downgrade, notices) *(sr-voice governor.rs: budget voice.egress_budget_mbps (0 = off), severity steps up every 5 s over budget and down every 15 s under 80 %, Level 0 capped first and Patrons last, QUALITY notice (op 13), sr_voice_governor_severity gauge)*
- [x] **VOX-SFU-009** [BE] Metrics (rooms, participants, egress/ingress Mbps, loss, CPU) *(rooms, participants, egress/ingress bps, loss ratio, forwarded packets; no CPU gauge yet)*
- [x] **VOX-SFU-010** [BE] Graceful drain on shutdown (RECONNECT hint) *(drain() closes clients with 4015 and refuses joins)*
- [x] **VOX-SFU-011** [TEST] Load test harness (headless senders/receivers in Rust) → 300 Mbps target *(sr-voice loadtest.rs and examples/loadtest.rs: headless str0m senders and viewers against one SfuCore, times only the core. First run (dev machine, release): 311 Mbit/s egress needed 110 % of one core, so the single media actor tops out near 280 Mbit/s; the 300 Mbit/s at under 50 % CPU target is NOT met and needs rooms sharded over several actors, then a run on the AX42)*

## VOX-E2E — End-to-end encryption
- [ ] **VOX-E2E-001** [BE][WASM] `sr-e2ee`: OpenMLS group per call, key packages, add/remove proposals, commits, welcome processing
- [ ] **VOX-E2E-002** [WASM] Frame encryptor/decryptor (AES-128-GCM, frame counter nonce, 8-byte tag, codec-aware unencrypted header ranges for Opus/VP8/H.264)
- [ ] **VOX-E2E-003** [FE] RTCRtpScriptTransform worker wiring (+ createEncodedStreams fallback)
- [ ] **VOX-E2E-004** [GW] Op 101 E2EE_MLS relay; server as external sender ordering commits per epoch
- [ ] **VOX-E2E-005** [FE] Identity key generation & storage (IndexedDB + non-exportable WebCrypto wrapping key)
- [ ] **VOX-E2E-006** [FE] Epoch transition handling (old-key grace window)
- [ ] **VOX-E2E-007** [FE] Privacy codes UI (shield popover, per-participant codes, verify)
- [ ] **VOX-E2E-008** [TEST] Interop test vectors Rust ↔ WASM; SFU plaintext-assertion test

## VOX-CLI — Client media engine
- [x] **VOX-CLI-001** [FE] getUserMedia device handling (hot-plug, permission prompts, device-lost recovery) _(feat/p10-voice-fe: a device lost mid-call falls back to the default input and keeps the call, with a dismissible note; ends the call only if the default fails too or the device keeps vanishing; controller.recoverMic, voice/__tests__/controller.test.ts)_
- [x] **VOX-CLI-002** [FE] Voice activity detection (energy gate + optional ML VAD), sensitivity
- [x] **VOX-CLI-003** [FE] Push-to-talk (keybind while focused, release delay, sounds), PTT-only channel enforcement (USE_VAD permission)
- [x] **VOX-CLI-004** [FE] Per-user volume (WebAudio gain 0–200%), local mute
- [x] **VOX-CLI-005** [FE] Output device selection (setSinkId)
- [ ] **VOX-CLI-006** [FE] Webcam publish with simulcast encodings, quality caps by entitlement _(partial, feat/p10-voice-fe: camera capture clamped to the entitlement (profiles from GET /donations/me and /guilds/{id}/support), device pick with fallback to the default, f/h/q simulcast encodings helper, self-view; not published because the SFU does not forward video (VOX-SFU-005), voice/video.ts, VIDEO_PUBLISH)_
- [ ] **VOX-CLI-007** [FE] getDisplayMedia screen/window/tab share with audio, quality selector, content hint (motion/detail) _(partial, feat/p10-voice-fe: getDisplayMedia screen/window/tab with audio, resolution and fps presets gated by entitlement, content hint motion/detail, browser Stop sharing handled, local LIVE tile; not published until the SFU forwards screen tracks (VOX-SFU-006), voice/video.ts)_
- [ ] **VOX-CLI-008** [FE] Stream preview thumbnail uploader
- [x] **VOX-CLI-009** [FE] ICE restart on network change, reconnect UI
- [x] **VOX-CLI-010** [FE] Voice persists across section navigation (global call store, never unmounted)

## VOX-UI — Interfaces
- [ ] **VOX-UI-001** [FE] Voice connection panel above user panel (Voice Connected / RTC Connecting, channel / server link, ping tooltip with graph, disconnect, camera, screen share, activity, soundboard buttons) _(partial, feat/p10-voice-fe: ping graph in the tooltip, camera and screen share buttons, recovered-mic note, refusal and governor notices; no Activities or soundboard buttons (no backend))_
- [x] **VOX-UI-002** [FE] Voice channel view: tile grid, focus mode, sidebar tiles, full-screen, hide non-video participants, "Join Voice" pre-join screen with camera preview _(feat/p10-voice-fe: grid, focus mode with a strip of the other tiles, full screen, hide non-video participants, Join Voice with a camera preview; lounge/voice/VoiceChannelView.tsx, JoinPreview.tsx, __tests__/callUi.test.tsx)_
- [ ] **VOX-UI-003** [FE] Participant tile (video or avatar, name, mute/deaf icons, speaking ring, E2EE, connection quality, context menu) _(partial, feat/p10-voice-fe: video or avatar tile (own camera, remote through the media store), mute/deaf/camera icons, speaking ring, own-tile connection quality, context menu; no remote per-tile quality (no stats) and no E2EE shield (VOX-E2E-007), lounge/voice/Tiles.tsx)_
- [ ] **VOX-UI-004** [FE] Stream tile (LIVE badge, viewer count, "Watch Stream", preview), stream viewer controls (volume, pop-out, full-screen, quality info) _(partial, feat/p10-voice-fe: stream tile with LIVE badge, Watch Stream / Stop Watching, volume, full screen and quality info; no viewer count or preview thumbnail (VOX-SIG-010) and no stream pop-out, Tiles.tsx StreamTile)_
- [ ] **VOX-UI-005** [FE] Bottom call controls (camera ▾ devices, share screen ▾ quality, activities, soundboard, mic ▾, deafen, disconnect, more ⋯) _(partial, feat/p10-voice-fe: mic with input-device menu, deafen, camera with device menu, share screen with options, text chat, watch party, more (layout, full screen, pop-out), disconnect; no Activities or soundboard (no backend), CallControls.tsx)_
- [x] **VOX-UI-006** [FE] Screen share picker modal (source type, quality resolution/fps radios gated by entitlement with Patron/level hint, share audio toggle) _(feat/p10-voice-fe: source type, resolution/fps radios gated by Patron or server-level entitlement with an upgrade hint, share-audio switch, content hint; lounge/voice/ScreenSharePicker.tsx)_
- [x] **VOX-UI-007** [FE] Text-in-voice chat panel _(feat/p10-voice-fe: the voice channel text chat beside the call (voice channels accept messages); lounge/voice/VoiceChat.tsx)_
- [ ] **VOX-UI-008** [FE] Voice user context menu (Profile, Mention, Message, Call, Add Note, User Volume slider, Mute, Disable Video, Server Mute/Deafen, Move To ▸, Disconnect, Timeout/Kick/Ban, Copy ID) _(partial, feat/p10-voice-fe: Profile, Mention, Message, Add Note, User Volume, Mute, Server Mute/Deafen, Move To, Disconnect, Timeout/Kick/Ban, Copy User ID in tiles and the channel list (useVoiceUserMenu.ts); Call (VOX-SIG-008) and Disable Video (remote video) are missing)_
- [x] **VOX-UI-009** [FE] Drag users between voice channels (MOVE_MEMBERS) _(feat/p10-voice-fe: drag a person onto another voice channel (PATCH voice-states, MOVE_MEMBERS) with a drop highlight; lounge/voice/voiceDrag.ts, ChannelSidebar.tsx)_
- [ ] **VOX-UI-010** [FE] Incoming call overlay (caller avatar, Accept voice/video, Decline, ringtone) + outgoing ringing state _(G-39: Decline now calls `POST /channels/{id}/call/stop-ringing`, the overlay follows CALL_UPDATE/DELETE and the local fallback timer is 45 s like the server; still no outgoing ringing state. Earlier note, partial, feat/p10-voice-fe: overlay with caller avatar, Accept, Accept with video, Decline, repeating ringtone and 30 s timeout, driven by CALL_CREATE/UPDATE/DELETE; the server sends none of those yet and a DM join is refused (VOX-SIG-008); no outgoing ringing state, lounge/voice/IncomingCall.tsx, voice/calls.ts)_
- [ ] **VOX-UI-011** [FE] DM call view (header area expands into call stage, resize handle)
- [x] **VOX-UI-012** [FE] Pop-out call/stream window (Document PiP API) _(feat/p10-voice-fe: Document Picture-in-Picture pop-out of the call (people, your video or share, mute, deafen, disconnect), copies styles, closes with the call; lounge/voice/PopOut.tsx. A remote stream pop-out waits for remote video)_
- [x] **VOX-UI-013** [FE] Sender-limit and viewer-cap refusal toasts with upgrade hints _(feat/p10-voice-fe: SR_VOICE_ERROR video_limit / viewer_cap keep the call, stop the camera and show the limit with Support and Patron links; the server does not send these codes yet (VOX-SIG-009), controller.onVideoRefusal)_
- [x] **VOX-UI-014** [FE] Egress governor "Reduced quality — the room is busy" notice _(feat/p10-voice-fe: QUALITY (op 13, reduced) shows "Reduced quality: the room is busy" in the voice panel; the SFU does not send it yet (VOX-SFU-008))_
- [ ] **VOX-UI-015** [FE] Voice channel status set/clear (pencil in channel header)
- [ ] **VOX-UI-016** [FE] Mobile call UI (full-screen, swipe to minimize, controls sheet) _(partial, feat/p10-voice-fe: compact tiles, swipe down to minimise, one scrolling touch-sized controls bar; no separate bottom sheet, lounge/voice/swipe.ts)_

## VOX-STG — Stage channels
- [x] **VOX-STG-001** [BE] Stage instances (topic, privacy), speaker/audience states, request to speak, invite to speak, moderators *(gateway/src/voice_stage.rs: stage instances (topic, privacy 2) with REST and STAGE_INSTANCE_* dispatches, the audience is suppressed at the SFU, request to speak, invite to speak, step down, auto-end, moderators = Manage Channels + Mute + Move; in memory on the node)*
- [ ] **VOX-STG-002** [FE] Stage view (speakers grid, audience list, raise hand, moderator controls, start/end stage modal)

## VOX-SB — Soundboard
- [x] **VOX-SB-001** [BE] Default sounds (original/CC0), guild sounds API, send soundboard sound event + cooldown *(sr-chat settings/soundboard.rs, sr-db 0640_soundboard.sql, sr-api settings/soundboard.rs: 48 guild sounds (MP3/Ogg/WAV, 5 s, 512 KiB, kept as uploaded: no server-side Opus transcode), eight synthesized default sounds, send-soundboard-sound with a 4 s cooldown, VOICE_CHANNEL_EFFECT_SEND, op 31)*
- [ ] **VOX-SB-002** [FE] Soundboard popover (search, favorites, server sections, preview, volume)
- [ ] **VOX-SB-003** [FE] Sound playback mixed into outgoing audio (E2EE path) + local playback for receivers' volume setting
