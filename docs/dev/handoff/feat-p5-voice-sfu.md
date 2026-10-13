# Handoff: feat/p5-voice-sfu (audio foundation COMPLETE, video and the rest deferred)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p5-voice-sfu, branch feat/p5-voice-sfu (base integration/phase4). Local commits only, nothing pushed.

## Read this first (voice client agent)
The signalling contract is `docs/specs/lounge-voice-video.md` section 4 and `docs/05-api-and-gateway.md` section 6.5. It was committed first (b3a267a). **These points changed since that commit**; the doc is current, but check your code against them:
1. **Glare roles flipped.** The client is the *impolite* peer, the server is *polite*. A client with its own offer outstanding **ignores** a server `SDP_OFFER` (no rollback); the server abandons its offer, answers the client's, then re-offers. The first commit said the opposite.
2. The microphone transceiver must be **`sendonly`** (the SFU then answers `recvonly`). With `sendrecv` the browser also gets a silent remote track.
3. Handle WebSocket messages **strictly in order** (await each `setRemoteDescription`/`setLocalDescription` before the next); the server's first offer follows its answer immediately.
4. New close code **4008** (rate limited, more than 200 messages per 10 s).
5. To rejoin the channel you are already in, send op 4 with `channel_id: null` and then the channel again. A repeated op 4 for the same channel only updates mute/deafen and does not issue a new `VOICE_SERVER_UPDATE`.
6. The voice WebSocket is `GET /voice/ws` on the public listener (`endpoint` in `VOICE_SERVER_UPDATE` is a full `ws(s)://` URL). The Caddy dev proxy has the route (`deploy/compose/caddy/Caddyfile`); **the Vite dev proxy in apps/web/vite.config.ts needs `'/voice': { target: 'ws://localhost:8080', ws: true }`**, I did not touch apps/web.
7. Refused joins arrive as the gateway dispatch `SR_VOICE_ERROR {guild_id, channel_id, code, message}` (codes in docs/05 6.5). Self mute also stops forwarding on the SFU, so a client cannot leak audio while "muted".
8. Push-to-talk: when the channel denies `USE_VAD` (`VOICE_SERVER_UPDATE` token claim `ptt_required`, derivable client side from permissions), the SFU forwards audio only while the client's last voice-WS `SPEAKING` value is non-zero (300 ms grace). Send `{speaking: 1}` on key down and `{speaking: 0}` after the release delay.
9. Enable locally with `SR_VOICE__ENABLED=true` (+ `SR_VOICE__TURN__SECRET=dev-turn-shared-secret`, `SR_VOICE__TURN__URLS=[...]` for the compose coturn). Defaults: UDP 50000-50099, announce `127.0.0.1`.

## What was built
- `sr-voice-proto`: voice WebSocket messages and close codes, HMAC join tokens (60 s), coturn REST credentials (HMAC-SHA1, 1 h), rendezvous-hash node selection, `VoiceRelay`/`VoiceCommand` (gateway to SFU control).
- `sr-voice`: sans-IO SFU core on str0m 0.24.1 with the `rust-crypto` backend (pure Rust; `cargo tree -i openssl` finds nothing, `cargo deny check` passes). ICE-lite, DTLS-SRTP, rooms per channel, one `sendonly` line per remote speaker (msid stream id = user id), Opus forwarded as is, speaking detection from the audio-level header, server-side mute/deafen/PTT enforcement, server-initiated renegotiation, ICE restart via client offer. Tokio shell: UDP port range (up to 256 sockets), passive ICE-TCP (RFC 4571), voice WebSocket (IDENTIFY, RESUME with a 20 s window, heartbeat, rate limit), drain, metrics gauges, standalone `sr-voice` binary (env config). The SFU also tells the gateway when it loses a participant.
- Gateway (`sr-gateway/src/voice.rs`): op 4, `VOICE_STATE_UPDATE` dispatch filtered by channel visibility, `voice_states` in `GUILD_CREATE`, `VOICE_SERVER_UPDATE` with endpoint, token and ICE servers, `SR_VOICE_ERROR`, intent `GUILD_VOICE_STATES`, join checks as pure functions, `moderate_voice`, cleanup on session loss (20 s grace), member leave, channel and guild delete, permission changes (re-validated), SFU-lost reports. `GuildDirectory` gained `voice_channel`, `channel_permissions`, `member_payload`.
- REST (`sr-api/src/routes/voice.rs`): `GET /sr/voice/turn`, `PATCH /guilds/{id}/voice-states/{user}` (`@me` allowed; mute, deaf, move, disconnect). New error code 40032. Audit entries (`MEMBER_UPDATE` mute/deaf changes, `MEMBER_MOVE`, `MEMBER_DISCONNECT`) with the target as `target_id` and the channel in `changes`.
- Config `[voice]` / `[voice.turn]` in `sr-core`, wiring in `sr-server/src/voice.rs` and `app.rs` (embedded SFU, routes mounted beside the gateway, drain on shutdown, SFU events to the gateway). `docs/dev/server.md` has the key table. Metrics on `/internal/metrics`: `sr_voice_rooms`, `_participants`, `_egress_bits_per_second`, `_ingress_bits_per_second`, `_packet_loss_ratio`, `_forwarded_packets_total` (checked on a running server).
- OpenAPI and the TypeScript client regenerated (`packages/api-client`). `node_modules` is not installed in this worktree, so I ran the already installed `openapi-typescript` from the main clone against this worktree's files; the diff is additions only.

