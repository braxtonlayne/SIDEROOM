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

## 4. SFU (`sr-voice`) and the signalling contract

Status: **audio is implemented** (wave 5, `feat/p5-voice-sfu`). Video, simulcast, screen share, E2EE and stage channels are later work; the places they plug in are marked `reserved` below. Rust types for everything here live in `crates/sr-voice-proto`.

### 4.1 Architecture
- str0m-based, sans-IO (ADR-0003). The SFU is **ICE-lite**: it advertises fixed host candidates (UDP range, plus an ICE-TCP passive port when configured) and never gathers. Clients are full ICE agents. DTLS-SRTP, Opus only for now.
- Rooms are keyed by channel id and pinned to one SFU node (`sr_voice_proto::select::select_endpoint`, rendezvous hashing over `voice.endpoints`). A room exists from the first join to the last leave.
- v1 runs the SFU **in the `sr-server` process** (WebSocket route `GET /voice/ws` on the public listener, media on the UDP range). The standalone `sr-voice` binary serves the same routes. The gateway enforces permissions and owns voice state; it tells the SFU about changes through `VoiceRelay` (server mute, disconnect). A relay to SFU nodes in other processes is a documented TODO.

### 4.2 Join flow
1. Client sends gateway op 4 `VOICE_STATE_UPDATE` (docs/05 section 6.5) with `channel_id`.
2. The gateway checks permissions and dispatches `VOICE_STATE_UPDATE` (to guild members who can see the channel) and, to the joining session only, `VOICE_SERVER_UPDATE`:
   ```json
   {"guild_id": "1", "channel_id": "2",
    "endpoint": "wss://chat.example.org/voice/ws",
    "token": "v1.<claims>.<mac>",
    "ice_servers": [{"urls": ["stun:turn.example.org:3478"]},
                    {"urls": ["turn:turn.example.org:3478?transport=udp"],
                     "username": "1760000000:42", "credential": "<base64>"}]}
   ```
   `endpoint` is a full `ws(s)://` URL (Discord sends a bare host; ours may carry a path behind Caddy). `token` is valid for 60 s and is single purpose (join this channel as this user). `ice_servers` is a ready `RTCConfiguration.iceServers` list; TURN credentials are ephemeral (section 6) and the list is empty when no STUN/TURN is configured.
3. Client opens the WebSocket at `endpoint` and sends `IDENTIFY`. The server answers `READY`.
4. Client creates its `RTCPeerConnection` (`iceServers` from step 2), adds its microphone track as a **`sendonly`** audio transceiver (not `sendrecv`: the SFU answers `recvonly`, and a `sendrecv` line would also surface a silent remote track), and sends `SDP_OFFER`. Audio only: do not add video m-lines yet (they would be accepted and ignored). The server answers `SDP_ANSWER` (ICE-lite, `a=ice-lite`, host candidates included, `a=setup:passive`).
5. ICE and DTLS complete; Opus RTP flows. As other participants exist or arrive, the server sends `SDP_OFFER`s that add one `sendonly` audio m-line per remote participant (see 4.4); the client answers each with `SDP_ANSWER`.

### 4.3 Wire format
Text WebSocket frames, one JSON object: `{"op": <int>, "d": <object>}`. Ids are decimal strings. One connection per voice session. Close codes are in 4.7.

