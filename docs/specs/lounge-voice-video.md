# Spec — Lounge: Voice, Video, Screen Share

Phase 2 · Decisions: D-045, D-046, D-050–D-053 · ADRs 0003, 0004 · Doc 08 (limits).

## 1. Surfaces
| Surface | Notes |
|---|---|
| Server voice channels | Join by click; text-in-voice chat; user limit; bitrate from server level |
| Stage channels | Speakers / audience / request to speak / stage topic; moderators invite to speak |
| DM & group DM calls | Ring UI (incoming call overlay with Accept/Decline, ringtone), call window inside the DM |
| Watch Party channels | See watch-parties spec. Video comes from HLS, not the SFU |

## 2. Media capabilities
- **Audio:** Opus, 48 kHz, DTX, FEC. Bitrate per channel = min(channel setting, server-level cap): 64/96 free, 128/256/384 with levels. Voice Activity Detection (client-side, `@ricky0123/vad`-like, or a WebAudio energy gate with sensitivity slider) **or Push-to-Talk** (D-046): configurable keybind, release delay 0–2000 ms, PTT sound cues. In the PWA, keybinds work while the tab is focused. A global PTT needs native clients (documented limitation). Priority speaker. Per-user volume (0–200%) and local mute. Server mute/deafen. Self mute/deafen. Echo cancellation and AGC via browser constraints. **No noise suppression** (D-045).
- **Video (webcam):** VP8/H.264 simulcast with 3 layers (full, ½, ¼). Free max 720p30. Levels/Patrons up to 1080p60.
- **Screen share ("Go Live"):** window/tab/screen picker, **with audio** (tab/system audio where the browser supports it), quality selector (resolution × fps) limited by entitlements, "stream preview" thumbnails in the channel list, viewer count, "Watch Stream" / "Stop Watching," pop-out stream window, focus and grid layouts.
- **Layouts:** grid, focus (spotlight), sidebar of tiles, full screen, pop-out (Document Picture-in-Picture API when available).
- **Indicators:** speaking ring, mute/deaf icons, "LIVE" badge, video icon, connection quality bars, E2EE shield (click → verification codes).

## 3. Sender allowance (D-050/D-052)
- `sr-server` keeps `active_video_senders[channel]`, excluding Patrons.
- Starting camera or stream: if `count ≥ limit(channel)` → reject with toast "This channel has reached its video limit (5). Support this server to raise it, or become a Patron to stream anytime." Camera and screen share each count as one.
- Viewers per stream cap (doc 08). The next viewer sees "This stream is full."
- Limits are re-evaluated on `SR_ENTITLEMENT_UPDATE`. Lowering a level never kicks existing streams. It only blocks new ones.

## 4. SFU (`sr-voice`)
- str0m-based. One UDP port range plus ICE-TCP fallback. DTLS-SRTP transport. Rooms keyed by channel id and pinned to a node.
- **Signaling:** through the gateway (`VOICE_STATE_UPDATE` → `VOICE_SERVER_UPDATE` with endpoint + token), then a small WebSocket to `sr-voice` for SDP/ICE (`sr-voice-proto`, Discord-voice-gateway-like opcodes: IDENTIFY, SELECT_PROTOCOL, READY, SESSION_DESCRIPTION, SPEAKING, HEARTBEAT, CLIENT_CONNECT/DISCONNECT, VIDEO, plus MLS ops via the main gateway op 101).
- Forwarding: simulcast layer selection per subscriber based on their viewport (tile size), bandwidth estimate (TWCC/REMB), and the **egress governor** (doc 08 §3). Keyframe request (PLI/FIR) coalescing. Speaker detection from RTP audio-level header extension (works under E2EE, since the header isn't encrypted).
- Metrics: rooms, participants, egress Mbps, packet loss, per-room bitrate.
- **Load target (exit gate):** a single AX42-class box forwards 300 Mbps of egress at < 50% CPU.

## 5. End-to-end encryption (D-053)
Modeled on Discord's DAVE:
1. **Key agreement:** MLS (RFC 9420) via `openmls` (Rust, compiled to WASM for the client). Each voice session is an MLS group. Each client has a per-session signature key bound to its identity key, signed by its account's persistent identity key (stored in IndexedDB, wrapped by a non-exportable WebCrypto key).
2. **Delivery service:** `sr-server` relays MLS key packages, proposals, commits, and welcomes over gateway op 101, and acts as the external sender that orders commits per epoch. It cannot read keys.
3. **Media:** each sender derives a per-epoch media key via the MLS exporter → per-frame AES-128-GCM (12-byte nonce from a frame counter, truncated 8-byte tag) applied in an `RTCRtpScriptTransform` worker (`sr-e2ee` WASM). Codec headers needed for packetization stay unencrypted (as in SFrame/DAVE). Fallback for browsers without ScriptTransform: Chrome's legacy `createEncodedStreams`. If neither is available, the client can't join E2EE calls (clear error).
4. **Membership changes** (join/leave) cause a new epoch. Senders re-key. Frames under the old key are accepted during a short transition window.
5. **Verification:** per-participant "privacy code" (fingerprint of the MLS epoch authenticator + identity key), shown in the shield popover; optional out-of-band compare.
6. **Bots/apps in voice** must participate in MLS through the SDK to send or receive audio (as with DAVE).
7. **Not E2EE:** watch-party HLS streams (library content), and text chat in voice.

## 6. TURN
coturn with ephemeral credentials from `GET /api/v1/sr/voice/turn` (HMAC, TTL 1h). ICE servers are delivered in `VOICE_SERVER_UPDATE`. TURN/TLS on 443 via Caddy layer-4 is optional for strict networks.

## 7. Soundboard
Server sounds (≤ 5 s, ≤ 512 KB, MP3/OGG/WAV → Opus), default sounds, per-user volume, cooldown, "use sounds from other servers" (free, D-044). Sounds are played client-side and sent as an audio stream through the user's mic path into the E2EE pipeline.

## 8. Voice-channel features
Voice channel status text · AFK channel + timeout · user limit · region auto (single region) · video quality mode (auto / 720p) · "Join with camera" prompt · **Activities** launcher (Phase 6) · **Start Watch Party** button (Phase 5) · move members (drag) · disconnect members · "Server Mute" etc. · Join/leave sounds · "Hide names" display option.

## 9. Acceptance criteria
- [ ] 10 users in a voice channel with 5 webcams on Chrome, Firefox, and Safari for 2 hours: no drift, E2EE shield verified, and the SFU never sees plaintext (test: an SFU debug hook asserts it cannot decode Opus payloads).
- [ ] The 6th non-Patron camera is refused at L0. A Patron's camera is accepted at count 5.
- [ ] Push-to-talk honours the release delay, and the PTT key is not sent to the composer.
- [ ] When the egress governor engages, new subscribers receive lower layers and existing high layers are stepped down gradually. A notice appears.
- [ ] Rejoining after a network change (Wi-Fi → 4G) completes in under 3s via ICE restart.
