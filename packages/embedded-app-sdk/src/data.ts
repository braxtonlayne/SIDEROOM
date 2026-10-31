// Shared activity state and inputs (ACT-CH-001). The Activity talks to a `DataTransport`; in the client the
// transport is the room's SCTP data channel through the SFU, encrypted by the host page with the room's
// sender key (the SFU relays opaque bytes). In tests the `LoopbackHub` below stands in for the whole path.
//
// Envelope (JSON, UTF-8, before encryption). Every member receives every message the SFU relays within the
// instance; `to` is checked here, after decryption, so the SFU never learns who a message is for.
//   { v: 1, t: 'state', ops: [{ k, d?, ver, by }] }   set (d present) or delete (d absent) of keys
//   { v: 1, t: 'sync' }                                a joiner asks the host for the document
//   { v: 1, t: 'snap', to, ops }                       the host's answer (a full snapshot, tombstones included)
//   { v: 1, t: 'input', to, b }                        guest -> host, `b` is base64 bytes
//   { v: 1, t: 'reply', to, b }                        host -> one member

/** Largest envelope the data channel carries; above this `set` and `input.send` throw. */
export const MAX_DATA_BYTES = 16 * 1024;
/** How long a deleted key's tombstone is kept (ms). */
export const TOMBSTONE_TTL_MS = 5 * 60_000;
/** Most tombstones kept at once. */
export const MAX_TOMBSTONES = 1000;
/** Keys only the instance's host may write. */
export const HOST_KEY_PREFIX = 'host:';

/** What an Activity (or the host page) provides: opaque bytes in and out, and who is who. */
export interface DataTransport {
  /** The member id of this participant. */
  readonly self: string;
  /** The member who launched the instance; the only writer of `host:*` keys. */
  readonly host: string;
  /** Send opaque bytes to every other participant of the instance. */
  send(bytes: Uint8Array): void;
  /** Bytes from another participant; `from` is the authenticated sender, never taken from the payload. */
  onMessage(cb: (from: string, bytes: Uint8Array) => void): () => void;
}

export interface StateOp {
  k: string;
  /** Absent for a delete (a tombstone). */
  d?: unknown;
  ver: number;
  by: string;
}

export interface StateChange {
  key: string;
  /** `undefined` after a delete. */
  value: unknown;
  by: string;
  version: number;
  local: boolean;
}

export interface StateEvent {
  changes: StateChange[];
  /** The document version after the change (the highest key version). */
  version: number;
}

export interface InputEvent {
  from: string;
  data: Uint8Array;
  /** True for a host reply addressed to this member, false for an input sent to the host. */
  reply: boolean;
}

export type DataEventMap = { state: StateEvent; input: InputEvent };
export type DataEventName = keyof DataEventMap;

export class DataError extends Error {
  readonly code: 'FORBIDDEN' | 'TOO_LARGE' | 'NO_TRANSPORT' | 'INVALID';
  constructor(code: DataError['code'], message: string) {
    super(message);
    this.name = 'DataError';
    this.code = code;
  }
}

const enc = new TextEncoder();
const dec = new TextDecoder();

export function b64(bytes: Uint8Array): string {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}
export function unb64(s: string): Uint8Array {
  const raw = atob(s);
  const out = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
  return out;
}

/** True when `a` beats `b`: higher version wins, ties go to the larger author id. */
function wins(a: { ver: number; by: string }, b: { ver: number; by: string } | undefined): boolean {
  if (!b) return true;
  if (a.ver !== b.ver) return a.ver > b.ver;
  return a.by > b.by;
}

type Listener<K extends DataEventName> = (e: DataEventMap[K]) => void;

/**
 * The shared document and the input pipe of one Activity instance. `sdk.state`, `sdk.input` and
 * `sdk.on('state'|'input')` are views of this object.
 */
