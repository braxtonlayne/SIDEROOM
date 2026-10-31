// The slice of the Embedded App SDK's data API (ACT-CH-001) the table uses, behind a small interface so the
// table logic runs against the SDK in the client and against an in-memory hub in tests and in `vite` dev.
//
// SDK shape this is written against (packages/embedded-app-sdk/src/data.ts on feat/p36-act-channel):
//   sdk.state.get(key) / sdk.state.set({ key: value })       last writer wins per key, `host:*` host-only
//   sdk.input.send(bytes)                                    guest -> host (a host sending gets it locally)
//   sdk.input.reply(member, bytes)                           host -> one member
//   sdk.on('state', e => ...), sdk.on('input', e => { from, data, reply })
//   sdk.isHost

export interface InputEvent {
  from: string;
  data: Uint8Array;
  /** True for a host reply addressed to this member, false for an input sent to the host. */
  reply: boolean;
}

export interface Transport {
  /** This participant's member id. */
  readonly self: string;
  /** The member who launched the instance: the table's dealer and the only writer of `host:*` keys. */
  readonly host: string;
  getState<T = unknown>(key: string): T | undefined;
  setState(patch: Record<string, unknown>): void;
  onState(cb: () => void): () => void;
  sendToHost(bytes: Uint8Array): void;
  /** Host only. */
  reply(member: string, bytes: Uint8Array): void;
  onInput(cb: (e: InputEvent) => void): () => void;
}

/** The parts of `SideRoomSDK` the adapter touches (structural, so a test double fits). */
export interface SdkLike {
  readonly isHost: boolean;
  readonly state: {
    get<T = unknown>(key: string): T | undefined;
    set(patch: Record<string, unknown>): void;
  };
  readonly input: {
    send(bytes: Uint8Array): void;
    reply(member: string, bytes: Uint8Array): void;
  };
  on(event: 'state', cb: (e: unknown) => void): () => void;
  on(event: 'input', cb: (e: InputEvent) => void): () => void;
}

export function sdkTransport(sdk: SdkLike, self: string, host: string): Transport {
  return {
    self,
    host,
    getState: (key) => sdk.state.get(key),
    setState: (patch) => sdk.state.set(patch),
    onState: (cb) => sdk.on('state', () => cb()),
    sendToHost: (bytes) => sdk.input.send(bytes),
    reply: (member, bytes) => sdk.input.reply(member, bytes),
    onInput: (cb) => sdk.on('input', cb),
  };
}

/**
 * An in-memory room: one shared document and a message pipe, for tests and for `vite` dev without a host.
 * Delivery is queued (like a network) and runs when `flush()` is called or on a microtask via `settle()`.
 */
export class LoopbackHub {
  readonly host: string;
  private readonly doc = new Map<string, unknown>();
  private readonly members = new Map<
    string,
    { state: Set<() => void>; input: Set<(e: InputEvent) => void> }
  >();
  private queue: (() => void)[] = [];
  /** Every `reply` the host sent, as (member, bytes): lets tests prove who was addressed. */
  readonly replies: { to: string; data: Uint8Array }[] = [];
  /** Every state write, as JSON text: lets tests prove what the document ever held. */
  readonly stateWrites: string[] = [];

  constructor(host: string) {
    this.host = host;
  }

  connect(self: string): Transport {
    const mine = { state: new Set<() => void>(), input: new Set<(e: InputEvent) => void>() };
    this.members.set(self, mine);
    const isHost = self === this.host;
    return {
      self,
      host: this.host,
      getState: <T>(key: string) => this.doc.get(key) as T | undefined,
      setState: (patch) => {
        for (const k of Object.keys(patch)) {
          if (k.startsWith('host:') && !isHost) throw new Error(`Only the host may write "${k}"`);
        }
        for (const [k, v] of Object.entries(patch)) {
          this.doc.set(k, structuredClone(v));
          this.stateWrites.push(JSON.stringify(v));
        }
        for (const m of this.members.values()) {
          for (const cb of m.state) this.queue.push(() => cb());
        }
      },
      onState: (cb) => {
        mine.state.add(cb);
        return () => mine.state.delete(cb);
      },
      sendToHost: (bytes) => {
        const host = this.members.get(this.host);
        if (!host) return;
        const copy = bytes.slice();
        for (const cb of host.input)
          this.queue.push(() => cb({ from: self, data: copy.slice(), reply: false }));
      },
      reply: (member, bytes) => {
        if (!isHost) throw new Error('Only the host may reply to a member');
        const copy = bytes.slice();
        this.replies.push({ to: member, data: copy });
        const target = this.members.get(member);
        if (!target) return;
        for (const cb of target.input)
          this.queue.push(() => cb({ from: self, data: copy.slice(), reply: true }));
      },
      onInput: (cb) => {
        mine.input.add(cb);
        return () => mine.input.delete(cb);
      },
    };
  }

  /** Deliver everything queued, including what delivery itself queues. */
  flush(): void {
    let guard = 0;
    while (this.queue.length) {
      if (guard++ > 10_000) throw new Error('loopback does not settle');
      (this.queue.shift() as () => void)();
    }
  }
}
