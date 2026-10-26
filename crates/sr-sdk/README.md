# sr-sdk

Typed bot SDK for SIDE ROOM: REST client, gateway shards, cache and command framework
(see `src/lib.rs` and `examples/echo_bot.rs`).

Voice: the `voice-e2ee` (MLS key agreement, frame transform) and `voice-transport` (op 4 helpers, SFU WebSocket, str0m WebRTC session, Opus frames) features are on by default (P15-DEV-5): `sr_sdk::voice_transport::VoiceSession` joins the SFU named by `VOICE_SERVER_UPDATE` and refuses to send audio into an end-to-end encrypted call without a frame transform; see `examples/voice_bot.rs` and `tests/voice_transport.rs`. The TypeScript SDK gets voice with its own transport, mirroring the Rust names, not `sr-e2ee-wasm` first.

Opting out of voice (D-268). Bots that do not join voice calls can drop openmls, str0m and p256 from their build:

```toml
sr-sdk = { path = "../SIDEROOM/crates/sr-sdk", default-features = false }
```

Add back only what you need with `features = ["voice-transport"]` (SFU session, no end-to-end encryption; it then refuses encrypted calls) or `features = ["voice-e2ee"]` (key agreement and frame transform without a transport). CI builds and tests `--no-default-features` and both single-feature sets.

Opus (DEV-FU-002, D-377, D-378). The `voice-opus` feature (on by default) adds `sr_sdk::audio`: `OpusEncoder` (exactly one 20 ms frame of 48 kHz PCM, 960 samples per channel, per `encode_frame`; any other length is an error), `OpusDecoder` (`decode_frame(packet, fec)`, so a bot can hear as well as speak) and `audio_level_dbov(pcm)`, the real level for `send_opus(frame, level)`. `examples/voice_bot.rs` sends an encoded 440 Hz tone. `send_opus` still takes any pre-encoded packet, so a bot that already has Opus (a file, another SDK) can switch the feature off. It builds libopus 1.6.1 from the vendored source of `opusic-sys` with **cmake**, the tool `aws-lc-sys` already asks for, so `cmake` must be on `PATH` (or named in the `CMAKE` environment variable; on a Windows machine with only the Visual Studio Build Tools, point it at `...\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe`). `default-features = false` builds no C code. The sound lives only in the SDK: the server never links libopus (D-201).

Address rule (D-234). The SDK runs in an operator's bot, not in the server, so it uses plain
`reqwest` instead of `sr_net::SafeClient` (an exception recorded in `deny.toml`). It contacts only:

- the base URL the operator configured;
- the gateway URL from `GET /gateway/bot` and the `resume_gateway_url` from READY, and only when
  their host equals the base host or a host added with `ClientBuilder::allow_gateway_host`. Any other
  host is logged at `warn` and the configured gateway URL (`wss://<base host>/gateway`) is used.

The SDK never fetches an address found in message content (attachments, embeds, avatars): if your
bot needs one, fetch it yourself with your own restrictions.

Gateway operations: `Client::gateway()` returns a `GatewayHandle` with
`request_guild_members` (op 8, chunks as a typed stream) and `update_presence` (op 3).