export class ActivityData {
  private readonly entries = new Map<string, StateOp>();
  private readonly listeners: { [K in DataEventName]: Set<Listener<K>> } = {
    state: new Set(),
    input: new Set(),
  };
  private transport: DataTransport | null = null;
  private off: (() => void) | null = null;
  /** Highest version of any tombstone already dropped: the document version never goes below it. */
  private floor = 0;
  /** When each tombstone was recorded (local clock), to expire it. */
  private readonly tombAt = new Map<string, number>();
  private readonly now: () => number;

  constructor(opts: { now?: () => number } = {}) {
    this.now = opts.now ?? Date.now;
  }

  /** Attach a transport and ask the host for the current document. */
  attach(transport: DataTransport): void {
    this.detach();
    this.transport = transport;
    this.off = transport.onMessage((from, bytes) => this.receive(from, bytes));
    if (transport.self !== transport.host) this.sendEnvelope({ v: 1, t: 'sync' });
  }

  detach(): void {
    this.off?.();
    this.off = null;
    this.transport = null;
  }

  get self(): string | null {
    return this.transport?.self ?? null;
  }
  get host(): string | null {
    return this.transport?.host ?? null;
  }
  get isHost(): boolean {
    return !!this.transport && this.transport.self === this.transport.host;
  }

  // ---- state ----

  /** The document version: the highest key version seen. 0 for an empty document. */
  get version(): number {
    let v = this.floor;
    for (const e of this.entries.values()) v = Math.max(v, e.ver);
    return v;
  }

  get<T = unknown>(key: string): T | undefined {
    const e = this.entries.get(key);
    return e && 'd' in e ? (e.d as T) : undefined;
  }

  /** A plain copy of the live keys (tombstones omitted). */
  snapshot(): Record<string, unknown> {
    const out: Record<string, unknown> = {};
    for (const [k, e] of this.entries) if ('d' in e) out[k] = e.d;
    return out;
  }

  /** Set one or more keys (last writer wins per key). Applied locally at once, then sent to the instance. */
  set(patch: Record<string, unknown>): void {
    this.write(Object.entries(patch).map(([k, d]) => ({ k, d })));
  }

  /** Delete keys. */
  delete(...keys: string[]): void {
    this.write(keys.map((k) => ({ k })));
  }

  private write(items: { k: string; d?: unknown }[]): void {
    const t = this.requireTransport();
    if (items.length === 0) return;
    for (const { k } of items) {
      if (k.startsWith(HOST_KEY_PREFIX) && t.self !== t.host) {
        throw new DataError('FORBIDDEN', `Only the host may write "${k}".`);
      }
    }
    const base = this.version;
    const ops: StateOp[] = items.map(({ k, d }) =>
      d === undefined ? { k, ver: base + 1, by: t.self } : { k, d, ver: base + 1, by: t.self },
    );
    // Validate size before touching local state.
    const bytes = enc.encode(JSON.stringify({ v: 1, t: 'state', ops }));
    if (bytes.length > MAX_DATA_BYTES - 256) {
      throw new DataError(
        'TOO_LARGE',
        `State change is ${bytes.length} bytes; the limit is ${MAX_DATA_BYTES}.`,
      );
    }
    this.applyOps(ops, true);
    t.send(bytes);
  }

  // ---- input ----

  /** Guest to host. A host sending to itself gets the event locally. */
  sendInput(data: Uint8Array): void {
    const t = this.requireTransport();
    if (t.self === t.host) {
      this.emit('input', { from: t.self, data: data.slice(), reply: false });
      return;
    }
    this.sendEnvelope({ v: 1, t: 'input', to: t.host, b: b64(data) });
  }

  /** Host to one member (e.g. a player's hole cards). Only the host may reply. */
  reply(member: string, data: Uint8Array): void {
    const t = this.requireTransport();
    if (t.self !== t.host) throw new DataError('FORBIDDEN', 'Only the host may reply to a member.');
    if (member === t.self) {
      this.emit('input', { from: t.self, data: data.slice(), reply: true });
      return;
    }
    this.sendEnvelope({ v: 1, t: 'reply', to: member, b: b64(data) });
  }

