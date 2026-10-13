# 08 — Lounge: Voice, Video, E2EE (`VOX`)

Phase 2. Spec: [lounge-voice-video](../specs/lounge-voice-video.md), ADRs 0003/0004, doc 08 §2–3.

## VOX-SIG — Signaling & state
- [ ] **VOX-SIG-001** [BE] Voice state model (guild/channel, session, self/server mute/deaf, video, stream, suppress (stage), request_to_speak)
- [ ] **VOX-SIG-002** [GW] Op 4 VOICE_STATE_UPDATE handling, VOICE_STATE_UPDATE dispatch, VOICE_SERVER_UPDATE with endpoint/token/ICE servers
- [ ] **VOX-SIG-003** [BE] Join checks (CONNECT, user limit, MOVE bypass, age gate, AFK)
- [ ] **VOX-SIG-004** [BE] Voice token minting (HMAC, 60s TTL) + SFU node selection
- [ ] **VOX-SIG-005** [BE] `sr-voice-proto` message types (identify, select protocol, session description, speaking, video, client connect/disconnect, heartbeat, resume)
- [ ] **VOX-SIG-006** [BE] TURN credential endpoint (coturn REST secret)
- [ ] **VOX-SIG-007** [BE] Server mute/deafen/move/disconnect APIs + permission checks
- [ ] **VOX-SIG-008** [BE] DM/group call lifecycle (ring, ringing timeout, CALL_CREATE/UPDATE/DELETE, missed-call system message)
- [ ] **VOX-SIG-009** [BE] Sender allowance counters per channel (excluding Patrons), viewers-per-stream counters, entitlement re-evaluation
- [ ] **VOX-SIG-010** [BE] Stream (Go Live) keys: STREAM_CREATE/UPDATE/DELETE, watch/unwatch, preview thumbnail upload endpoint (periodic JPEG from streamer)

## VOX-SFU — `sr-voice`
- [ ] **VOX-SFU-001** [BE] str0m integration: UDP socket loop, ICE-lite, DTLS-SRTP, ICE-TCP fallback
- [ ] **VOX-SFU-002** [BE] Voice WS endpoint + token validation
- [ ] **VOX-SFU-003** [BE] Room manager (create on first join, destroy on last leave), participant tracks
- [ ] **VOX-SFU-004** [BE] Audio forwarding (Opus), audio-level header → speaking events
- [ ] **VOX-SFU-005** [BE] Video forwarding with simulcast layer selection per subscriber (viewport hints, BWE via TWCC)
- [ ] **VOX-SFU-006** [BE] Screen share tracks (+ screen audio) as separate stream group
- [ ] **VOX-SFU-007** [BE] PLI/FIR coalescing, NACK/RTX handling
- [ ] **VOX-SFU-008** [BE] Egress governor (budget, per-tier priority downgrade, notices)
- [ ] **VOX-SFU-009** [BE] Metrics (rooms, participants, egress/ingress Mbps, loss, CPU)
- [ ] **VOX-SFU-010** [BE] Graceful drain on shutdown (RECONNECT hint)
- [ ] **VOX-SFU-011** [TEST] Load test harness (headless senders/receivers in Rust) → 300 Mbps target

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
- [ ] **VOX-CLI-001** [FE] getUserMedia device handling (hot-plug, permission prompts, device-lost recovery) _(partial, feat/p5-voice-client: permission prompts and blocked-mic help, hot-plug device lists, fallback to the default device; a device lost mid-call ends the call with a fix-it message instead of recovering)_
- [x] **VOX-CLI-002** [FE] Voice activity detection (energy gate + optional ML VAD), sensitivity
- [x] **VOX-CLI-003** [FE] Push-to-talk (keybind while focused, release delay, sounds), PTT-only channel enforcement (USE_VAD permission)
- [x] **VOX-CLI-004** [FE] Per-user volume (WebAudio gain 0–200%), local mute
- [x] **VOX-CLI-005** [FE] Output device selection (setSinkId)
- [ ] **VOX-CLI-006** [FE] Webcam publish with simulcast encodings, quality caps by entitlement
- [ ] **VOX-CLI-007** [FE] getDisplayMedia screen/window/tab share with audio, quality selector, content hint (motion/detail)
- [ ] **VOX-CLI-008** [FE] Stream preview thumbnail uploader
- [x] **VOX-CLI-009** [FE] ICE restart on network change, reconnect UI
- [x] **VOX-CLI-010** [FE] Voice persists across section navigation (global call store, never unmounted)