| Op | Name | Dir | `d` |
|---|---|---|---|
| 0 | IDENTIFY | C→S | `{token}`. Must be the first message, within 10 s. |
| 1 | READY | S→C | `{session_id, resume_token, heartbeat_interval, user_id, channel_id, guild_id?, participants: [{user_id, speaking, muted, deafened}]}` |
| 2 | HEARTBEAT | C→S | `{t}` (any JSON value; echoed). |
| 3 | HEARTBEAT_ACK | S→C | `{t}` |
| 4 | SDP_OFFER | both | `{sdp}` |
| 5 | SDP_ANSWER | both | `{sdp}` |
| 6 | ICE_CANDIDATE | C→S | `{candidate: string \| null, sdp_mid, sdp_mline_index}`; `null` = end of candidates. Optional: candidates already inside the offer need no trickle. S→C is reserved. |
| 7 | SPEAKING | both | C→S `{speaking}`; S→C `{user_id, speaking, audio_level?}` |
| 8 | RESUME | C→S | `{session_id, resume_token}` |
| 9 | RESUMED | S→C | Same shape as READY (current participants). |
| 10 | CLIENT_CONNECT | S→C | `{user_id}` |
| 11 | CLIENT_DISCONNECT | S→C | `{user_id}` |
| 12 | VIDEO | | reserved (publish/subscribe video, simulcast layers, screen share) |
| 13 | QUALITY | | reserved (egress governor notices, doc 08 section 3) |

- **Heartbeat.** Send `HEARTBEAT` every `heartbeat_interval` ms (13 750). The server closes with 4009 after 2.5 intervals of silence. Heartbeats are about the WebSocket only; media liveness is ICE/DTLS.
- **SPEAKING.** `speaking` is a bitmask: `1` microphone, `2` soundshare (reserved), `4` priority speaker. Client→server: the client reports its own state. It **must** send `1` while push-to-talk is held and `0` when released (after its release delay), because when the channel denies `USE_VAD` the SFU forwards a user's audio only while the last reported value is non-zero (300 ms grace). With voice activity the client may send it or not; the server detects speech itself from the RTP audio-level header extension (`urn:ietf:params:rtp-hdrext:ssrc-audio-level`, works under E2EE). Server→client: sent on transitions only, for **every other** participant (and not echoed to the speaker). `speaking` is `0` when they stop. `4` is OR-ed in for members with `PRIORITY_SPEAKER`; clients duck other voices while a priority bit is set. `audio_level` is dBov (-127 quiet to 0 loud) when known.
- **CLIENT_CONNECT / CLIENT_DISCONNECT.** Sent to the others when a participant's voice session starts or ends. The gateway's `VOICE_STATE_UPDATE` remains the source of truth for who is in the channel; these tell the client which remote audio lines to expect.
- **Muting is enforced on the server.** A participant without `SPEAK`, or server muted, has their audio dropped by the SFU. A deafened participant (server or self) receives no audio. Self mute is a client concern (the client stops sending or sends silence); report it on the gateway so others see the icon.

