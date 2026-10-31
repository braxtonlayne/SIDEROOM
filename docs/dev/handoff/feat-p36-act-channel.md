# feat/p36-act-channel handoff (ACT-CH-001)

First slice (SDK API + loopback transport): **6beb30a2**. Follow-up SDK fixes (chunked late-join snapshot, tombstone expiry, sdk.self / sdk.hostId, public sdk.data): **fbbe151a** (cherry-pick both, in order).

`git cherry-pick 6beb30a2` from feat/p36-act-channel. API (packages/embedded-app-sdk/src/data.ts, sdk.ts):

- `new SideRoomSDK(appId, { transport })`; tests use `const hub = new LoopbackHub(hostId); hub.connect(memberId)` (one transport per participant; `hub.manual = true` + `hub.flush()` to test concurrent writes; `hub.relayed` shows the opaque bytes the "SFU" saw).
- `sdk.state.get(key)`, `.all()`, `.version`, `.set({k: v, ...})`, `.delete(...keys)`: last-writer-wins per key (higher version, ties to the larger member id), `host:*` keys writable only by the host (guest `set` throws DataError FORBIDDEN, forged ops are dropped on receipt). A late joiner syncs from the host automatically.
- `sdk.input.send(bytes)` guest to host; `sdk.input.reply(memberId, bytes)` host to one member (the hole-cards path).
- `sdk.on('state', e => e.changes[{key,value,by,version,local}])`, `sdk.on('input', e => {from, data, reply})` (reply=true on a member receiving the host's reply). Returns an unsubscribe function.
- `sdk.isHost`. Max envelope 16 KiB (DataError TOO_LARGE).

Status: see sections below (updated as the work lands).

## Notes for the activity agents (SDK behaviour and limits)

- `sdk.self`, `sdk.hostId`, `sdk.isHost`, and the public `sdk.data` (ActivityData: `version`, `snapshot()`, `compact()`).
- Late join: the host answers a join with the live keys in several `snap` envelopes, each <= 16 KiB, plus a `floor` (document version) so the joiner's next write outranks anything deleted. Big blobs (images) must still be split by the activity into keys of <= ~15 KiB each (one `set` is limited to MAX_DATA_BYTES - 256 = 16 128 bytes of JSON).
- Tombstones live 5 minutes (TOMBSTONE_TTL_MS) and at most 1000; they are not sent to joiners.
- Throughput: the SFU budget is `voice.datachannel_kbps` = 256 kbit/s per member (32 000 bytes/s, burst up to one second or one full message); above it messages are DROPPED, not queued, so an activity must tolerate loss only on that path (state recovers on the next write of the key; a full resync is a leave and join). Practical numbers: about 8 full 4 KB state writes per second per member, or ~100 small (300 byte) strokes/moves per second. Max message 16 KiB. At most 8 instances per member. A 720 KB map image is 48 chunks of 15 KB: ~22 s at the full budget for one sender; send it from the host once, not per joiner (a late joiner pulls it from the host's snapshot, which also uses the host's budget).
- Host handover: NOT implemented. If the host leaves, `host:*` keys become read-only for everyone and `input.send` has no recipient (messages are dropped by the receivers). Activities should end the instance when the host leaves (the gateway's instance ends when the starter leaves the call only if the launcher says so); a successor protocol (host:successor key + the gateway naming the new `started_by`) is an owner/gateway decision, left open.
