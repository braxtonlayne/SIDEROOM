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
- **Implementation (VOX-SIG-009).** The gateway counts sends per channel: a camera is `self_video`, a screen share is a stream (`self_stream`, section 4.9); each counts one. Patrons are `exempt` (D-052) and are never refused. A refusal arrives as `SR_VOICE_ERROR` (`video_limit_reached`) with `limit` and `count` for the toast. The viewer cap is the streamer's `stream_viewers`; the next viewer gets `stream_full`. An entitlement change re-reads the limits of everyone in the guild's voice channels (`SR_VOICE_LIMITS_UPDATE {guild_id, video_senders, stream_viewers, exempt}` to each) and never removes a sender or viewer.

## 4. SFU (`sr-voice`) and the signalling contract

Status: **audio** (wave 5, `feat/p5-voice-sfu`) and, in phase 10, **video on the server side**: simulcast camera, screen share with audio, layer selection, keyframe coalescing and the egress governor (sections 4.9 to 4.12), exercised with str0m test clients; no real browser or encoder has been run against it yet, and the client side (VOX-CLI/UI) is not built. End-to-end encryption (section 5) is not started. Rust types for everything here live in `crates/sr-voice-proto`.

### 4.1 Architecture
- str0m-based, sans-IO (ADR-0003). The SFU is **ICE-lite**: it advertises fixed host candidates (UDP range, plus an ICE-TCP passive port when configured) and never gathers. Clients are full ICE agents. DTLS-SRTP, Opus only for now.
- Rooms are keyed by channel id and pinned to one SFU node (`sr_voice_proto::select::select_endpoint`, rendezvous hashing over `voice.endpoints`). A room exists from the first join to the last leave.
- v1 runs the SFU **in the `sr-server` process** (WebSocket route `GET /voice/ws` on the public listener, media on the UDP range). The standalone `sr-voice` binary serves the same routes. The gateway enforces permissions and owns voice state; it tells the SFU about changes through `VoiceRelay` (server mute, disconnect). A relay to SFU nodes in other processes is a documented TODO.

- **Sharded media actors (VOX-SFU-011, phase 13).** The shell runs `voice.shards` actors (default 1 until the AX42 run is recorded, then 0; 0 = half the cores, at most 8, never more than the bound UDP sockets). Each owns an `SfuCore`, a disjoint slice of the UDP sockets (socket `i` belongs to shard `i % n`, so the candidates a shard hands out always lead back to it), the rooms whose channel id hashes to it, and `1/n` of `voice.egress_budget_mbps`. Participant ids are `shard + 1 + k * n`, so the shard of an id is `(id - 1) % n`; the session id starts with the shard as two hex digits, so RESUME finds its way back. What crosses shards: a user joining a channel on another shard evicts their old participant, ICE-TCP (one listener) offers a peer's first packet to every shard until one accepts it, the node-wide `max_participants` is an atomic counter, and the gauges are sums over the shards' snapshots. A room never spans shards.

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
4. Client creates its `RTCPeerConnection` (`iceServers` from step 2), adds its microphone track as a **`sendonly`** audio transceiver (not `sendrecv`: the SFU answers `recvonly`, and a `sendrecv` line would also surface a silent remote track), and sends `SDP_OFFER`. Video m-lines come later, with a `VIDEO publish` message (section 4.9); an undeclared video line is ignored. The server answers `SDP_ANSWER` (ICE-lite, `a=ice-lite`, host candidates included, `a=setup:passive`).
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
| 12 | VIDEO | both | `{t: "publish" \| "subscribe" \| "published" \| "layer", ...}`, section 4.9. |
| 13 | QUALITY | S→C | `{reduced, max_layer, reason}`, section 4.11. |

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
- Forwarding: Opus is forwarded as-is to every subscriber in the room (no mixing, no decoding), preserving the sender's timing. Video frames (VP8 or H.264) are forwarded the same way, per subscriber: layer selection, keyframe coalescing and the **egress governor** are in sections 4.9 to 4.11. Speaker detection from the audio-level header (section 4.3).
- Metrics (Prometheus, node level): `sr_voice_rooms`, `sr_voice_participants`, `sr_voice_egress_bits_per_second`, `sr_voice_ingress_bits_per_second`, `sr_voice_packet_loss_ratio` (gauges, from RTCP receiver reports as seen by the SFU) and `sr_voice_forwarded_packets_total`. Video adds `sr_voice_forwarded_video_frames_total`, `sr_voice_keyframe_requests_total` (after coalescing) and the gauge `sr_voice_governor_severity` (0 = not engaged).
- **Load target (exit gate):** a single AX42-class box forwards 300 Mbps of egress at < 50% CPU. The harness is `cargo run --release -p sr-voice --example loadtest` (section 4.12); **first measurement** (a Windows dev machine shared with seven other jobs, release build, 5 publishers x 1.5 Mbit/s full layer, 40 viewers watching all five, 311 Mbit/s of egress): the media actor was busy 110 % of one core, i.e. **a single actor tops out near 280 Mbit/s per core**. The exit gate (300 Mbit/s at under 50 % CPU of the whole box) therefore needs the TODO(scale) item: shard rooms across several actors (the core has no global state to prevent it), then a measurement on the AX42-class host. Open owner decision: where to spend that effort.
- Deployment: a UDP port range (`voice.udp_port_min..=voice.udp_port_max`, one socket per port, at most 256, each participant is advertised one port so load spreads across sockets) and optionally one ICE-TCP port (`voice.tcp_port`: passive candidates, RFC 4571 framing, no TLS wrapper: it helps with UDP-blocking firewalls, not with HTTP-only proxies, which need TURN/TLS on 443). Open both in the firewall and set `voice.announce_ips` to the node's public addresses.

