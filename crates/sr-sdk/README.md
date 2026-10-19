# sr-sdk

Typed bot SDK for SIDE ROOM: REST client, gateway shards, cache and command framework
(see `src/lib.rs` and `examples/echo_bot.rs`).

Voice: the `voice-e2ee` (MLS key agreement, frame transform) and `voice-transport` (op 4 helpers, SFU WebSocket, str0m WebRTC session, Opus frames) features are on by default (P15-DEV-5): `sr_sdk::voice_transport::VoiceSession` joins the SFU named by `VOICE_SERVER_UPDATE` and refuses to send audio into an end-to-end encrypted call without a frame transform; see `examples/voice_bot.rs` and `tests/voice_transport.rs`. The TypeScript SDK gets voice with its own transport, mirroring the Rust names, not `sr-e2ee-wasm` first.

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