## Tests
New this wave: sr-voice-proto 12; sr-voice 4 unit + 12 in-memory loopback (`tests/loopback.rs`: two or three real str0m clients through the core, audio reaches B and never loops back, bit-exact payloads, late joiner, speaking events, priority bit, mute, deafen, PTT, leave, replace, limits, garbage input) + 9 socket e2e (`tests/e2e.rs`: real axum WebSocket, UDP and ICE-TCP on loopback, resume, bad tokens, mute/disconnect commands, replace, drain, lost participant, pre-join mute); sr-gateway 26 (8 pure-function tests, 18 protocol tests); sr-api 5; sr-core 1; sr-server 2 unit + 1 Postgres test (`voice_pg_tests.rs`, `#[ignore]`, passed against a local Postgres 17 scratch database with `--test-threads=1`). Whole workspace: 1161 passed, 0 failed, 107 ignored (the ignored ones need a database). A mutation check (removing the `may_send` gate) makes the mute and PTT tests fail.
Gates run: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace`, `cargo deny check`. The scratch database `sr_voice_sfu_test` was created for this work and dropped at the end. No migrations were added (voice state is in memory).

## Decisions
- **Voice state is in memory per node**, keyed by user (one voice connection per user, like Discord). Lost on restart; server mute/deafen reset when the user leaves voice. Persisting them needs a table (suggest a migration in 0270-0289 if the owner wants it); nothing blocks that.
- The SFU runs **in the sr-server process** by default; the gateway reaches it through the `VoiceRelay` trait (direct call). A standalone `sr-voice` node serves the same WebSocket but cannot receive mute/disconnect commands yet (TODO: HTTP relay). Tokens carry the flags at mint time and the SFU remembers a mute that arrives before the participant connects (60 s).
- Self mute is enforced on the SFU as well as shown to others; deafen stops forwarding to that user.
- Moving a user does not check the target's own permissions in the destination (Discord behaviour); they get media flags from their permissions there, so they can listen but only speak with `SPEAK`. Moderators need `MOVE_MEMBERS` on both channels and `CONNECT` on the destination.
- Voice permission changes (channel, role, member updates) re-validate everyone in the guild's voice channels; people who lost VIEW/CONNECT are removed.
- ICE-TCP is plain TCP passive candidates (no SSL-TCP hello). HTTP-only networks still need TURN/TLS on 443 (Caddy layer-4, optional per spec).
- `GET /sr/voice/turn` answers 403 `SR_FEATURE_DISABLED` when TURN is not configured (the first doc draft said 503).

## Not done
- Video, simulcast, screen share, PLI/FIR coalescing, TWCC bandwidth estimation, egress governor (hooks are marked `TODO(egress-governor)`, `TODO(video)` in `sr-voice/src/core.rs`), E2EE/MLS (op 101), stage channels, soundboard: VOX-SFU-005..008, VOX-E2E-*, VOX-STG-*, VOX-SB-*.
- VOX-SIG-003 partly (no age gate, no AFK move/timeout); VOX-SIG-008 (DM calls), 009 (sender allowance), 010 (stream keys).
- VOX-SFU-011 load harness and the 300 Mbps measurement; no CPU gauge.
- Bitrate cap per channel is not enforced on the SFU (documented as `TODO(bitrate-cap)`).
- Discord-compat `PATCH /guilds/{id}/members/{id}` with `mute`/`deaf`/`channel_id`, and `PATCH /guilds/{id}/voice-states/@me` for stage.
- Multi-node SFU: selection function exists, but the control relay and per-node metrics aggregation do not.
- Not verified at runtime: a real browser (Chrome/Firefox/Safari) against the SFU, and the coturn compose entry (no Docker on this machine). I reviewed `deploy/compose/coturn/turnserver.conf` against the REST scheme, added quotas and commented production hardening, but have not started coturn or exercised the credentials against it.

## Files worth knowing
`crates/sr-voice/src/core.rs` (engine), `server.rs` (actor, sockets), `ws.rs` (WebSocket), `tests/`; `crates/sr-gateway/src/voice.rs`, `voice_tests.rs`; `crates/sr-api/src/routes/voice.rs`; `crates/sr-server/src/voice.rs`, `voice_pg_tests.rs`; `crates/sr-voice-proto/src/*`.