### 4.9 Video: publishing, subscribing, screen share (VOX-SFU-005, VOX-SFU-006)
A participant publishes up to four tracks, each on its own **`sendonly` m-line** of its offer: microphone, **camera** (VP8 or H.264, simulcast with three layers `q`, `h`, `f` = quarter, half and full resolution), **screen** (one layer) and **screen audio**. The SFU cannot tell the lines apart, so the client declares them (before or after the offer that carries the line):

```json
{"op": 12, "d": {"t": "publish", "mid": "3", "source": "screen", "active": true, "width": 1920, "height": 1080, "fps": 30}}
```
`source` is `camera`, `screen` or `screen_audio` (the first audio line is the microphone without a declaration). `active: false` ends the track; stop the line in the next offer. A source can be on one line only, and a line only carries a source of its kind.

- **The gateway decides who may publish.** `VOICE_SERVER_UPDATE`'s token and later `VoiceCommand::SetFlags` carry `camera` and `screen`: a camera is allowed only after `self_video` passed the sender allowance (section 3) and `STREAM` ("Video") permission, a screen only while the member has a stream (section 4.10). Frames from a track the member may not publish are dropped. The stage audience and the AFK channel publish nothing. The permission can arrive after the publish message (the SFU re-checks when it changes).
- **Who receives what.** Each subscriber gets the microphone of everyone, the **camera** of everyone unless it turned it off, and a **screen only if the gateway listed it as a viewer** (`VoiceCommand::StreamViewers`) **and** it subscribed to it. The viewer list is what enforces the stream's viewer cap; a client cannot subscribe around it.
- **Subscriber messages** (`op 12`, C→S): `{"t": "subscribe", "subscriptions": [{"user_id", "source", "enabled"?, "layer"?: "low"|"mid"|"high", "viewport"?: {"width", "height"}}]}`. `layer` is the largest layer the client wants; `viewport` is the tile size in device pixels and the SFU picks the smallest layer at least that tall (a quarter, half or all of the publisher's declared height). Without a hint a camera is a thumbnail (`low`) and a screen is its only layer. The message replaces the hints for the sources it names.
- **Announcements** (S→C): `{"t": "published", "user_id", "source", "active", "width", "height", "fps"}` when a camera or screen goes live or stops (and for everything already live on join), and `{"t": "layer", "user_id", "source", "layer", "reason"}` when the layer sent to this client changes.
- **Lines the SFU adds.** The server offers one `sendonly` line per (publisher, source) the subscriber should get (section 4.4). The msid stream id is `<user>` for the microphone, `<user>-camera` for the camera and **`<user>-screen` for both the screen and its audio**, so a browser plays them as one synchronized stream group. Use `event.streams[0].id`.
- **Layer selection** (per subscriber and source, `crates/sr-voice/src/layers.rs`). The layer is the largest one within the client's hint cap and the governor cap whose measured bitrate fits 85 % of the subscriber's bandwidth share. Down is immediate; up waits 4 s of continuous headroom. **A switch happens on a keyframe of the new layer**: the old layer keeps flowing until then and the SFU asks the publisher for the keyframe. A subscriber whose layer vanishes (the publisher's own bandwidth dropped it) moves to the best remaining one at once.
- **Bandwidth signal.** By default the loss reported in RTCP receiver reports (`crates/sr-voice/src/loss.rs`: back off to 70 % of the current send rate at 10 % loss, probe up 30 % per 8 calm seconds). With `voice.video_bwe = true` str0m's TWCC estimator is used instead; it is better but makes str0m pad every sending connection to 50 kbit/s, audio-only ones too, so it is off by default.
- Frames are forwarded as delivered by str0m's depacketizer and repacketized per subscriber; the keyframe flag comes from the codec payload descriptor, which end-to-end encryption leaves in the clear. A subscriber that does not support the publisher's codec gets nothing from that publisher (browsers support both VP8 and H.264).

### 4.10 Go Live streams (VOX-SIG-010)
A stream is a screen share inside a voice channel; its key is `guild:{guild}:{channel}:{user}` (`call:{channel}:{user}` in a DM call). Gateway ops (docs/05 section 6.6): 18 `STREAM_CREATE`, 19 `STREAM_DELETE` (the owner stops, a viewer stops watching), 20 `STREAM_WATCH`, 22 `STREAM_SET_PAUSED`. The owner must be in the channel, hold `STREAM`, fit the sender allowance and ask for a quality within their limits (`stream_quality_not_allowed` otherwise). Starting sets `self_stream` and tells the SFU the owner may publish a screen; each watcher is added to the viewer list. Leaving the channel, moving or a moderator disconnect ends the stream and drops the viewer list. The streamer uploads a JPEG preview now and then (`POST /streams/{stream_key}/preview`, at most one per 5 s, 256 KiB); everyone who can see the channel can read it (`GET`, with an ETag).

### 4.11 Egress governor (VOX-SFU-008)
`voice.egress_budget_mbps` (0 = off; suggested 70 % of the measured uplink) is the node's egress budget. The SFU compares its measured egress with it once a second and keeps one **severity** `0..10`: +1 every 5 s while over budget (so layers come down gradually), -1 per 15 s while below 80 % of it. A subscriber's layer cap follows from its **tier** (server level 0 to 3 from its allowance, Patron = 4): tier t is capped to the half layer from severity 2t+1 and to the thumbnail from 2t+2, so **level 0 goes first and Patrons last**. New subscribers start under the cap; existing ones are switched down at the next opportunity. A subscriber that receives video under a cap gets `QUALITY {reduced: true, max_layer, reason: "room_busy"}` once, and `{reduced: false, ...}` when it lifts. The tier is fixed when the member joins the room.

### 4.12 Keyframes, NACK, RTX, and the load test (VOX-SFU-007, VOX-SFU-011)
- **Keyframe requests** (PLI or FIR from a subscriber, a new subscriber, a layer switch) go through one coalescer per (publisher, source, layer): the first goes out at once, the rest are merged into one more request when 500 ms have passed since the last, a keyframe that arrives clears the waiting ones, and a pending FIR upgrades a PLI. 40 requests from 5 subscribers reach the publisher as 1 or 2.
- **NACK and RTX** are str0m's: it keeps the last 1000 video packets per line and answers NACKs from them; RTX is negotiated in the SDP for VP8 and H.264. A subscriber that loses more than the buffer holds escalates to PLI, which ends up in the coalescer.
- **Load test.** `cargo run --release -p sr-voice --example loadtest -- --publishers 5 --viewers 40 --seconds 10 --layer high` runs real str0m clients against one `SfuCore` in memory, times only the core, and prints egress, the share of one core and that share scaled to 300 Mbit/s. It excludes sockets and the kernel, so it is a lower bound. `--shards N` runs N copies of that load on N threads at once (one `SfuCore` each, as the sharded SFU does) and reports the total, the busiest shard and the cores needed for 300 Mbit/s.

## 5. End-to-end encryption (D-053)
Modeled on Discord's DAVE:
1. **Key agreement:** MLS (RFC 9420) via `openmls` (Rust, compiled to WASM for the client). Each voice session is an MLS group. Each client has a per-session signature key bound to its identity key, signed by its account's persistent identity key (stored in IndexedDB, wrapped by a non-exportable WebCrypto key).
2. **Delivery service:** `sr-server` relays MLS key packages, proposals, commits, and welcomes over gateway op 101, and acts as the external sender that orders commits per epoch. It cannot read keys.
3. **Media:** each sender derives a per-epoch media key via the MLS exporter → per-frame AES-128-GCM (12-byte nonce from a frame counter, truncated 8-byte tag) applied in an `RTCRtpScriptTransform` worker (`sr-e2ee` WASM). Codec headers needed for packetization stay unencrypted (as in SFrame/DAVE). Fallback for browsers without ScriptTransform: Chrome's legacy `createEncodedStreams`. If neither is available, the client can't join E2EE calls: it does not declare `sr_e2ee`, the server refuses the join (`e2ee_required`, D-263) and the client shows "This server requires encrypted calls; your browser cannot join."
4. **Membership changes** (join/leave) cause a new epoch. Senders re-key. Frames under the old key are accepted during a short transition window.
5. **Verification:** per-participant "privacy code" (fingerprint of the MLS epoch authenticator + identity key), shown in the shield popover; optional out-of-band compare.
6. **Bots/apps in voice** must participate in MLS through the SDK to send or receive audio (as with DAVE). A bot declares `sr_e2ee: true` in its op 4 like any client, and sends op 101 `hello` within 10 seconds of `VOICE_SERVER_UPDATE` or is removed (D-263); `sr_sdk::voice::E2eeCall::join_state` builds the join. The e2e observer client (`sr.e2e.noVoiceE2ee`, e2e builds only) keeps declaring the capability and says hello, but runs no MLS and only receives.
7. **Not E2EE:** watch-party HLS streams (library content), and text chat in voice.

**Status (P14).** Built and tested: MLS on `openmls` (`crates/sr-e2ee/src/mls.rs`, ciphersuite `MLS_128_DHKEMP256_AES128GCM_SHA256_P256`, D-249; session signature keys certified by the P-256 identity key inside the credential; adds and removes checked against the voice roster), the frame transform (`frame.rs`, `codec.rs`), the 10 s key grace window, D-250 privacy codes, the op 101 relay with `hello`/`call` setup, per-epoch commit ordering, `ready`/`execute` and the server as external sender of Remove proposals (`crates/sr-gateway/src/voice_e2ee.rs`), the WASM build, and the browser side (MLS session, media worker attached to every sender and receiver, identity keys, shield). `voice.e2ee` and `VITE_SR_VOICE_E2EE` are still off: the media path has not run in a real browser yet. Item 5 follows D-250 (a stable pairwise code per participant plus a per-call code) rather than the per-epoch participant code above. Details: `docs/dev/handoff/feat-p14-e2ee.md`.

**Wire details as built (D-249, J-21).** Ciphersuite `MLS_128_DHKEMP256_AES128GCM_SHA256_P256`; identity key ECDSA P-256 (WebCrypto, non-extractable) certifying a per-session MLS signature key. *Frame format v1:* the body keeps its clear codec ranges, then an 8-byte truncated GCM tag, a random 4-byte encryptor instance id, a ULEB128 frame counter, version byte `1`, the trailer length and the magic `SR`. One encryptor serves all of a sender's tracks, the instance id is mixed into the key derivation (a restarted encryptor never reuses a nonce), and the receiver checks the truncated tag by recomputing the full GCM tag over the ciphertext (constant-time comparison of the first 8 bytes) before releasing any plaintext. *Setup:* after a commit the members answer `ready` and the sender switches on `execute` (or after a 5-second fallback). *Relay state* (commit ordering per epoch, the external-sender key) lives per gateway node in memory, like voice states: with several nodes a call's voice states and its relay state must sit on the same node (the voice-state caveat). *Removals:* the server signs Remove proposals as external sender; member-committed removals were the interim design (until the external sender landed) and remain only a fallback. *Default:* E2EE is on by default once VOX-E2E-001 is verified in a real-browser run; `voice.e2ee` turns it off (D-249).

## 6. TURN
coturn with ephemeral credentials from `GET /api/v1/sr/voice/turn` (HMAC, TTL 1h). ICE servers are delivered in `VOICE_SERVER_UPDATE`. TURN/TLS on 443 via Caddy layer-4 is optional for strict networks.

## 7. Soundboard
Server sounds (≤ 5 s, ≤ 512 KB, MP3/OGG/WAV, stored as uploaded and decoded by the browser, D-201), default sounds, per-user volume, cooldown, "use sounds from other servers" (free, D-044). Sounds are played client-side and sent as an audio stream through the user's mic path into the E2EE pipeline.

**Implementation (VOX-SB-001).** Server side the sound is only a file and an announcement.
- Guild sounds (48 per server for everyone): `GET|POST /guilds/{id}/soundboard-sounds`, `GET|PATCH|DELETE .../{sound_id}`, `GET .../{sound_id}/audio` (the file as uploaded, members only). Upload is a data URI (`name`, `sound`, `volume` 0 to 1, `emoji_id` or `emoji_name`); the server sniffs MP3, Ogg (Opus or Vorbis) or WAV, measures the length (at most 5 s) and refuses over 512 KiB. Needs Create or Manage Expressions; the uploader may change their own with Create. The "-> Opus" transcode is not done on the server (browsers decode all three with `decodeAudioData`). Changes are announced as `GUILD_SOUNDBOARD_SOUNDS_UPDATE {guild_id, soundboard_sounds}`; gateway op 31 `REQUEST_SOUNDBOARD_SOUNDS {guild_ids}` answers `SOUNDBOARD_SOUNDS` per guild.
- Default sounds: eight original tones synthesized by the server (ids 1 to 8, `GET /soundboard-default-sounds`, `.../{id}/audio` as WAV), so there is nothing to license.
- Playing: `POST /channels/{id}/send-soundboard-sound {sound_id, source_guild_id?}` needs `USE_SOUNDBOARD`, `SPEAK` and `CONNECT` in the channel, `USE_EXTERNAL_SOUNDS` for another server's sound (that server must be one the member is in; free, D-044), the member connected to that channel and audible (not muted, deafened, in the stage audience or the AFK channel), and one play per 4 s. It dispatches `VOICE_CHANNEL_EFFECT_SEND {channel_id, guild_id, user_id, emoji, sound_id, sound_volume}` to everyone who can see the channel; the playing client decodes the file and mixes it into its microphone stream.

## 8. Voice-channel features
**Implementation notes (phase 10).** *Age gate (VOX-SIG-003):* an age-restricted (NSFW) voice channel refuses members who have not answered "Are you 18 or older?" (D-032) with `SR_VOICE_ERROR` `age_confirmation_required`; after `POST /sr/age-gate` the join is retried. *AFK:* a member silent for the guild's `afk_timeout` (spoken audio as heard by the SFU, or any state change, resets it) is moved to the AFK channel with a new token; there they listen only. A zero timeout or no AFK channel turns it off; the check runs every 15 s. *Stage channels (VOX-STG-001):* section 8a. *DM calls (VOX-SIG-008):* section 8b.

Voice channel status text · AFK channel + timeout · user limit · region auto (single region) · video quality mode (auto / 720p) · "Join with camera" prompt · **Activities** launcher (Phase 6) · **Start Watch Party** button (Phase 5) · move members (drag) · disconnect members · "Server Mute" etc. · Join/leave sounds · "Hide names" display option.

**Video quality mode (J-11).** `channels.video_quality_mode` is 1 (auto, the default) or 2 (full), set with `PATCH /channels/{id}` by `MANAGE_CHANNELS` on voice and stage channels (the channel editor's "Full video quality" switch). The gateway puts it in the join token (`video_quality`) and the SFU keeps it as `Flags::video_full`: in a full-quality channel the egress governor never reduces the simulcast layer a member is handed because the node is busy (camera layers stay up to the 720p tier whatever the room load); in auto channels the governor's tier caps apply as before. The mode is fixed when a member joins; a change applies from the next join. `rtc_region` is returned as `null` and refused on write (400): the installation has one voice region.

### 8a. Stage channels (VOX-STG-001)
A stage instance is the live event in a stage channel (`{id, guild_id, channel_id, topic, privacy_level: 2, discoverable_disabled, guild_scheduled_event_id}`; the id is the channel id). Moderators (all of Manage Channels, Mute Members and Move Members in the channel) start it (`POST /stage-instances {channel_id, topic 1-120 chars}`), change it (`PATCH`) and end it (`DELETE`); `GET /stage-instances/{channel_id}` and `GET /guilds/{id}/stage-instances` read. It is announced as `STAGE_INSTANCE_CREATE/UPDATE/DELETE` to everyone who can see the channel and ends by itself when the last member leaves or the channel is deleted. Instances live in the gateway's memory like voice states.
- Everyone who joins a stage is **suppressed** (audience: the SFU drops their audio) except moderators. `suppress` and `request_to_speak_timestamp` are on the voice state.
- **Request to speak:** `PATCH /guilds/{id}/voice-states/ {channel_id, request_to_speak_timestamp: <ISO>}` (needs `REQUEST_TO_SPEAK`, a running stage and a seat in the audience; the server stamps its own time; `null` lowers the hand). **Invite to speak:** a moderator `PATCH /guilds/{id}/voice-states/{user} {channel_id, suppress: false}` (needs Mute Members); `suppress: true` sends a speaker back. A speaker may step down with `suppress: true` on ``; becoming a speaker on their own needs Mute Members. Ending the stage sends everyone but moderators back to the audience.

### 8a2. Voice channel status (VOX-UI-015)
A short line under a voice or stage channel's name (Discord's "voice channel status"): `PUT /channels/{id}/voice-status {status: string | null}` sets it (empty or `null` clears; at most 500 characters), announced as `VOICE_CHANNEL_STATUS_UPDATE {id, guild_id, status}` to everyone who can see the channel, and `GUILD_CREATE` carries it as `status` on the channel objects. Needs the new permission `SET_VOICE_CHANNEL_STATUS` (bit 48, Discord's number; on for @everyone in new servers and, by migration 0860, in existing ones) and a seat in the channel, or `MANAGE_CHANNELS` (from anywhere). It lives in the gateway's memory next to the voice states and ends when the last member leaves or the channel is deleted. A status can only be set while somebody is in the channel.

### 8b. DM and group DM calls (VOX-SIG-008)
Join with op 4 and no `guild_id`: `{channel_id: <dm>, self_mute, self_deaf, self_video}`; only recipients may. The first member to connect starts the call and **rings** everyone else for 45 s: `CALL_CREATE {channel_id, message_id, region, ringing, voice_states}` to all members. `CALL_UPDATE {channel_id, message_id, region, ringing}` follows every change (someone answered, declined, was rung again, the ring ran out). `POST /channels/{id}/call/ring {recipients?}` rings members who are not in the call (the caller must be in it); `POST /channels/{id}/call/stop-ringing {recipients?}` declines (no body: yourself) or, from inside the call, stops ringing others; `GET /channels/{id}/call` returns the `CALL_CREATE` shape. When the last member leaves: `CALL_DELETE {channel_id}` and a **`CALL` system message** (type 3, author the caller) with `call: {participants, ended_timestamp}`; members who never joined missed it. DM calls use the free allowance (5 senders, 720p30) and all voice permissions; the voice token has no `guild_id`.

## 9. Acceptance criteria
- [ ] 10 users in a voice channel with 5 webcams on Chrome, Firefox, and Safari for 2 hours: no drift, E2EE shield verified, and the SFU never sees plaintext (test: an SFU debug hook asserts it cannot decode Opus payloads).
- [ ] The 6th non-Patron camera is refused at L0. A Patron's camera is accepted at count 5.
- [ ] Push-to-talk honours the release delay, and the PTT key is not sent to the composer.
- [ ] When the egress governor engages, new subscribers receive lower layers and existing high layers are stepped down gradually. A notice appears.
- [ ] Rejoining after a network change (Wi-Fi → 4G) completes in under 3s via ICE restart.
