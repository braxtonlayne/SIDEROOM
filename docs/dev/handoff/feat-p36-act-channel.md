# feat/p36-act-channel handoff (ACT-CH-001)

First slice (SDK API + loopback transport), cherry-pickable: **6beb30a2**

`git cherry-pick 6beb30a2` from feat/p36-act-channel. API (packages/embedded-app-sdk/src/data.ts, sdk.ts):

- `new SideRoomSDK(appId, { transport })`; tests use `const hub = new LoopbackHub(hostId); hub.connect(memberId)` (one transport per participant; `hub.manual = true` + `hub.flush()` to test concurrent writes; `hub.relayed` shows the opaque bytes the "SFU" saw).
- `sdk.state.get(key)`, `.all()`, `.version`, `.set({k: v, ...})`, `.delete(...keys)`: last-writer-wins per key (higher version, ties to the larger member id), `host:*` keys writable only by the host (guest `set` throws DataError FORBIDDEN, forged ops are dropped on receipt). A late joiner syncs from the host automatically.
- `sdk.input.send(bytes)` guest to host; `sdk.input.reply(memberId, bytes)` host to one member (the hole-cards path).
- `sdk.on('state', e => e.changes[{key,value,by,version,local}])`, `sdk.on('input', e => {from, data, reply})` (reply=true on a member receiving the host's reply). Returns an unsubscribe function.
- `sdk.isHost`. Max envelope 16 KiB (DataError TOO_LARGE).

Status: see sections below (updated as the work lands).
