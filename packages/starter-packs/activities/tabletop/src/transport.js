// The seam between the Activity and the Embedded App SDK's shared state (ACT-CH-001). The app only talks to a
// `Transport`; there are two: a loopback room (tests, and `?loopback` for working on the UI without a call)
// and `sdkTransport`, which maps the Transport onto `sdk.state`, `sdk.input` and `sdk.on(...)`.
//
// Transport:
//   me            { id, name, avatarUrl }
//   hostId        the member who launched the instance (the only writer of `host:*`)
//   isHost        me.id === hostId
//   doc()         a snapshot of the shared document, { key: value }
//   set(key, v)   write a key (`null` deletes); `host:*` only as the host, `ruler:<me.id>` as anyone
//   send(msg)     an intent for the host (the host's own go straight to its handler)
//   onChange(cb)  called with no arguments after the document changed
//   onInput(cb)   host only: cb(fromId, msg) for each intent
//   people()      the members of the instance: [{ id, name, avatarUrl }]
//   close()

const enc = new TextEncoder();
const dec = new TextDecoder();

/** Intents travel as UTF-8 JSON bytes. */
export const encodeInput = (msg) => enc.encode(JSON.stringify(msg));
export function decodeInput(bytes) {
  try {
    const v = JSON.parse(dec.decode(bytes));
    return v && typeof v === 'object' ? v : null;
  } catch {
    return null;
  }
}

/** The write rule the SDK enforces for the document: `host:*` is the host's, `<anything else>:<id>` its member's. */
export function mayWrite(userId, hostId, key) {
  if (key.startsWith('host:')) return userId === hostId;
  return key === `ruler:${userId}`;
}

/** An in-memory room with the SDK's semantics: last-writer-wins per key, host-only `host:*`, input to the host. */
export function createLoopbackRoom(hostId) {
  const doc = {};
  const peers = new Set();
  let version = 0;
  const notify = () => {
    version += 1;
    for (const p of peers) for (const cb of [...p.changeCbs]) cb();
  };
  const room = {
    doc,
    get version() {
      return version;
    },
    join(user) {
      const me = { id: user.id, name: user.name ?? user.id, avatarUrl: user.avatarUrl ?? null };
      const p = {
        me,
        changeCbs: new Set(),
        inputCbs: new Set(),
        hostId,
        get isHost() {
          return me.id === hostId;
        },
        doc: () => JSON.parse(JSON.stringify(doc)),
        set(key, value) {
          if (!mayWrite(me.id, hostId, key)) throw new Error(`not allowed to write ${key}`);
          if (value === null || value === undefined) delete doc[key];
          else doc[key] = JSON.parse(JSON.stringify(value));
          notify();
        },
        send(msg) {
          // Round-trip through bytes as the real channel does.
          const m = decodeInput(encodeInput(msg));
          for (const q of peers) if (q.me.id === hostId) for (const cb of [...q.inputCbs]) cb(me.id, m);
        },
        onChange(cb) {
          p.changeCbs.add(cb);
          return () => p.changeCbs.delete(cb);
        },
        onInput(cb) {
          p.inputCbs.add(cb);
          return () => p.inputCbs.delete(cb);
        },
        people: () => [...peers].map((q) => q.me),
        close() {
          peers.delete(p);
        },
      };
      peers.add(p);
      return p;
    },
  };
  return room;
}

/**
 * Map a Transport onto the SDK. Written against the API of ACT-CH-001 (roadmap-p27 section 14):
 *   sdk.state.get() -> { version, data }       sdk.state.set(key, value)      sdk.on('state', cb)
 *   sdk.input.send(bytes)                      sdk.on('input', cb({ from, data }))
 *   sdk.state.hostId                           the launching member (reconcile with the published stub)
 * `people` is the list from `getInstanceConnectedParticipants` (kept current by the caller).
 */
export function sdkTransport(sdk, { me, hostId, people = () => [] }) {
  const changeCbs = new Set();
  const inputCbs = new Set();
  const offState = sdk.on('state', () => {
    for (const cb of [...changeCbs]) cb();
  });
  const offInput = sdk.on('input', (e) => {
    const msg = decodeInput(e.data);
    if (msg) for (const cb of [...inputCbs]) cb(e.from, msg);
  });
  const t = {
    me,
    hostId,
    isHost: me.id === hostId,
    doc: () => ({ ...(sdk.state.get().data ?? {}) }),
    set(key, value) {
      if (!mayWrite(me.id, hostId, key)) throw new Error(`not allowed to write ${key}`);
      return sdk.state.set(key, value ?? null);
    },
    send(msg) {
      if (t.isHost) {
        // The host is not its own input recipient on the wire; hand it to the handler directly.
        for (const cb of [...inputCbs]) cb(me.id, msg);
        return Promise.resolve();
      }
      return sdk.input.send(encodeInput(msg));
    },
    onChange(cb) {
      changeCbs.add(cb);
      return () => changeCbs.delete(cb);
    },
    onInput(cb) {
      inputCbs.add(cb);
      return () => inputCbs.delete(cb);
    },
    people,
    close() {
      offState?.();
      offInput?.();
    },
  };
  return t;
}

/**
 * Wire the host: apply every intent (own and others') with `handleInput` and write the results. Returns the
 * unsubscribe function. `seed` and `now` are injectable for tests.
 */
export function runHost(transport, handleInput, { seed, now = Date.now } = {}) {
  return transport.onInput((from, msg) => {
    const res = handleInput(transport.doc(), from, msg, {
      hostId: transport.hostId,
      now: now(),
      seed,
    });
    for (const w of res.writes) transport.set(w.key, w.value);
  });
}