### 4.4 SDP negotiation (who offers, glare, mapping)
- The client sends the **first** offer. After that either side may offer.
- The **server offers** whenever the room's membership changes (a remote participant starts or stops, or on join to add the existing participants): it adds or stops one `sendonly` audio m-line per remote participant. The m-line's `a=msid` stream id is the participant's **user id** (decimal). The client maps tracks to users with `event.streams[0].id`. Do not rely on mid ordering; mids are recycled.
- **Glare.** The **client is the impolite peer, the server is polite.** The server never sends an offer while its own previous offer is unanswered. If a client `SDP_OFFER` arrives while a server offer is pending, the server abandons its offer, answers the client's, and sends a fresh offer afterwards. So: **a client that has its own offer outstanding ignores any server `SDP_OFFER` it receives** (no rollback needed) and just waits for the answer to its own; the server's replacement offer follows. A client answer to an abandoned offer is ignored.
- **ICE restart / network change.** The client calls `restartIce()` and sends a new `SDP_OFFER` (fresh ufrag/pwd). The server accepts it on the existing WebSocket and the existing participant. If the WebSocket is also gone, reconnect it first and use `RESUME`.
- **Ordering.** WebSocket messages must be handled strictly in order: finish `setRemoteDescription`/`setLocalDescription` for one SDP message before starting the next (the server's first offer follows its answer immediately).
- Codec: Opus (payload type from the client's offer), `usedtx=1;useinbandfec=1;stereo=0`. DTX is allowed; the audio-level header extension must be negotiated (browsers do by default).
- Bitrate: advertise the channel's bitrate (kbps) through `RTCRtpSender.setParameters({encodings: [{maxBitrate}]})`; the server does not transcode and trusts but does not enforce it in v1. TODO(bitrate-cap): enforce `min(channel bitrate, server level cap)` in the SFU.

### 4.5 Resume
- If only the WebSocket drops, media keeps flowing. The SFU keeps the participant for **20 s** after a socket loss. Reconnect to the same `endpoint`, send `RESUME` (not `IDENTIFY`), receive `RESUMED` with the current participants, and carry on (answer any new server offer). `RESUME` needs the `resume_token` from `READY`, not a new join token.
- After 20 s, or after any close with 4006/4014, the session is gone: rejoin through the gateway to get a fresh token. To rejoin the channel you are already in, send op 4 with `channel_id: null` and then again with the channel (a repeated op 4 with the same channel only updates mute/deafen and does not issue a new `VOICE_SERVER_UPDATE`).
- Media failure: if ICE stays disconnected for 15 s the SFU drops the participant and closes the WebSocket with 4006. Clients that notice a dead connection earlier should `restartIce()` first (4.4).
- If the SFU loses a participant on its own (WebSocket not resumed within 20 s, or ICE dead for 15 s) it reports that to the gateway, which clears the voice state and dispatches the leave, so no ghost stays in the channel.
- If the gateway session is lost (it detached and did not resume within 20 s), the gateway removes the voice state and tells the SFU to drop the participant.
- A second `IDENTIFY` for the same user closes the older connection with 4016.

### 4.6 What the gateway tells the client
`VOICE_STATE_UPDATE` (gateway op 4 and dispatch), `VOICE_SERVER_UPDATE`, `SR_VOICE_ERROR` and the REST routes are specified in docs/05 section 6.5. A voice state change from a moderator (server mute, deafen, move, disconnect) arrives as a `VOICE_STATE_UPDATE` for the affected user; for a disconnect or move, the voice WebSocket is closed with 4014 (a move is followed by a new `VOICE_SERVER_UPDATE`). Clients must close their `RTCPeerConnection` on 4014.

### 4.7 Close codes
`4001` unknown opcode, `4002` decode error, `4003` not authenticated, `4004` authentication failed (bad or expired token: rejoin via gateway), `4005` already authenticated, `4008` rate limited (more than 200 messages in 10 s), `4006` session invalid (RESUME failed: rejoin), `4009` heartbeat timeout (reconnect and `RESUME`), `4014` removed from the call (do not reconnect), `4015` SFU restarting (reconnect and `RESUME`, else rejoin), `4016` replaced by a newer connection of the same user, `4017` SFU full.

### 4.8 Forwarding, metrics, targets
- Forwarding: Opus is forwarded as-is to every subscriber in the room (no mixing, no decoding), preserving the sender's timing. Later: simulcast layer selection per subscriber from viewport, bandwidth estimate (TWCC/REMB) and the **egress governor** (doc 08 section 3); keyframe request (PLI/FIR) coalescing. Speaker detection from the audio-level header (section 4.3).
- Metrics (Prometheus, node level): `sr_voice_rooms`, `sr_voice_participants`, `sr_voice_egress_bits_per_second`, `sr_voice_ingress_bits_per_second`, `sr_voice_packet_loss_ratio` (gauges, from RTCP receiver reports as seen by the SFU) and `sr_voice_forwarded_packets_total`.
- **Load target (exit gate):** a single AX42-class box forwards 300 Mbps of egress at < 50% CPU. Not yet measured (VOX-SFU-011).
- Deployment: a UDP port range (`voice.udp_port_min..=voice.udp_port_max`, one socket per port, at most 256, each participant is advertised one port so load spreads across sockets) and optionally one ICE-TCP port (`voice.tcp_port`: passive candidates, RFC 4571 framing, no TLS wrapper: it helps with UDP-blocking firewalls, not with HTTP-only proxies, which need TURN/TLS on 443). Open both in the firewall and set `voice.announce_ips` to the node's public addresses.

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