## VOX-UI — Interfaces
- [ ] **VOX-UI-001** [FE] Voice connection panel above user panel (Voice Connected / RTC Connecting, channel / server link, ping tooltip with graph, disconnect, camera, screen share, activity, soundboard buttons) _(partial, feat/p5-voice-client: state, channel/server link, quality bars with ping tooltip, settings, disconnect; no ping graph, camera, screen share, activity or soundboard buttons yet)_
- [ ] **VOX-UI-002** [FE] Voice channel view: tile grid, focus mode, sidebar tiles, full-screen, hide non-video participants, "Join Voice" pre-join screen with camera preview _(partial, feat/p5-voice-client: audio-only tile grid and Join Voice screen; no focus mode, sidebar tiles, full screen or camera preview)_
- [ ] **VOX-UI-003** [FE] Participant tile (video or avatar, name, mute/deaf icons, speaking ring, E2EE, connection quality, context menu) _(partial, feat/p5-voice-client: avatar tile, name, mute/deaf icons, speaking ring, context menu and volume; no video, E2EE or per-tile quality)_
- [ ] **VOX-UI-004** [FE] Stream tile (LIVE badge, viewer count, "Watch Stream", preview), stream viewer controls (volume, pop-out, full-screen, quality info)
- [ ] **VOX-UI-005** [FE] Bottom call controls (camera ▾ devices, share screen ▾ quality, activities, soundboard, mic ▾, deafen, disconnect, more ⋯) _(partial, feat/p5-voice-client: mute, deafen, disconnect only)_
- [ ] **VOX-UI-006** [FE] Screen share picker modal (source type, quality resolution/fps radios gated by entitlement with Patron/level hint, share audio toggle)
- [ ] **VOX-UI-007** [FE] Text-in-voice chat panel
- [ ] **VOX-UI-008** [FE] Voice user context menu (Profile, Mention, Message, Call, Add Note, User Volume slider, Mute, Disable Video, Server Mute/Deafen, Move To ▸, Disconnect, Timeout/Kick/Ban, Copy ID) _(partial, feat/p5-voice-client: User Volume, local Mute, Server Mute/Deafen, Move To, Disconnect in the channel list, tiles and member list; the profile/DM/ban entries are not in the voice menu)_
- [ ] **VOX-UI-009** [FE] Drag users between voice channels (MOVE_MEMBERS)
- [ ] **VOX-UI-010** [FE] Incoming call overlay (caller avatar, Accept voice/video, Decline, ringtone) + outgoing ringing state
- [ ] **VOX-UI-011** [FE] DM call view (header area expands into call stage, resize handle)
- [ ] **VOX-UI-012** [FE] Pop-out call/stream window (Document PiP API)
- [ ] **VOX-UI-013** [FE] Sender-limit and viewer-cap refusal toasts with upgrade hints
- [ ] **VOX-UI-014** [FE] Egress governor "Reduced quality — the room is busy" notice
- [ ] **VOX-UI-015** [FE] Voice channel status set/clear (pencil in channel header)
- [ ] **VOX-UI-016** [FE] Mobile call UI (full-screen, swipe to minimize, controls sheet)

## VOX-STG — Stage channels
- [ ] **VOX-STG-001** [BE] Stage instances (topic, privacy), speaker/audience states, request to speak, invite to speak, moderators
- [ ] **VOX-STG-002** [FE] Stage view (speakers grid, audience list, raise hand, moderator controls, start/end stage modal)

## VOX-SB — Soundboard
- [ ] **VOX-SB-001** [BE] Default sounds (original/CC0), guild sounds API, send soundboard sound event + cooldown
- [ ] **VOX-SB-002** [FE] Soundboard popover (search, favorites, server sections, preview, volume)
- [ ] **VOX-SB-003** [FE] Sound playback mixed into outgoing audio (E2EE path) + local playback for receivers' volume setting