  // ---- events ----

  on<K extends DataEventName>(event: K, cb: Listener<K>): () => void {
    this.listeners[event].add(cb as never);
    return () => this.listeners[event].delete(cb as never);
  }

  private emit<K extends DataEventName>(event: K, e: DataEventMap[K]): void {
    for (const l of [...this.listeners[event]]) (l as Listener<K>)(e);
  }

  // ---- wire ----

  private requireTransport(): DataTransport {
    if (!this.transport) throw new DataError('NO_TRANSPORT', 'No data channel is attached.');
    return this.transport;
  }

  private sendEnvelope(env: Record<string, unknown>): void {
    const bytes = enc.encode(JSON.stringify(env));
    if (bytes.length > MAX_DATA_BYTES) {
      throw new DataError(
        'TOO_LARGE',
        `Message is ${bytes.length} bytes; the limit is ${MAX_DATA_BYTES}.`,
      );
    }
    this.requireTransport().send(bytes);
  }

  private applyOps(ops: StateOp[], local: boolean): void {
    const changes: StateChange[] = [];
    for (const op of ops) {
      if (!wins(op, this.entries.get(op.k))) continue;
      this.entries.set(op.k, op);
      if ('d' in op) this.tombAt.delete(op.k);
      else this.tombAt.set(op.k, this.now());
      changes.push({
        key: op.k,
        value: 'd' in op ? op.d : undefined,
        by: op.by,
        version: op.ver,
        local,
      });
    }
    this.compact();
    if (changes.length) this.emit('state', { changes, version: this.version });
  }

  /**
   * Drop tombstones older than [`TOMBSTONE_TTL_MS`] (and the oldest beyond [`MAX_TOMBSTONES`]). A tombstone
   * only has to outlive the delivery of a concurrent stale write, so minutes are plenty. The version it
   * carried is kept in `floor`, so later writes still outrank it.
   */
  compact(): void {
    if (this.tombAt.size === 0) return;
    const cutoff = this.now() - TOMBSTONE_TTL_MS;
    const stale = [...this.tombAt].filter(([, at]) => at < cutoff).map(([k]) => k);
    if (this.tombAt.size - stale.length > MAX_TOMBSTONES) {
      const rest = [...this.tombAt].filter(([k]) => !stale.includes(k)).sort((a, b) => a[1] - b[1]);
      for (const [k] of rest.slice(0, rest.length - MAX_TOMBSTONES)) stale.push(k);
    }
    for (const k of stale) {
      const e = this.entries.get(k);
      if (e) this.floor = Math.max(this.floor, e.ver);
      this.entries.delete(k);
      this.tombAt.delete(k);
    }
  }

  /** The live entries as snapshot envelopes, each under the size limit (late join of a big document). */
  private snapshotEnvelopes(to: string): Record<string, unknown>[] {
    this.compact();
    const limit = MAX_DATA_BYTES - 256;
    const out: Record<string, unknown>[] = [];
    let batch: StateOp[] = [];
    const size = (ops: StateOp[]) =>
      enc.encode(JSON.stringify({ v: 1, t: 'snap', to, floor: this.version, ops })).length;
    for (const e of this.entries.values()) {
      if (!('d' in e)) continue; // tombstones are not sent; `floor` carries their version
      if (batch.length && size([...batch, e]) > limit) {
        out.push({ v: 1, t: 'snap', to, floor: this.version, ops: batch });
        batch = [];
      }
      batch.push(e);
    }
    // Always send at least one envelope so the joiner learns the floor.
    out.push({ v: 1, t: 'snap', to, floor: this.version, ops: batch });
    return out;
  }

