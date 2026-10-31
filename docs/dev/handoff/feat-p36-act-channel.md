# feat/p36-act-channel handoff (ACT-CH-001)

First slice (SDK API + loopback transport): **6beb30a2**. Follow-up SDK fixes (chunked late-join snapshot, tombstone expiry, sdk.self / sdk.hostId, public sdk.data): **fbbe151a** (cherry-pick both, in order).

`git cherry-pick 6beb30a2` from feat/p36-act-channel. API (packages/embedded-app-sdk/src/data.ts, sdk.ts):

- `new SideRoomSDK(appId, { transport })`; tests use `const hub = new LoopbackHub(hostId); hub.connect(memberId)` (one transport per participant; `hub.manual = true` + `hub.flush()` to test concurrent writes; `hub.relayed` shows the opaque bytes the "SFU" saw).
- `sdk.state.get(key)`, `.all()`, `.version`, `.set({k: v, ...})`, `.delete(...keys)`: last-writer-wins per key (higher version, ties to the larger member id), `host:*` keys writable only by the host (guest `set` throws DataError FORBIDDEN, forged ops are dropped on receipt). A late joiner syncs from the host automatically.
- `sdk.input.send(bytes)` guest to host; `sdk.input.reply(memberId, bytes)` host to one member (the hole-cards path).
- `sdk.on('state', e => e.changes[{key,value,by,version,local}])`, `sdk.on('input', e => {from, data, reply})` (reply=true on a member receiving the host's reply). Returns an unsubscribe function.
- `sdk.isHost`. Max envelope 16 KiB (DataError TOO_LARGE).


## Notes for the activity agents (SDK behaviour and limits)

- `sdk.self`, `sdk.hostId`, `sdk.isHost`, and the public `sdk.data` (ActivityData: `version`, `snapshot()`, `compact()`).
- Late join: the host answers a join with the live keys in several `snap` envelopes, each <= 16 KiB, plus a `floor` (document version) so the joiner's next write outranks anything deleted. Big blobs (images) must still be split by the activity into keys of <= ~15 KiB each (one `set` is limited to MAX_DATA_BYTES - 256 = 16 128 bytes of JSON).
- Tombstones live 5 minutes (TOMBSTONE_TTL_MS) and at most 1000; they are not sent to joiners.
- Throughput: the SFU budget is `voice.datachannel_kbps` = 256 kbit/s per member (32 000 bytes/s, burst up to one second or one full message); above it messages are DROPPED, not queued, so an activity must tolerate loss only on that path (state recovers on the next write of the key; a full resync is a leave and join). Practical numbers: about 8 full 4 KB state writes per second per member, or ~100 small (300 byte) strokes/moves per second. Max message 16 KiB. At most 8 instances per member. A 720 KB map image is 48 chunks of 15 KB: ~22 s at the full budget for one sender; send it from the host once, not per joiner (a late joiner pulls it from the host's snapshot, which also uses the host's budget).
- Host handover: NOT implemented. If the host leaves, `host:*` keys become read-only for everyone and `input.send` has no recipient (messages are dropped by the receivers). Activities should end the instance when the host leaves (the gateway's instance ends when the starter leaves the call only if the launcher says so); a successor protocol (host:successor key + the gateway naming the new `started_by`) is an owner/gateway decision, left open.

## Done (ACT-CH-001)

- **sr-voice** `datachannel.rs` + `core.rs`: one SCTP data channel per member (label `sr-act`, str0m; works with Chromium's offer), framing `kind|instance|payload` (join/leave/data), relay scoped to (room, instance), never echoed, nothing before the E2EE handshake (sender or receiver), text/malformed frames dropped, per-member token bucket `voice.datachannel_kbps` (256; config in sr-core `voice.datachannel_kbps`, `SR_VOICE__DATACHANNEL_KBPS`, documented in docs/dev/server.md), max 8 instances per member, stats via `SfuCore::datachannel_stats()`. Tests: `crates/sr-voice/tests/datachannel.rs` (6, real str0m clients) and unit tests.
- **sr-e2ee** `data.rs`: `Codec::Data` (id 4, no clear ranges), `DataSender`/`DataReceiver` with a 64-message replay window per (sender, epoch, instance); 8 tests. **sr-e2ee-wasm** + `@sideroom/wasm` `createDataSender`/`createDataReceiver` (interop test in packages/wasm/test/e2ee.test.ts).
- **Capability** `datachannel`: bundle manifest `sideroom.json` (`{"capabilities":["datachannel"]}`), parsed in `bundle.rs` (unknown capability refused by name), migration **1760**, shown on `BundleView` and `InstanceView` (OpenAPI + api-client regenerated; development builds get every capability). Tests: bundle unit tests, `crates/sr-activities/tests/db.rs::an_instance_carries_the_capabilities_of_its_reviewed_bundle`.
- **Web host**: `host.ts` (`dataSend` command, `DATA_MESSAGE` event, joins the instance at the handshake, `ready` carries `user_id`, `host_id`, `capabilities`), `voice/dataPort.ts` (the data channel end, ordered send/receive chains), engine creates the channel on the call's peer connection, E2EE worker seals/opens (`dataSeal`/`dataOpen`). Voice-channel instances only.
- **SDK**: `sdk.state`, `sdk.input`, `sdk.on`, `sdk.self`, `sdk.hostId`, `sdk.isHost`, public `sdk.data`, loopback transport, automatic host-bridge transport when `ready` reports the capability. Tests: data.test.ts, data.sync.test.ts, apps/web `activities/__tests__/dataBridge.test.ts` (two SDKs through two hosts and a relay).
- **sr-sdk** (bots): `voice::DataChannel` (seal/open/join/leave, epochs like `E2eeMedia`), `ConnectOptions::data`, `VoiceSession::send_data`, `VoiceEvent::DataOpen`/`Data`. NOTE: `ConnectOptions` gained a field; struct literals need `..Default::default()`. Test `crates/sr-sdk/tests/voice_data.rs` against the real SFU.
- **Threat model**: row T32 in docs/15-threat-model.md (renumber if another wave also added T32).
- **Live spec**: `e2e/tests/activities-live.spec.ts` now also has both contexts exchange state through the SFU (passed on Chromium, ports 8080/5173, memory object storage, scratch DB; test bundle now has `sideroom.json`, the `data` SDK module and a "Set state" button).

## Not done / open

- Host handover (see above); a mount-file/image-bytes command is out of scope (per coordinator).
- Text-channel instances have no data channel (no SFU there); `datachannel` works in voice-channel instances only.
- Firefox/WebKit not run for the live case (Chromium only, like the rest of the spec).
- No platform kill switch (`platform.activities_datachannel` belongs to OPS-FLG-001).
- VoiceLink (bot reconnect) keeps `ConnectOptions` but the bot must re-send its `join` frames after `Reconnected` (documented in `DataChannel`).

## Tests run

cargo: sr-e2ee (37+1), sr-voice (all, 43 unit + 6 datachannel + e2e/loopback/video/music), sr-activities lib + db (--include-ignored, scratch DB), sr-sdk `voice_data` and clippy `--all-targets`, sr-core, sr-server `immutable_rules` (incl. authz matrix); clippy -D warnings on sr-e2ee, sr-e2ee-wasm, sr-voice, sr-core, sr-activities, sr-sdk, sr-server; cargo fmt --check. JS: embedded-app-sdk (23), wasm e2ee interop (6), apps/web voice+activities+developers (402), `pnpm -r typecheck`, eslint and prettier on touched, build-web + check-entrance-isolation (all ok) + check-bundle-budget (ok), Playwright `activities-live.spec.ts` (chromium). Scratch DB dropped.

## CHANGELOG lines

- Activities can share state and inputs through the SFU: `sdk.state`, `sdk.input` and `sdk.on('state'|'input')` over an SCTP data channel sealed with the call's keys (ACT-CH-001).
- `voice.datachannel_kbps` (default 256) limits what each member can send to an activity instance.
- Activity bundles may declare capabilities in `sideroom.json` (`datachannel`); migration 1760.
- Bots: `voice::DataChannel` and `ConnectOptions::data` in sr-sdk.

## Owner questions

- Host handover when the host leaves (gateway-assigned successor?) and whether instances should end when the host leaves the call.
- Whether `datachannel` should need staff approval beyond the existing bundle review (today it is shown with the bundle and enforced per reviewed bundle).