  private receive(from: string, bytes: Uint8Array): void {
    const t = this.transport;
    if (!t || from === t.self || bytes.length > MAX_DATA_BYTES) return;
    let env: Record<string, unknown>;
    try {
      env = JSON.parse(dec.decode(bytes)) as Record<string, unknown>;
    } catch {
      return;
    }
    if (typeof env !== 'object' || env === null || env.v !== 1) return;
    const forMe = env.to === undefined || env.to === t.self;
    switch (env.t) {
      case 'state':
      case 'snap': {
        if (!forMe) return;
        if (env.t === 'snap' && from === t.host && typeof env.floor === 'number') {
          if (Number.isFinite(env.floor)) this.floor = Math.max(this.floor, env.floor);
        }
        this.applyOps(this.cleanOps(env.ops, from, t, env.t === 'snap'), false);
        return;
      }
      case 'sync':
        if (t.self === t.host) {
          for (const env of this.snapshotEnvelopes(from)) this.sendEnvelope(env);
        }
        return;
      case 'input':
        // Only the host consumes inputs.
        if (t.self === t.host && forMe && typeof env.b === 'string') {
          this.emit('input', { from, data: unb64(env.b), reply: false });
        }
        return;
      case 'reply':
        // A reply is honoured only from the host.
        if (from === t.host && forMe && typeof env.b === 'string') {
          this.emit('input', { from, data: unb64(env.b), reply: true });
        }
        return;
    }
  }

  /**
   * Validate ops from `from`. A normal change must carry `by === from` (the sender cannot forge an author);
   * a snapshot is accepted only from the host and keeps each op's author. `host:*` keys are accepted only
   * when the original author is the host.
   */
  private cleanOps(raw: unknown, from: string, t: DataTransport, snapshot: boolean): StateOp[] {
    if (!Array.isArray(raw) || (snapshot && from !== t.host)) return [];
    const out: StateOp[] = [];
    for (const o of raw as Record<string, unknown>[]) {
      if (typeof o !== 'object' || o === null) continue;
      const { k, ver, by } = o as { k?: unknown; ver?: unknown; by?: unknown };
      if (typeof k !== 'string' || typeof ver !== 'number' || !Number.isFinite(ver)) continue;
      if (typeof by !== 'string') continue;
      if (!snapshot && by !== from) continue;
      if (k.startsWith(HOST_KEY_PREFIX) && by !== t.host) continue;
      out.push('d' in o ? { k, d: o.d, ver, by } : { k, ver, by });
    }
    return out;
  }
}

/** A loopback "SFU" for tests: every transport sees what the others send, never its own. */
export class LoopbackHub {
  readonly host: string;
  private readonly peers = new Map<string, Set<(from: string, b: Uint8Array) => void>>();
  /** Everything relayed, as opaque bytes, for tests that assert on what the "SFU" saw. */
  readonly relayed: { from: string; bytes: Uint8Array }[] = [];
  /** When set, delivery is queued until `flush()` (to test ordering and concurrent writes). */
  manual = false;
  private queue: (() => void)[] = [];

  constructor(host: string) {
    this.host = host;
  }

  connect(member: string): DataTransport {
    const subs = new Set<(from: string, b: Uint8Array) => void>();
    this.peers.set(member, subs);
    return {
      self: member,
      host: this.host,
      send: (bytes) => {
        const copy = bytes.slice();
        this.relayed.push({ from: member, bytes: copy });
        for (const [id, s] of this.peers) {
          if (id === member) continue;
          const deliver = () => s.forEach((cb) => cb(member, copy.slice()));
          if (this.manual) this.queue.push(deliver);
          else deliver();
        }
      },
      onMessage: (cb) => {
        subs.add(cb);
        return () => subs.delete(cb);
      },
    };
  }

  /** Deliver everything queued while `manual` was set. */
  flush(): void {
    while (this.queue.length) this.queue.shift()?.();
  }

  disconnect(member: string): void {
    this.peers.delete(member);
  }
}
